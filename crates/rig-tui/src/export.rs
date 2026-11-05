//! Export of an Activation plan: POTA ADIF and SOTA CSV V2, for upload by hand.
//!
//! Pure except [`write`]: the host reads the Log and passes the records in, [`build`] returns a
//! [`Preview`] of every file with its counts, exclusions and warnings, and nothing touches the
//! disk until [`write`] is called. Export never uploads and never changes a plan or the Log.
//! Every Contact that is left out is named in the preview; warnings advise and never block.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use hamtools_config::{Plan, PlanActivity};
use logbook::Record;

use crate::refdata::parse_locations;
use crate::tracker::{POTA_TARGET, Program, SOTA_TARGET, my_refs};

/// What the preview help says nobody has checked yet. Shown in the modal and kept in step with
/// the ticket notes; the first real upload is the test.
pub const UNVERIFIED: [&str; 6] = [
    "POTA: whether the uploader accepts the fields written here (the API is undocumented)",
    "POTA: a multi-state park is one file with MY_STATE; the -US-MI name suffix is documented for email only",
    "POTA: a slash in a callsign becomes _ in the file name (the docs are silent)",
    "POTA: its exact duplicate rule (call, band, date and mode or submode is assumed)",
    "SOTA: the server's acceptance of a frequency such as 7.032MHz for a band outside its list",
    "SOTA: LF line ends (the page says CR LF; the uploader probably autodetects)",
];

/// Everything [`build`] needs from the host.
pub struct Request<'a> {
    pub plan: &'a Plan,
    /// The operator callsign; names the files and is the fallback station callsign.
    pub operator: &'a str,
    pub records: &'a [Record],
    /// Also export Contacts for the plan's reference made on other UTC dates.
    pub include_other_dates: bool,
    /// A park's cached location (`US-IN`, or `US-IN,US-MI`), if known. Used for `MY_STATE`.
    pub location_of: &'a dyn Fn(&str) -> Option<String>,
    /// Where the operator was for a multi-state park, as a full location (`US-MI`). Ignored
    /// (with a warning) if the park is not in it; without it, the uploader asks.
    pub my_state: Option<&'a str>,
}

/// One file to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportFile {
    pub name: String,
    pub contents: String,
    /// Contacts in the file.
    pub contacts: usize,
    /// The first few Contact lines, for the preview.
    pub head: Vec<String>,
}

/// What would be written, and what would not.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Preview {
    pub files: Vec<ExportFile>,
    /// Contacts selected for export (an n-fer Contact counts once).
    pub selected: usize,
    /// Repeats of an earlier Contact by call, band and mode; still written.
    pub duplicates: Vec<String>,
    /// Contacts for this reference left out, each named with why.
    pub excluded: Vec<String>,
    /// Contacts on the plan's day with no reference of their own: never exported.
    pub no_reference: Vec<String>,
    /// Soft warnings; none blocks.
    pub warnings: Vec<String>,
}

fn upper(s: Option<&str>) -> String {
    s.unwrap_or_default().trim().to_ascii_uppercase()
}

fn describe(r: &Record) -> String {
    let when = r.start().map_or_else(|| "no start time".to_string(), stamp);
    format!(
        "{} {} {} {}",
        or_dash(r.call()),
        when,
        or_dash(r.band()),
        or_dash(r.mode())
    )
}

fn or_dash(s: Option<&str>) -> &str {
    s.filter(|s| !s.trim().is_empty()).unwrap_or("-")
}

fn stamp(t: DateTime<Utc>) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}Z",
        t.year(),
        t.month(),
        t.day(),
        t.hour(),
        t.minute()
    )
}

