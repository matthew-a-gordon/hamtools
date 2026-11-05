//! The Activation tracker: a query over the Log, never stored state.
//!
//! POTA counts 10 Contacts for a park on one UTC day, excluding duplicates by call, band and mode;
//! SOTA counts 4 distinct stations for a summit. An n-fer is one Contact listing several parks
//! (`MY_SIG_INFO=US-1,US-2`) and counts toward each. Warnings are advice and never block a save.
//! Nothing here does I/O: the host reads the Log and passes the records in.

use chrono::NaiveDate;
use logbook::Record;

/// The Contacts a POTA activation needs for one park.
pub const POTA_TARGET: usize = 10;
/// The distinct stations a SOTA activation needs.
pub const SOTA_TARGET: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Program {
    Pota,
    Sota,
}

impl Program {
    pub fn label(self) -> &'static str {
        match self {
            Program::Pota => "POTA",
            Program::Sota => "SOTA",
        }
    }

    pub fn target(self) -> usize {
        match self {
            Program::Pota => POTA_TARGET,
            Program::Sota => SOTA_TARGET,
        }
    }
}

/// One counted station, kept to warn about a duplicate while the next call is typed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Worked {
    call: String,
    band: String,
    mode: String,
}

/// How far one reference has got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub reference: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tracker {
    pub program: Program,
    pub progress: Vec<Progress>,
    /// Contacts on the counted day for the plan's references, for duplicate warnings.
    worked: Vec<Worked>,
}

fn upper(s: Option<&str>) -> String {
    s.unwrap_or_default().trim().to_ascii_uppercase()
}

/// The references a record lists as the activator's own, for `program`.
pub(crate) fn my_refs(program: Program, record: &Record) -> Vec<String> {
    match program {
        Program::Pota => {
            if !upper(record.get("MY_SIG")).eq("POTA") {
                return Vec::new();
            }
            upper(record.get("MY_SIG_INFO"))
                .split(',')
                .map(|r| r.trim().to_string())
                .filter(|r| !r.is_empty())
                .collect()
        }
        Program::Sota => {
            let r = upper(record.get("MY_SOTA_REF"));
            if r.is_empty() { Vec::new() } else { vec![r] }
        }
    }
}

impl Tracker {
    /// Query `records` for the plan's `refs` on the UTC `day`.
    pub fn query(program: Program, refs: &[String], day: NaiveDate, records: &[Record]) -> Self {
        let refs: Vec<String> = refs.iter().map(|r| r.trim().to_ascii_uppercase()).collect();
        let mut seen: Vec<Vec<Worked>> = vec![Vec::new(); refs.len()];
        for record in records {
            if record.start().map(|s| s.date_naive()) != Some(day) {
                continue;
            }
            let listed = my_refs(program, record);
            let worked = Worked {
                call: upper(record.call()),
                band: upper(record.band()),
                mode: upper(record.mode()),
            };
            for (i, r) in refs.iter().enumerate() {
                if !listed.contains(r) {
                    continue;
                }
                let duplicate = seen[i].iter().any(|w| match program {
                    Program::Pota => *w == worked,
                    Program::Sota => w.call == worked.call,
                });
                if !duplicate {
                    seen[i].push(worked.clone());
                }
            }
        }
        let progress = refs
            .iter()
            .zip(&seen)
            .map(|(r, s)| Progress {
                reference: r.clone(),
                count: s.len(),
            })
            .collect();
        let mut worked: Vec<Worked> = Vec::new();
        for s in seen {
            for w in s {
                if !worked.contains(&w) {
                    worked.push(w);
                }
            }
        }
        Self {
            program,
            progress,
            worked,
        }
    }

    /// `POTA US-4571: 6 of 10`, or `SOTA W7W/KG-001: 3 of 4 distinct`. A met target says so in
    /// words as well as in the count.
    pub fn row(&self) -> String {
        let target = self.program.target();
        let parts: Vec<String> = self
            .progress
            .iter()
            .map(|p| {
                let distinct = if self.program == Program::Sota {
                    " distinct"
                } else {
                    ""
                };
                let done = if p.count >= target {
                    " (qualified)"
                } else {
                    ""
                };
                format!("{}: {} of {target}{distinct}{done}", p.reference, p.count)
            })
            .collect();
        format!("{} {}", self.program.label(), parts.join(", "))
    }

    /// The soft warning for a Contact about to be logged, if it repeats one already counted.
    pub fn duplicate_warning(&self, call: &str, band: Option<&str>, mode: &str) -> Option<String> {
        let call = call.trim().to_ascii_uppercase();
        if call.is_empty() {
            return None;
        }
        let band = upper(band);
        let mode = mode.trim().to_ascii_uppercase();
        let hit = self.worked.iter().any(|w| match self.program {
            Program::Pota => w.call == call && w.band == band && w.mode == mode,
            Program::Sota => w.call == call,
        });
        hit.then(|| match self.program {
            Program::Pota => {
                let band = band.to_ascii_lowercase();
                format!("duplicate: {call} already logged on {band} {mode} today")
            }
            Program::Sota => format!("duplicate: {call} already counted for this summit"),
        })
    }
}

