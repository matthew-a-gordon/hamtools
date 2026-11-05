//! Starting `rigctld` for a serial port, when none is running.
//!
//! This module never opens a serial port. [`scan_ports`] only lists directory entries under
//! `/dev`; the `rigctld` it starts is the one process that opens the port (the project boundary:
//! tools are TCP clients). Which radio is on a port is the operator's choice, because the model
//! and baud come from the profile kind, not from probing a device that could be a GPS dongle.
//! Untested on hardware.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};

use hamtools_config::RigKind;

/// A serial port that could have a radio on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    /// What `rigctld -r` gets: the stable `/dev/serial/by-id` name when there is one.
    pub path: PathBuf,
}

/// Devices that are never a radio, matched against the `/dev/serial/by-id` name (lowercase).
/// The uConsole's own keyboard board shows up as a `ttyACM` port named `usb-ClockworkPI_uConsole_*`.
const NOT_A_RADIO: [&str; 1] = ["clockworkpi"];

/// The ports under `dev` (normally `/dev`): `ttyUSB*` and `ttyACM*`, each under its
/// `serial/by-id` name when it has one, minus devices known not to be radios. Reads directory
/// entries only; opens nothing.
pub fn scan_ports(dev: &Path) -> Vec<Port> {
    let mut by_target: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    let mut excluded: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(dev.join("serial/by-id")) {
        for entry in entries.flatten() {
            let Ok(target) = fs::canonicalize(entry.path()) else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if NOT_A_RADIO.iter().any(|n| name.contains(n)) {
                excluded.push(target.clone());
            }
            by_target.insert(target, entry.path());
        }
    }
    let mut ports = Vec::new();
    if let Ok(entries) = fs::read_dir(dev) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !(name.starts_with("ttyUSB") || name.starts_with("ttyACM")) {
                continue;
            }
            let plain = entry.path();
            let canonical = fs::canonicalize(&plain).unwrap_or_else(|_| plain.clone());
            if excluded.contains(&canonical) {
                continue;
            }
            ports.push(Port {
                path: by_target.get(&canonical).cloned().unwrap_or(plain),
            });
        }
    }
    ports.sort_by(|a, b| a.path.cmp(&b.path));
    ports
}

/// Model and baud for a profile kind, from what was verified on the radios (see CLAUDE.md).
/// `None` for a kind nothing is known about: a generic rig cannot be started blind.
pub fn model_and_baud(kind: RigKind) -> Option<(u32, u32)> {
    match kind {
        RigKind::Kx3 => Some((2045, 38_400)),
        RigKind::Ft847 => Some((1001, 9_600)),
        RigKind::Generic => None,
    }
}

/// The TCP port of a `host:port` address when the host is this machine: a daemon started here
/// can only answer for a loopback address.
pub fn local_tcp_port(addr: &str) -> Option<u16> {
    let (host, port) = addr.rsplit_once(':')?;
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
        .then(|| port.parse().ok())
        .flatten()
}

/// One `rigctld` to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    /// The profile it serves (index into the app's rigs) and that profile's name.
    pub rig: usize,
    pub name: String,
    pub kind: RigKind,
    pub port: PathBuf,
    pub tcp: u16,
}

impl Launch {
    /// `rigctld`'s arguments, or `None` for a kind with no known model.
    pub fn args(&self) -> Option<Vec<String>> {
        let (model, baud) = model_and_baud(self.kind)?;
        Some(vec![
            "-m".into(),
            model.to_string(),
            "-s".into(),
            baud.to_string(),
            "-r".into(),
            self.port.display().to_string(),
            "-t".into(),
            self.tcp.to_string(),
        ])
    }
}

struct Running {
    launch: Launch,
    child: Child,
    /// What `rigctld` prints, line by line, from a reader thread (so the pipe never fills).
    lines: Receiver<String>,
    /// The last few lines, for the message.
    recent: Vec<String>,
}

/// The daemons this process started. They are children of the app: dropping this kills them, so
/// nothing is left holding a serial port.
#[derive(Default)]
pub struct Daemons {
    running: Vec<Running>,
    /// The executable to run; `rigctld` unless a test says otherwise.
    program: Option<std::ffi::OsString>,
}

