//! The operator's position from gpsd: a TCP client of gpsd's JSON protocol, never the GPS's
//! serial port (only daemons open serial ports).
//!
//! `main` runs [`watch_in_background`] on its own thread and hands each change to `App`, the same
//! shape as the address lookup, so `App` stays free of threads and sockets. When gpsd is not
//! running or has no device, the state is [`Gps::Absent`] and the app behaves as if there were no
//! GPS; the thread keeps retrying.
//!
//! Privacy: latitude and longitude are shown in the strip and turned into a Locator on request;
//! they are never written to the config or the Log (only the 6-character Locator is).
//!
//! Untested on hardware beyond reading gpsd's replies.

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use logbook::Grid;
use serde_json::Value;

/// gpsd's default address.
pub const DEFAULT_ADDR: &str = "127.0.0.1:2947";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// gpsd reports about once a second; this long without a position report means no fix.
const STALE_AFTER: Duration = Duration::from_secs(5);
const RETRY_AFTER: Duration = Duration::from_secs(10);
const WATCH: &[u8] = b"?WATCH={\"enable\":true,\"json\":true};\n";

/// A position fix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {
    pub lat_deg: f64,
    pub lon_deg: f64,
    /// Metres above mean sea level, when gpsd gives it (3D fixes).
    pub alt_m: Option<f64>,
    /// gpsd's fix mode: 2 (2D) or 3 (3D).
    pub mode: u8,
}

impl Fix {
    /// The 6-character Locator the fix lies in.
    pub fn locator(&self) -> Grid {
        Grid::at(self.lat_deg, self.lon_deg)
    }
}

/// What gpsd says about the position.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Gps {
    /// No gpsd, or gpsd has no GPS device.
    #[default]
    Absent,
    /// A GPS device is attached but has no fix.
    NoFix,
    Fix(Fix),
}

impl Gps {
    pub fn fix(&self) -> Option<&Fix> {
        match self {
            Gps::Fix(fix) => Some(fix),
            _ => None,
        }
    }
}

/// Folds gpsd's JSON reports into a [`Gps`]. Pure; the socket is [`watch_in_background`]'s.
#[derive(Debug, Default)]
pub struct Reports {
    /// Paths of the devices gpsd has active.
    devices: Vec<String>,
    fix: Option<Fix>,
    /// When the last position report (`TPV`) came.
    last_tpv: Option<Instant>,
}

impl Reports {
    /// Take one line of gpsd's output, received at `now`.
    pub fn line(&mut self, line: &str, now: Instant) {
        let Ok(report) = serde_json::from_str::<Value>(line) else {
            return;
        };
        match report.get("class").and_then(Value::as_str) {
            Some("DEVICES") => {
                self.devices = report
                    .get("devices")
                    .and_then(Value::as_array)
                    .map(|list| list.iter().filter_map(device_path).collect())
                    .unwrap_or_default();
            }
            Some("DEVICE") => {
                let Some(path) = device_path(&report) else {
                    return;
                };
                // `"activated": 0` means the device went away.
                let gone = report.get("activated").and_then(Value::as_f64) == Some(0.0);
                self.devices.retain(|p| *p != path);
                if !gone {
                    self.devices.push(path);
                }
            }
            Some("TPV") => {
                self.last_tpv = Some(now);
                if let Some(path) = device_path(&report)
                    && !self.devices.contains(&path)
                {
                    self.devices.push(path);
                }
                self.fix = tpv_fix(&report);
            }
            _ => {}
        }
    }

    /// The state at `now`: a fix older than [`STALE_AFTER`] no longer counts.
    pub fn state(&self, now: Instant) -> Gps {
        if self.devices.is_empty() {
            return Gps::Absent;
        }
        let fresh = self
            .last_tpv
            .is_some_and(|at| now.saturating_duration_since(at) <= STALE_AFTER);
        match self.fix {
            Some(fix) if fresh => Gps::Fix(fix),
            _ => Gps::NoFix,
        }
    }
}

fn device_path(report: &Value) -> Option<String> {
    report
        .get("path")
        .or_else(|| report.get("device"))
        .and_then(Value::as_str)
        .map(String::from)
}

fn tpv_fix(report: &Value) -> Option<Fix> {
    let mode = report.get("mode").and_then(Value::as_u64)?;
    if mode < 2 {
        return None;
    }
    let lat_deg = report.get("lat").and_then(Value::as_f64)?;
    let lon_deg = report.get("lon").and_then(Value::as_f64)?;
    if !(-90.0..=90.0).contains(&lat_deg) || !(-180.0..=180.0).contains(&lon_deg) {
        return None;
    }
    let alt_m = report
        .get("altMSL")
        .or_else(|| report.get("alt"))
        .and_then(Value::as_f64)
        .filter(|_| mode >= 3);
    Some(Fix {
        lat_deg,
        lon_deg,
        alt_m,
        mode: mode as u8,
    })
}

/// Watch gpsd at `addr` on a new thread, sending each change of [`Gps`] (the first state
/// included). The thread ends when the receiver is dropped.
pub fn watch_in_background(addr: String, tx: Sender<Gps>) {
    thread::spawn(move || {
        let mut last = None;
        let mut send = |state: Gps| {
            if last != Some(state) {
                last = Some(state);
                tx.send(state).is_ok()
            } else {
                true
            }
        };
        loop {
            match connect(&addr) {
                Ok(stream) => {
                    if !session(stream, &mut send) {
                        return;
                    }
                }
                Err(_) => {
                    if !send(Gps::Absent) {
                        return;
                    }
                }
            }
            if !send(Gps::Absent) {
                return;
            }
            thread::sleep(RETRY_AFTER);
        }
    });
}