/// A file-name part: letters, digits, `-`, `_` and `.` only (`/` in a call becomes `_`).
fn safe(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn compact(date: &str) -> String {
    date.chars().filter(char::is_ascii_digit).collect()
}

/// Build the preview for `req`.
pub fn build(req: &Request<'_>) -> Preview {
    let program = match req.plan.activity {
        PlanActivity::Pota => Program::Pota,
        PlanActivity::Sota => Program::Sota,
    };
    let mut preview = Preview::default();
    let refs: Vec<String> = req
        .plan
        .references
        .iter()
        .map(|r| r.trim().to_ascii_uppercase())
        .collect();
    if refs.is_empty() {
        preview
            .warnings
            .push("the plan has no reference; nothing to export".into());
        return preview;
    }
    let Ok(day) = req.plan.date.parse::<NaiveDate>() else {
        preview
            .warnings
            .push(format!("the plan date {:?} is not a date", req.plan.date));
        return preview;
    };

    // Select, newest facts first: which Contacts belong to this activation.
    let mut selected: Vec<(&Record, DateTime<Utc>, Vec<String>)> = Vec::new();
    let mut other_dates = 0;
    for record in req.records {
        let listed = my_refs(program, record);
        let start = record.start();
        if listed.is_empty() {
            if start.map(|s| s.date_naive()) == Some(day) {
                preview.no_reference.push(describe(record));
            }
            continue;
        }
        if !listed.iter().any(|l| refs.contains(l)) {
            continue;
        }
        let Some(start) = start else {
            preview
                .excluded
                .push(format!("{} (no valid start time)", describe(record)));
            continue;
        };
        if start.date_naive() != day {
            other_dates += 1;
            if !req.include_other_dates {
                preview.excluded.push(format!(
                    "{} (UTC date is not the plan's {day}; press d to include)",
                    describe(record)
                ));
                continue;
            }
        }
        selected.push((record, start, listed));
    }
    selected.sort_by_key(|(_, start, _)| *start);
    preview.selected = selected.len();

    if !preview.no_reference.is_empty() {
        preview.warnings.push(format!(
            "{} Contact(s) on {day} have no reference of their own and are not exported",
            preview.no_reference.len()
        ));
    }
    if other_dates > 0 {
        if req.include_other_dates {
            preview.warnings.push(format!(
                "{other_dates} Contact(s) are on other UTC dates than {day} (rollover?)"
            ));
        } else {
            preview.warnings.push(format!(
                "{other_dates} Contact(s) for this reference are on other UTC dates and are left out"
            ));
        }
    }
    if selected.is_empty() {
        preview
            .warnings
            .push("no Contact matches this plan; no file would be written".into());
        return preview;
    }

    match program {
        Program::Pota => build_pota(req, &refs, &selected, &mut preview),
        Program::Sota => build_sota(req, &refs[0], &selected, &mut preview),
    }
    preview
}

fn push_missing(preview: &mut Preview, r: &Record, needed: &[&str]) {
    let missing: Vec<&str> = needed
        .iter()
        .copied()
        .filter(|f| r.get(f).is_none_or(|v| v.trim().is_empty()))
        .collect();
    if !missing.is_empty() {
        preview
            .warnings
            .push(format!("{} is missing {}", describe(r), missing.join(", ")));
    }
}

type Selected<'a> = [(&'a Record, DateTime<Utc>, Vec<String>)];

// ---- POTA ADIF -----------------------------------------------------------------------------

fn field(out: &mut String, name: &str, value: &str) {
    if !value.is_empty() {
        out.push_str(&format!("<{name}:{}>{value}", value.len()));
    }
}

fn adi_header() -> String {
    let mut out = String::from("ADIF export from hamtools\n");
    field(&mut out, "ADIF_VER", "3.1.4");
    out.push('\n');
    field(&mut out, "PROGRAMID", "hamtools");
    out.push('\n');
    field(&mut out, "PROGRAMVERSION", env!("CARGO_PKG_VERSION"));
    out.push('\n');
    out.push_str("<EOH>\n");
    out
}