impl Daemons {
    #[cfg(test)]
    fn with_program(program: impl Into<std::ffi::OsString>) -> Self {
        Self {
            running: Vec::new(),
            program: Some(program.into()),
        }
    }

    /// Ports a daemon of ours already holds.
    pub fn ports_in_use(&self) -> Vec<PathBuf> {
        self.running.iter().map(|r| r.launch.port.clone()).collect()
    }

    /// Start `rigctld` for `launch`. The message is for the status line either way.
    pub fn start(&mut self, launch: Launch) -> Result<String, String> {
        let args = launch
            .args()
            .ok_or_else(|| format!("no known rigctld model for a {} rig", launch.kind.as_str()))?;
        if self
            .running
            .iter()
            .any(|r| r.launch.port == launch.port || r.launch.tcp == launch.tcp)
        {
            return Err(format!(
                "a rigctld of ours already uses {} or TCP port {}",
                launch.port.display(),
                launch.tcp
            ));
        }
        let mut child = Command::new(self.program.as_deref().unwrap_or("rigctld".as_ref()))
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| match e.kind() {
                ErrorKind::NotFound => {
                    "rigctld not found in PATH (Debian/Raspberry Pi OS: apt install libhamlib-utils)"
                        .to_string()
                }
                _ => format!("could not start rigctld: {e}"),
            })?;
        let message = format!(
            "started rigctld for {} on {} (TCP {}); connecting...",
            launch.name,
            launch.port.display(),
            launch.tcp
        );
        let (tx, lines) = mpsc::channel();
        if let Some(err) = child.stderr.take() {
            // Bytes, not `lines()`: hamlib's output is not always valid UTF-8, and a decode
            // error would end the reader and hide every later line.
            std::thread::spawn(move || {
                let mut reader = BufReader::new(err);
                let mut buf = Vec::new();
                while matches!(reader.read_until(b'\n', &mut buf), Ok(n) if n > 0) {
                    let line = String::from_utf8_lossy(&buf).into_owned();
                    buf.clear();
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
        }
        self.running.push(Running {
            launch,
            child,
            lines,
            recent: Vec::new(),
        });
        Ok(message)
    }

    /// Daemons that have exited since the last call, each as a status message with the last
    /// lines `rigctld` printed (usually why). `rigctld` block-buffers its output when piped, so
    /// those lines only arrive when it exits. One that cannot open its port (hamlib 4.7.2)
    /// does not exit: it keeps running with no rig, and the strip then shows rig errors.
    pub fn poll(&mut self) -> Vec<String> {
        let mut gone = Vec::new();
        self.running.retain_mut(|r| {
            let state = match r.child.try_wait() {
                Ok(None) => return true,
                Ok(Some(status)) => format!("exited ({status})"),
                Err(_) => "is gone".to_string(),
            };
            // The reader thread may be a moment behind the exit.
            std::thread::sleep(std::time::Duration::from_millis(50));
            while let Ok(line) = r.lines.try_recv() {
                if !line.trim().is_empty() {
                    r.recent.push(line.trim().to_string());
                }
            }
            let skip = r.recent.len().saturating_sub(3);
            let why = r.recent[skip..].join(" ");
            let port = r.launch.port.display();
            gone.push(if why.is_empty() {
                format!("rigctld on {port} {state}")
            } else {
                format!("rigctld on {port} {state}: {why}")
            });
            false
        });
        gone
    }
}

impl Drop for Daemons {
    fn drop(&mut self) {
        for r in &mut self.running {
            let _ = r.child.kill();
            let _ = r.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rig-tui-daemon-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scan_lists_usb_and_acm_ports_only() {
        let dev = scratch("scan");
        for name in ["ttyUSB1", "ttyUSB0", "ttyACM0", "ttyS0", "null"] {
            fs::write(dev.join(name), "").unwrap();
        }
        let ports = scan_ports(&dev);
        let names: Vec<_> = ports
            .iter()
            .map(|p| p.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["ttyACM0", "ttyUSB0", "ttyUSB1"]);
        let _ = fs::remove_dir_all(dev);
    }

    #[test]
    fn scan_prefers_the_by_id_name() {
        let dev = scratch("byid");
        fs::write(dev.join("ttyUSB0"), "").unwrap();
        fs::write(dev.join("ttyUSB1"), "").unwrap();
        fs::create_dir_all(dev.join("serial/by-id")).unwrap();
        symlink("../../ttyUSB0", dev.join("serial/by-id/usb-FTDI_kx3")).unwrap();
        let ports = scan_ports(&dev);
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].path, dev.join("serial/by-id/usb-FTDI_kx3"));
        assert_eq!(ports[1].path, dev.join("ttyUSB1"));
        let _ = fs::remove_dir_all(dev);
    }

    #[test]
    fn scan_skips_the_uconsoles_own_keyboard_port() {
        let dev = scratch("uconsole");
        fs::write(dev.join("ttyACM0"), "").unwrap();
        fs::write(dev.join("ttyUSB0"), "").unwrap();
        fs::create_dir_all(dev.join("serial/by-id")).unwrap();
        symlink(
            "../../ttyACM0",
            dev.join("serial/by-id/usb-ClockworkPI_uConsole_20230713-if01"),
        )
        .unwrap();
        let ports = scan_ports(&dev);
        assert_eq!(
            ports,
            [Port {
                path: dev.join("ttyUSB0")
            }]
        );
        let _ = fs::remove_dir_all(dev);
    }

    #[test]
    fn scan_of_a_missing_dir_is_empty() {
        assert!(scan_ports(Path::new("/nonexistent/rig-tui")).is_empty());
    }

    #[test]
    fn args_use_the_verified_model_and_baud() {
        let launch = |kind, tcp| Launch {
            rig: 0,
            name: "r".into(),
            kind,
            port: "/dev/ttyUSB0".into(),
            tcp,
        };
        assert_eq!(
            launch(RigKind::Kx3, 4532).args().unwrap().join(" "),
            "-m 2045 -s 38400 -r /dev/ttyUSB0 -t 4532"
        );
        assert_eq!(
            launch(RigKind::Ft847, 4533).args().unwrap().join(" "),
            "-m 1001 -s 9600 -r /dev/ttyUSB0 -t 4533"
        );
        assert!(launch(RigKind::Generic, 4534).args().is_none());
    }

    #[test]
    fn only_loopback_addresses_can_be_served_here() {
        assert_eq!(local_tcp_port("127.0.0.1:4532"), Some(4532));
        assert_eq!(local_tcp_port("localhost:4533"), Some(4533));
        assert_eq!(local_tcp_port("shack.lan:4532"), None);
        assert_eq!(local_tcp_port("127.0.0.1"), None);
    }

    fn launch(port: &str, tcp: u16) -> Launch {
        Launch {
            rig: 0,
            name: "kx3".into(),
            kind: RigKind::Kx3,
            port: port.into(),
            tcp,
        }
    }

    #[test]
    fn a_daemon_that_exits_reports_what_it_printed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("exit");
        let script = dir.join("fake-rigctld");
        fs::write(
            &script,
            "#!/bin/sh\necho 'IO error' >&2\necho 'no such port' >&2\nexit 3\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let mut daemons = Daemons::with_program(script);
        assert!(daemons.start(launch("/dev/ttyUSB0", 4532)).is_ok());
        assert!(
            daemons.start(launch("/dev/ttyUSB0", 4533)).is_err(),
            "the same port twice is refused"
        );
        let mut gone = Vec::new();
        for _ in 0..50 {
            gone = daemons.poll();
            if !gone.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert_eq!(gone.len(), 1, "{gone:?}");
        assert!(gone[0].contains("/dev/ttyUSB0"), "{gone:?}");
        assert!(gone[0].contains("exit status: 3"), "{gone:?}");
        assert!(gone[0].contains("IO error no such port"), "{gone:?}");
        assert!(daemons.ports_in_use().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_program_says_where_to_get_it() {
        let mut daemons = Daemons::with_program("/nonexistent/rigctld");
        let err = daemons.start(launch("/dev/ttyUSB0", 4532)).unwrap_err();
        assert!(err.contains("libhamlib-utils"), "{err}");
    }
}