/// The warning for a plan whose UTC date is not today's: after midnight UTC an activation that
/// began on the plan's date continues on a new day, which the tracker (and POTA) count afresh.
pub fn date_warning(plan_date: &str, today: NaiveDate, began_on_plan_date: bool) -> Option<String> {
    let today_text = today.format("%Y-%m-%d").to_string();
    if plan_date == today_text {
        None
    } else if began_on_plan_date {
        Some(format!(
            "UTC date rolled over: plan {plan_date}, now {today_text}; the tracker counts {today_text} only"
        ))
    } else {
        Some(format!(
            "plan date {plan_date} is not today ({today_text} UTC)"
        ))
    }
}

/// Whether any record counted for `program` and `refs` started on the UTC `day`.
pub fn any_on(program: Program, refs: &[String], day: NaiveDate, records: &[Record]) -> bool {
    let refs: Vec<String> = refs.iter().map(|r| r.trim().to_ascii_uppercase()).collect();
    records.iter().any(|r| {
        r.start().map(|s| s.date_naive()) == Some(day)
            && my_refs(program, r).iter().any(|m| refs.contains(m))
    })
}

/// The centre of a 4 or 6 character Maidenhead Locator as (latitude, longitude) in degrees.
pub fn grid_center(grid: &str) -> Option<(f64, f64)> {
    let g: Vec<char> = grid.trim().to_ascii_uppercase().chars().collect();
    if g.len() != 4 && g.len() != 6 {
        return None;
    }
    let field = |c: char| (('A'..='R').contains(&c)).then(|| f64::from(c as u8 - b'A'));
    let digit = |c: char| c.to_digit(10).map(f64::from);
    let sub = |c: char| (('A'..='X').contains(&c)).then(|| f64::from(c as u8 - b'A'));
    let mut lon = -180.0 + 20.0 * field(g[0])? + 2.0 * digit(g[2])?;
    let mut lat = -90.0 + 10.0 * field(g[1])? + digit(g[3])?;
    if g.len() == 6 {
        lon += sub(g[4])? / 12.0 + 1.0 / 24.0;
        lat += sub(g[5])? / 24.0 + 1.0 / 48.0;
    } else {
        lon += 1.0;
        lat += 0.5;
    }
    Some((lat, lon))
}

/// Great-circle distance in kilometres.
pub fn distance_km(from: (f64, f64), to: (f64, f64)) -> f64 {
    let (la1, lo1, la2, lo2) = (
        from.0.to_radians(),
        from.1.to_radians(),
        to.0.to_radians(),
        to.1.to_radians(),
    );
    let a = ((la2 - la1) / 2.0).sin().powi(2)
        + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    6371.0 * 2.0 * a.sqrt().asin()
}

/// What the host tells the Activator screen about the active plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanBanner {
    pub program: Program,
    pub references: Vec<String>,
    pub name: Option<String>,
    pub location: Option<String>,
    pub grid: Option<String>,
    pub distance_km: Option<f64>,
    pub alt_m: Option<u32>,
    pub points: Option<u32>,
    pub date: String,
}

impl PlanBanner {
    /// One line: reference, name, location, grid, distance, UTC date, status.
    pub fn line(&self) -> String {
        let mut parts = vec![self.references.join("+")];
        parts.extend(self.name.clone());
        parts.extend(self.location.clone());
        if self.program == Program::Sota {
            parts.extend(self.alt_m.map(|a| format!("{a} m")));
            parts.extend(self.points.map(|p| format!("{p} pts")));
        }
        parts.extend(self.grid.clone());
        parts.extend(self.distance_km.map(|d| format!("{d:.0} km")));
        parts.push(self.date.clone());
        parts.push("active".to_string());
        parts.join("  ")
    }
}