fn pota_record(r: &Record, operator: &str, park: &str, state: Option<&str>) -> String {
    let mut out = String::new();
    let get = |n: &str| r.get(n).map(str::trim).unwrap_or_default();
    let station = Some(get("STATION_CALLSIGN"))
        .filter(|s| !s.is_empty())
        .unwrap_or(operator);
    field(&mut out, "STATION_CALLSIGN", station);
    field(
        &mut out,
        "OPERATOR",
        Some(get("OPERATOR"))
            .filter(|s| !s.is_empty())
            .unwrap_or(operator),
    );
    for name in [
        "CALL", "QSO_DATE", "TIME_ON", "BAND", "FREQ", "MODE", "SUBMODE", "RST_SENT", "RST_RCVD",
    ] {
        field(&mut out, name, get(name));
    }
    field(&mut out, "MY_SIG", "POTA");
    field(&mut out, "MY_SIG_INFO", park);
    if let Some(state) = state {
        field(
            &mut out,
            "MY_STATE",
            state.rsplit('-').next().unwrap_or(state),
        );
    }
    if get("SIG").eq_ignore_ascii_case("POTA") && !get("SIG_INFO").is_empty() {
        field(&mut out, "SIG", "POTA");
        field(&mut out, "SIG_INFO", &get("SIG_INFO").to_ascii_uppercase());
    }
    out.push_str("<EOR>\n");
    out
}

fn build_pota(req: &Request<'_>, refs: &[String], selected: &Selected<'_>, preview: &mut Preview) {
    let day = compact(&req.plan.date);
    let call = safe(req.operator);
    let mut seen_dupes: BTreeSet<String> = BTreeSet::new();
    for r in selected {
        push_missing(
            preview,
            r.0,
            &["CALL", "QSO_DATE", "TIME_ON", "BAND", "MODE"],
        );
    }
    if refs.len() > 1 {
        preview.warnings.push(format!(
            "n-fer: {} parks, one file each; a Contact is in every file for its parks",
            refs.len()
        ));
    }
    for park in refs {
        let contacts: Vec<&(&Record, DateTime<Utc>, Vec<String>)> = selected
            .iter()
            .filter(|(_, _, listed)| listed.contains(park))
            .collect();
        if contacts.is_empty() {
            preview
                .warnings
                .push(format!("{park}: no Contact lists this park; no file"));
            continue;
        }
        // Duplicates by call, band and mode, and the count that qualifies.
        let mut keys: Vec<(String, String, String, DateTime<Utc>)> = Vec::new();
        for (r, start, _) in &contacts {
            let submode = upper(r.get("SUBMODE"));
            let mode = if submode.is_empty() {
                upper(r.mode())
            } else {
                submode
            };
            let key = (upper(r.call()), upper(r.band()), mode);
            if let Some(first) = keys
                .iter()
                .find(|k| (&k.0, &k.1, &k.2) == (&key.0, &key.1, &key.2))
            {
                let line = format!(
                    "{park}: {} repeats {} (first at {})",
                    describe(r),
                    key.0,
                    stamp(first.3)
                );
                if seen_dupes.insert(line.clone()) {
                    preview.duplicates.push(line);
                }
            } else {
                keys.push((key.0, key.1, key.2, *start));
            }
        }
        if keys.len() < POTA_TARGET {
            preview.warnings.push(format!(
                "{park}: {} of {POTA_TARGET} Contacts without duplicates; too few to count as an activation",
                keys.len()
            ));
        }
        let location = (req.location_of)(park);
        let states = location.as_deref().map(parse_locations).unwrap_or_default();
        if states.is_empty() {
            preview.warnings.push(format!(
                "{park}: no cached state, so MY_STATE is left out (cache the park with a refresh)"
            ));
        }
        // One file per park. A Contact is never repeated under another state: it would count
        // under each. For a multi-state park the operator says where they were, else the
        // uploader asks.
        let chosen: Option<String> = match (states.len(), req.my_state) {
            (1, _) => Some(states[0].clone()),
            (0, _) => None,
            (_, Some(want)) => {
                let want = want.trim().to_ascii_uppercase();
                if states.contains(&want) {
                    Some(want)
                } else {
                    preview.warnings.push(format!(
                        "{park}: {want} is not one of its locations ({}); the uploader asks which state",
                        states.join(", ")
                    ));
                    None
                }
            }
            (_, None) => {
                preview.warnings.push(format!(
                    "{park}: spans {}; the uploader asks which state (one file, no MY_STATE)",
                    states.join(", ")
                ));
                None
            }
        };
        let mut body = String::new();
        let mut lines = Vec::new();
        for (r, _, _) in &contacts {
            let line = pota_record(r, req.operator, park, chosen.as_deref());
            lines.push(line.trim_end().to_string());
            body.push_str(&line);
        }
        let suffix = match (&chosen, states.len() > 1) {
            (Some(s), true) => format!("-{s}"),
            _ => String::new(),
        };
        preview.files.push(ExportFile {
            name: format!("{call}@{}-{day}{suffix}.adi", safe(park)),
            contents: format!("{}{body}", adi_header()),
            contacts: contacts.len(),
            head: lines.into_iter().take(3).collect(),
        });
    }
}