fn connect(addr: &str) -> std::io::Result<TcpStream> {
    let mut last_error = std::io::Error::new(ErrorKind::NotFound, "no address");
    for sock in addr.to_socket_addrs()? {
        match TcpStream::connect_timeout(&sock, CONNECT_TIMEOUT) {
            Ok(stream) => return Ok(stream),
            Err(e) => last_error = e,
        }
    }
    Err(last_error)
}

/// One connection: watch until it fails. Returns false when the receiver has gone.
fn session(mut stream: TcpStream, send: &mut impl FnMut(Gps) -> bool) -> bool {
    // A short read timeout lets a fix go stale while gpsd is silent.
    if stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .is_err()
        || stream.write_all(WATCH).is_err()
    {
        return true;
    }
    let mut reader = BufReader::new(stream);
    let mut reports = Reports::default();
    let mut line = String::new();
    loop {
        match reader.read_line(&mut line) {
            Ok(0) => return true,
            Ok(_) => {
                reports.line(line.trim_end(), Instant::now());
                line.clear();
            }
            // A timeout may leave half a line in `line`; the next read appends the rest.
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return true,
        }
        if !send(reports.state(Instant::now())) {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;
    use std::sync::mpsc;

    // Town-level Middlebury, IN only.
    const TPV_3D: &str =
        r#"{"class":"TPV","device":"/dev/ttyS0","mode":3,"lat":41.67,"lon":-85.71,"altMSL":250.0}"#;

    #[test]
    fn no_device_is_absent() {
        let mut r = Reports::default();
        let now = Instant::now();
        r.line(r#"{"class":"VERSION","release":"3.25"}"#, now);
        r.line(r#"{"class":"DEVICES","devices":[]}"#, now);
        assert_eq!(r.state(now), Gps::Absent);
    }

    #[test]
    fn device_without_fix_is_no_fix() {
        let mut r = Reports::default();
        let now = Instant::now();
        r.line(
            r#"{"class":"DEVICES","devices":[{"class":"DEVICE","path":"/dev/ttyS0"}]}"#,
            now,
        );
        assert_eq!(r.state(now), Gps::NoFix);
        r.line(r#"{"class":"TPV","device":"/dev/ttyS0","mode":1}"#, now);
        assert_eq!(r.state(now), Gps::NoFix);
    }

    #[test]
    fn three_d_fix_gives_position_altitude_and_locator() {
        let mut r = Reports::default();
        let now = Instant::now();
        r.line(TPV_3D, now);
        let Gps::Fix(fix) = r.state(now) else {
            panic!("expected a fix");
        };
        assert_eq!((fix.lat_deg, fix.lon_deg, fix.mode), (41.67, -85.71, 3));
        assert_eq!(fix.alt_m, Some(250.0));
        assert_eq!(fix.locator().as_str(), "EN71DQ");
    }

    #[test]
    fn two_d_fix_has_no_altitude() {
        let mut r = Reports::default();
        let now = Instant::now();
        r.line(
            r#"{"class":"TPV","device":"/dev/ttyS0","mode":2,"lat":41.67,"lon":-85.71,"alt":9.0}"#,
            now,
        );
        assert_eq!(r.state(now).fix().map(|f| f.alt_m), Some(None));
    }

    #[test]
    fn fix_goes_stale() {
        let mut r = Reports::default();
        let then = Instant::now();
        r.line(TPV_3D, then);
        assert_eq!(r.state(then + STALE_AFTER * 2), Gps::NoFix);
    }

    #[test]
    fn device_removal_makes_it_absent() {
        let mut r = Reports::default();
        let now = Instant::now();
        r.line(TPV_3D, now);
        r.line(
            r#"{"class":"DEVICE","path":"/dev/ttyS0","activated":0}"#,
            now,
        );
        assert_eq!(r.state(now), Gps::Absent);
    }

    #[test]
    fn junk_and_out_of_range_are_ignored() {
        let mut r = Reports::default();
        let now = Instant::now();
        r.line("not json", now);
        r.line(
            r#"{"class":"TPV","device":"/dev/ttyS0","mode":3,"lat":91.0,"lon":0.0}"#,
            now,
        );
        assert_eq!(r.state(now), Gps::NoFix);
    }

    #[test]
    fn watches_a_fake_gpsd() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = [0u8; 64];
            let n = sock.read(&mut buf).unwrap();
            assert!(buf[..n].starts_with(b"?WATCH="));
            sock.write_all(b"{\"class\":\"DEVICES\",\"devices\":[{\"path\":\"/dev/ttyS0\"}]}\n")
                .unwrap();
            sock.write_all(format!("{TPV_3D}\n").as_bytes()).unwrap();
            thread::sleep(Duration::from_millis(300));
        });
        let (tx, rx) = mpsc::channel();
        watch_in_background(addr, tx);
        let mut seen = Vec::new();
        while let Ok(state) = rx.recv_timeout(Duration::from_secs(3)) {
            seen.push(state);
            if matches!(state, Gps::Fix(_)) {
                break;
            }
        }
        assert!(
            seen.iter()
                .any(|s| matches!(s, Gps::Fix(f) if f.locator().as_str() == "EN71DQ"))
        );
    }
}