/// Everything the Activator screen shows that comes from the plan and the Log.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivationView {
    pub banner: Option<PlanBanner>,
    pub tracker: Option<Tracker>,
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(fields: &[(&str, &str)]) -> Record {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let adi: String = fields
            .iter()
            .map(|(n, v)| format!("<{n}:{}>{v} ", v.len()))
            .collect();
        let dir = std::env::temp_dir().join(format!(
            "rig-tui-tracker-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.adi");
        std::fs::write(&path, format!("test\n<EOH>\n{adi}<EOR>\n")).unwrap();
        let log = logbook::Log::new(&path, logbook::Callsign::new("K9TST").unwrap());
        let record = log.read().unwrap().remove(0);
        let _ = std::fs::remove_dir_all(&dir);
        record
    }

    fn pota(call: &str, band: &str, mode: &str, date: &str, refs: &str) -> Record {
        record(&[
            ("CALL", call),
            ("BAND", band),
            ("MODE", mode),
            ("QSO_DATE", date),
            ("TIME_ON", "123000"),
            ("MY_SIG", "POTA"),
            ("MY_SIG_INFO", refs),
        ])
    }

    const DAY: (i32, u32, u32) = (2026, 11, 5);

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(DAY.0, DAY.1, DAY.2).unwrap()
    }

    fn refs(r: &[&str]) -> Vec<String> {
        r.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn pota_counts_contacts_for_the_park_on_the_day() {
        let log = vec![
            pota("W1AW", "20m", "CW", "20261105", "US-1"),
            pota("K9XX", "20m", "CW", "20261105", "US-1"),
            pota("N0AA", "20m", "CW", "20261105", "US-2"),
            pota("N0BB", "20m", "CW", "20261104", "US-1"),
        ];
        let t = Tracker::query(Program::Pota, &refs(&["US-1"]), day(), &log);
        assert_eq!(t.row(), "POTA US-1: 2 of 10");
    }

    #[test]
    fn pota_excludes_duplicates_by_call_band_and_mode() {
        let log = vec![
            pota("W1AW", "20m", "CW", "20261105", "US-1"),
            pota("w1aw", "20m", "CW", "20261105", "US-1"),
            pota("W1AW", "40m", "CW", "20261105", "US-1"),
            pota("W1AW", "20m", "SSB", "20261105", "US-1"),
        ];
        let t = Tracker::query(Program::Pota, &refs(&["US-1"]), day(), &log);
        assert_eq!(t.progress[0].count, 3);
    }

    #[test]
    fn an_nfer_counts_toward_each_park() {
        let log = vec![
            pota("W1AW", "20m", "CW", "20261105", "US-1,US-2"),
            pota("K9XX", "20m", "CW", "20261105", "US-2"),
        ];
        let t = Tracker::query(Program::Pota, &refs(&["US-1", "US-2"]), day(), &log);
        assert_eq!(t.row(), "POTA US-1: 1 of 10, US-2: 2 of 10");
    }

    #[test]
    fn a_met_target_is_said_in_words() {
        let log: Vec<Record> = (0..10)
            .map(|i| pota(&format!("W{i}AW"), "20m", "CW", "20261105", "US-1"))
            .collect();
        let t = Tracker::query(Program::Pota, &refs(&["US-1"]), day(), &log);
        assert_eq!(t.row(), "POTA US-1: 10 of 10 (qualified)");
    }

    #[test]
    fn sota_counts_distinct_stations() {
        let sota = |call: &str, band: &str| {
            record(&[
                ("CALL", call),
                ("BAND", band),
                ("MODE", "CW"),
                ("QSO_DATE", "20261105"),
                ("TIME_ON", "1200"),
                ("MY_SOTA_REF", "W7W/KG-001"),
            ])
        };
        let log = vec![
            sota("W1AW", "20m"),
            sota("W1AW", "40m"),
            sota("K9XX", "20m"),
        ];
        let t = Tracker::query(Program::Sota, &refs(&["W7W/KG-001"]), day(), &log);
        assert_eq!(t.row(), "SOTA W7W/KG-001: 2 of 4 distinct");
    }

    #[test]
    fn a_duplicate_warns_softly_and_only_for_the_same_call_band_and_mode() {
        let log = vec![pota("W1AW", "20m", "CW", "20261105", "US-1")];
        let t = Tracker::query(Program::Pota, &refs(&["US-1"]), day(), &log);
        assert!(
            t.duplicate_warning("w1aw", Some("20m"), "CW")
                .unwrap()
                .contains("duplicate")
        );
        assert_eq!(t.duplicate_warning("W1AW", Some("40m"), "CW"), None);
        assert_eq!(t.duplicate_warning("", Some("20m"), "CW"), None);
    }

    #[test]
    fn the_date_warning_tells_a_rollover_from_a_wrong_date() {
        assert_eq!(date_warning("2026-11-05", day(), true), None);
        let w = date_warning("2026-11-04", day(), true).unwrap();
        assert!(w.contains("rolled over") && w.contains("2026-11-05"), "{w}");
        let w = date_warning("2026-11-09", day(), false).unwrap();
        assert!(w.contains("not today"), "{w}");
    }

    #[test]
    fn grid_distance_is_close_to_the_known_value() {
        // EN71 (Middlebury) to EN61 is one field-square west: about 80 km.
        let a = grid_center("EN71DQ").unwrap();
        let b = grid_center("EN61").unwrap();
        let d = distance_km(a, b);
        assert!((60.0..140.0).contains(&d), "{d}");
        assert_eq!(grid_center("ZZ99"), None);
    }
}