// ---- SOTA CSV V2 ---------------------------------------------------------------------------

/// SOTA's mode names: CW, SSB, FM, AM, Data, Other.
pub fn sota_mode(mode: &str) -> &'static str {
    match mode.trim().to_ascii_uppercase().as_str() {
        "CW" => "CW",
        "SSB" | "USB" | "LSB" => "SSB",
        "FM" => "FM",
        "AM" => "AM",
        "RTTY" | "PSK" | "MFSK" | "FT8" | "FT4" | "JT65" | "JT9" | "OLIVIA" | "DIGITAL"
        | "DATA" | "HELL" | "THOR" | "CONTESTIA" | "MT63" | "DOMINO" | "FSK441" | "JT4"
        | "WSPR" => "Data",
        _ => "Other",
    }
}

/// SOTA's band names (the frequency column): its list, else the actual frequency in MHz.
fn sota_band_column(band: &str, freq: &str) -> String {
    if let Some(listed) = sota_band(band) {
        return listed.to_string();
    }
    match freq.trim().parse::<f64>() {
        Ok(mhz) if mhz > 0.0 => {
            let text = format!("{mhz:.6}");
            format!("{}MHz", text.trim_end_matches('0').trim_end_matches('.'))
        }
        _ => band.trim().to_ascii_lowercase(),
    }
}

/// The band's name in SOTA's own list, if it has one.
fn sota_band(band: &str) -> Option<&'static str> {
    Some(match band.trim().to_ascii_lowercase().as_str() {
        "160m" => "1.8MHz",
        "80m" => "3.5MHz",
        "60m" => "5MHz",
        "40m" => "7MHz",
        "30m" => "10MHz",
        "20m" => "14MHz",
        "17m" => "18MHz",
        "15m" => "21MHz",
        "12m" => "24MHz",
        "10m" => "28MHz",
        "6m" => "50MHz",
        "2m" => "144MHz",
        "70cm" => "432MHz",
        "23cm" => "1240MHz",
        _ => return None,
    })
}

fn csv_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn sota_line(r: &Record, start: DateTime<Utc>, operator: &str, summit: &str) -> String {
    let get = |n: &str| r.get(n).map(str::trim).unwrap_or_default();
    let station = Some(get("STATION_CALLSIGN"))
        .filter(|s| !s.is_empty())
        .unwrap_or(operator);
    let notes = get("COMMENT");
    format!(
        "V2,{},{},{:02}/{:02}/{:02},{:02}{:02},{},{},{},{},{}",
        station.to_ascii_uppercase(),
        summit,
        start.day(),
        start.month(),
        start.year() % 100,
        start.hour(),
        start.minute(),
        sota_band_column(get("BAND"), get("FREQ")),
        sota_mode(get("MODE")),
        get("CALL").to_ascii_uppercase(),
        get("SOTA_REF").to_ascii_uppercase(),
        csv_quote(notes),
    )
}

fn build_sota(req: &Request<'_>, summit: &str, selected: &Selected<'_>, preview: &mut Preview) {
    for r in selected {
        push_missing(preview, r.0, &["CALL", "BAND", "MODE"]);
    }
    if req.plan.references.len() > 1 {
        preview
            .warnings
            .push("a SOTA plan holds one summit; only the first is exported".to_string());
    }
    let mut calls: BTreeSet<String> = BTreeSet::new();
    let mut seen: Vec<(String, String, String)> = Vec::new();
    for (r, start, _) in selected {
        calls.insert(upper(r.call()));
        let key = (upper(r.call()), upper(r.band()), upper(r.mode()));
        if seen.contains(&key) {
            preview.duplicates.push(format!(
                "{summit}: {} repeats an earlier Contact",
                describe(r)
            ));
        } else {
            seen.push(key);
        }
        let _ = start;
    }
    if calls.len() < SOTA_TARGET {
        preview.warnings.push(format!(
            "{summit}: {} of {SOTA_TARGET} distinct stations; too few to count as an activation",
            calls.len()
        ));
    }
    // An activation ends when the summit changes, never at UTC midnight: a night activation is
    // one file. Contacts from another summit in between start a new file.
    let elsewhere: Vec<DateTime<Utc>> = req
        .records
        .iter()
        .filter(|r| {
            let listed = my_refs(Program::Sota, r);
            !listed.is_empty() && !listed.iter().any(|l| l == summit)
        })
        .filter_map(Record::start)
        .collect();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut previous: Option<DateTime<Utc>> = None;
    for (i, (_, start, _)) in selected.iter().enumerate() {
        let changed = previous.is_some_and(|p| elsewhere.iter().any(|e| *e > p && *e < *start));
        if changed || groups.is_empty() {
            groups.push(Vec::new());
        }
        groups.last_mut().expect("a group").push(i);
        previous = Some(*start);
    }
    let mut used: BTreeSet<String> = BTreeSet::new();
    for group in groups {
        let first = selected[group[0]].1;
        let lines: Vec<String> = group
            .iter()
            .map(|&i| sota_line(selected[i].0, selected[i].1, req.operator, summit))
            .collect();
        let contents = lines.iter().map(|l| format!("{l}\n")).collect::<String>();
        let base = format!(
            "{}@{}-{}",
            safe(req.operator),
            safe(summit),
            first.format("%Y%m%d")
        );
        let mut stem = base.clone();
        let mut n = 2;
        while !used.insert(stem.clone()) {
            stem = format!("{base}-{n}");
            n += 1;
        }
        preview.files.push(ExportFile {
            name: format!("{stem}.csv"),
            contents,
            contacts: lines.len(),
            head: lines.into_iter().take(3).collect(),
        });
    }
}

// ---- writing -------------------------------------------------------------------------------

#[derive(Debug)]
pub enum WriteError {
    /// These files exist and `overwrite` was not given; nothing was written.
    Exists(Vec<String>),
    /// A file name that is not a plain name (never expected from [`build`]).
    BadName(String),
    Io(String),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exists(names) => write!(f, "already exists: {}", names.join(", ")),
            Self::BadName(n) => write!(f, "refusing file name {n:?}"),
            Self::Io(e) => f.write_str(e),
        }
    }
}

fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'@'))
}

/// The files of `preview` that already exist in `dir`.
pub fn existing(dir: &Path, preview: &Preview) -> Vec<String> {
    preview
        .files
        .iter()
        .filter(|f| dir.join(&f.name).exists())
        .map(|f| f.name.clone())
        .collect()
}

/// Write every file into `dir` (created if missing), and nowhere else. Without `overwrite`, an
/// existing file stops the whole export before anything is written.
pub fn write(dir: &Path, preview: &Preview, overwrite: bool) -> Result<Vec<PathBuf>, WriteError> {
    for f in &preview.files {
        if !plain_name(&f.name) {
            return Err(WriteError::BadName(f.name.clone()));
        }
    }
    if !overwrite {
        let clash = existing(dir, preview);
        if !clash.is_empty() {
            return Err(WriteError::Exists(clash));
        }
    }
    fs::create_dir_all(dir).map_err(|e| WriteError::Io(format!("{}: {e}", dir.display())))?;
    let mut done = Vec::new();
    for f in &preview.files {
        let path = dir.join(&f.name);
        let mut options = OpenOptions::new();
        options.write(true);
        if overwrite {
            options.create(true).truncate(true);
        } else {
            options.create_new(true);
        }
        options
            .open(&path)
            .and_then(|mut file| file.write_all(f.contents.as_bytes()))
            .map_err(|e| WriteError::Io(format!("{}: {e}", path.display())))?;
        done.push(path);
    }
    Ok(done)
}
