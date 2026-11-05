//! Export writers and selection, against golden files. Pure: records in, files out; the only I/O is
//! the temp directory the write tests use.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use hamtools_config::{Plan, PlanActivity};
use logbook::{Callsign, Log, Record};
use rig_tui::export::{self, Request, WriteError};

fn temp_dir(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rig-tui-export-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Records as the Log would hand them back: written as ADI, read with `Log::read`.
fn records(rows: &[&[(&str, &str)]]) -> Vec<Record> {
    let dir = temp_dir("log");
    let mut adi = String::from("test\n<EOH>\n");
    for row in rows {
        for (n, v) in *row {
            adi.push_str(&format!("<{n}:{}>{v}", v.len()));
        }
        adi.push_str("<EOR>\n");
    }
    let path = dir.join("log.adi");
    std::fs::write(&path, adi).unwrap();
    let out = Log::new(&path, Callsign::new("K9TST").unwrap())
        .read()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    out
}

fn plan(activity: PlanActivity, refs: &[&str], date: &str) -> Plan {
    Plan {
        id: "p".into(),
        activity,
        references: refs.iter().map(|r| r.to_string()).collect(),
        date: date.into(),
        bands: vec![],
        modes: vec![],
        softkey_set: None,
        softkeys: BTreeMap::new(),
        notes: None,
        done: false,
        snapshot: None,
        other_snapshots: vec![],
    }
}

fn locations(park: &str) -> Option<String> {
    match park {
        "US-1234" | "US-5678" => Some("US-IN".into()),
        "US-9999" => Some("US-IN,US-MI".into()),
        _ => None,
    }
}

fn request<'a>(plan: &'a Plan, records: &'a [Record], other_dates: bool) -> Request<'a> {
    Request {
        plan,
        operator: "K9TST/P",
        records,
        include_other_dates: other_dates,
        location_of: &locations,
        my_state: None,
    }
}

const BASE: [(&str, &str); 7] = [
    ("STATION_CALLSIGN", "K9TST/P"),
    ("OPERATOR", "K9TST"),
    ("QSO_DATE", "20261105"),
    ("TIME_ON", "150000"),
    ("BAND", "40m"),
    ("FREQ", "7.030000"),
    ("MODE", "CW"),
];

fn row<'a>(extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut v: Vec<(&str, &str)> = BASE.to_vec();
    for (n, val) in extra {
        match v.iter_mut().find(|(k, _)| k == n) {
            Some(slot) => slot.1 = val,
            None => v.push((n, val)),
        }
    }
    v
}

fn pota_log() -> Vec<Record> {
    let a = row(&[
        ("CALL", "W1ABC"),
        ("RST_SENT", "599"),
        ("RST_RCVD", "599"),
        ("MY_SIG", "POTA"),
        ("MY_SIG_INFO", "US-1234"),
        ("COMMENT", "not exported"),
    ]);
    let b = row(&[
        ("CALL", "W2DEF"),
        ("TIME_ON", "150500"),
        ("BAND", "20m"),
        ("FREQ", "14.060000"),
        ("MY_SIG", "POTA"),
        ("MY_SIG_INFO", "US-1234"),
        ("SIG", "POTA"),
        ("SIG_INFO", "US-0001"),
    ]);
    let dup = row(&[
        ("CALL", "W1ABC"),
        ("TIME_ON", "151000"),
        ("MY_SIG", "POTA"),
        ("MY_SIG_INFO", "US-1234"),
    ]);
    let no_ref = row(&[("CALL", "K1XYZ"), ("TIME_ON", "152000")]);
    let next_day = row(&[
        ("CALL", "N3LATE"),
        ("QSO_DATE", "20261106"),
        ("TIME_ON", "003000"),
        ("MY_SIG", "POTA"),
        ("MY_SIG_INFO", "US-1234"),
    ]);
    let elsewhere = row(&[
        ("CALL", "W9ELSE"),
        ("MY_SIG", "POTA"),
        ("MY_SIG_INFO", "US-7777"),
    ]);
    // Out of order on purpose: the file is chronological.
    records(&[&b, &a, &dup, &no_ref, &next_day, &elsewhere])
}

#[test]
fn pota_adif_matches_golden_with_park_to_park_and_state() {
    let log = pota_log();
    let plan = plan(PlanActivity::Pota, &["US-1234"], "2026-11-05");
    let preview = export::build(&request(&plan, &log, false));
    assert_eq!(preview.files.len(), 1);
    let file = &preview.files[0];
    assert_eq!(file.name, "K9TST_P@US-1234-20261105.adi");
    assert_eq!(file.contacts, 3);
    assert_eq!(file.contents, include_str!("fixtures/export/pota.adi"));
}

#[test]
fn pota_preview_names_every_exclusion_duplicate_and_warning() {
    let log = pota_log();
    let plan = plan(PlanActivity::Pota, &["US-1234"], "2026-11-05");
    let p = export::build(&request(&plan, &log, false));
    assert_eq!(p.selected, 3);
    assert_eq!(p.no_reference.len(), 1);
    assert!(p.no_reference[0].starts_with("K1XYZ"));
    assert_eq!(p.excluded.len(), 1);
    assert!(p.excluded[0].starts_with("N3LATE 2026-11-06 00:30Z"));
    assert_eq!(p.duplicates.len(), 1);
    assert!(p.duplicates[0].contains("W1ABC"));
    let w = p.warnings.join("\n");
    assert!(w.contains("2 of 10"), "{w}");
    assert!(w.contains("have no reference"), "{w}");
    assert!(w.contains("other UTC dates"), "{w}");
    assert_eq!(p.files[0].head.len(), 3);
}

#[test]
fn the_toggle_includes_other_dates_and_warns_of_the_rollover() {
    let log = pota_log();
    let plan = plan(PlanActivity::Pota, &["US-1234"], "2026-11-05");
    let p = export::build(&request(&plan, &log, true));
    assert_eq!(p.selected, 4);
    assert!(p.excluded.is_empty());
    assert!(p.warnings.join("\n").contains("rollover"));
    assert!(p.files[0].contents.contains("N3LATE"));
}

#[test]
fn an_nfer_writes_one_file_per_park_and_a_contact_in_each() {
    let both = row(&[
        ("CALL", "W1ABC"),
        ("MY_SIG", "POTA"),
        ("MY_SIG_INFO", "US-1234,US-5678"),
    ]);
    let only_second = row(&[
        ("CALL", "W2DEF"),
        ("TIME_ON", "150500"),
        ("MY_SIG", "POTA"),
        ("MY_SIG_INFO", "US-5678"),
    ]);
    let log = records(&[&both, &only_second]);
    let plan = plan(PlanActivity::Pota, &["US-1234", "US-5678"], "2026-11-05");
    let p = export::build(&request(&plan, &log, false));
    assert_eq!(p.selected, 2);
    let names: Vec<&str> = p.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "K9TST_P@US-1234-20261105.adi",
            "K9TST_P@US-5678-20261105.adi"
        ]
    );
    assert_eq!(
        p.files[0].contents,
        include_str!("fixtures/export/nfer_1234.adi")
    );
    assert_eq!(
        p.files[1].contents,
        include_str!("fixtures/export/nfer_5678.adi")
    );
    assert!(p.warnings.join("\n").contains("n-fer"));
}

fn multi_state_log() -> Vec<Record> {
    let a = row(&[
        ("CALL", "W1ABC"),
        ("MY_SIG", "POTA"),
        ("MY_SIG_INFO", "US-9999"),
    ]);
    records(&[&a])
}

#[test]
fn a_multi_state_park_writes_one_file_not_one_per_state() {
    let log = multi_state_log();
    let plan = plan(PlanActivity::Pota, &["US-9999"], "2026-11-05");
    let p = export::build(&request(&plan, &log, false));
    assert_eq!(p.files.len(), 1);
    assert_eq!(p.files[0].name, "K9TST_P@US-9999-20261105.adi");
    assert!(!p.files[0].contents.contains("MY_STATE"));
    assert!(p.warnings.join("\n").contains("asks which state"));
}

#[test]
fn a_chosen_state_is_the_only_one_written_with_the_full_location_suffix() {
    let log = multi_state_log();
    let plan = plan(PlanActivity::Pota, &["US-9999"], "2026-11-05");
    let mut req = request(&plan, &log, false);
    req.my_state = Some("US-MI");
    let p = export::build(&req);
    assert_eq!(p.files.len(), 1);
    assert_eq!(p.files[0].name, "K9TST_P@US-9999-20261105-US-MI.adi");
    assert!(p.files[0].contents.contains("<MY_STATE:2>MI"));
    assert!(!p.files[0].contents.contains(">IN"));
}

#[test]
fn a_chosen_state_the_park_is_not_in_is_ignored_with_a_warning() {
    let log = multi_state_log();
    let plan = plan(PlanActivity::Pota, &["US-9999"], "2026-11-05");
    let mut req = request(&plan, &log, false);
    req.my_state = Some("US-OH");
    let p = export::build(&req);
    assert!(!p.files[0].contents.contains("MY_STATE"));
    assert!(p.warnings.join("\n").contains("US-OH"));
}

#[test]
fn the_pota_duplicate_key_uses_the_submode_when_present() {
    let row_for = |time: &'static str, sub: &'static str| {
        row(&[
            ("CALL", "W1ABC"),
            ("TIME_ON", time),
            ("MODE", "MFSK"),
            ("SUBMODE", sub),
            ("MY_SIG", "POTA"),
            ("MY_SIG_INFO", "US-1234"),
        ])
    };
    let ft4 = row_for("150000", "FT4");
    let ft8 = row_for("150500", "FT8");
    let ft8_again = row_for("151000", "FT8");
    let log = records(&[&ft4, &ft8, &ft8_again]);
    let plan = plan(PlanActivity::Pota, &["US-1234"], "2026-11-05");
    let p = export::build(&request(&plan, &log, false));
    assert_eq!(p.duplicates.len(), 1, "{:?}", p.duplicates);
    assert!(p.warnings.join("\n").contains("2 of 10"));
}

#[test]
fn an_unknown_state_warns_and_leaves_my_state_out() {
    let a = row(&[
        ("CALL", "W1ABC"),
        ("MY_SIG", "POTA"),
        ("MY_SIG_INFO", "US-4242"),
    ]);
    let log = records(&[&a]);
    let plan = plan(PlanActivity::Pota, &["US-4242"], "2026-11-05");
    let p = export::build(&request(&plan, &log, false));
    assert!(!p.files[0].contents.contains("MY_STATE"));
    assert!(p.warnings.join("\n").contains("no cached state"));
}

fn sota_log() -> Vec<Record> {
    let a = row(&[
        ("CALL", "W7XYZ"),
        ("MY_SOTA_REF", "W7W/KG-001"),
        ("COMMENT", "nice \"sig\", thanks"),
    ]);
    let s2s = row(&[
        ("CALL", "K7SUM"),
        ("TIME_ON", "150230"),
        ("BAND", "20m"),
        ("MODE", "SSB"),
        ("SUBMODE", "USB"),
        ("MY_SOTA_REF", "W7W/KG-001"),
        ("SOTA_REF", "W7W/SN-002"),
    ]);
    let ft8 = row(&[
        ("CALL", "N0DIG"),
        ("TIME_ON", "150900"),
        ("MODE", "FT8"),
        ("MY_SOTA_REF", "W7W/KG-001"),
    ]);
    let other_summit = row(&[("CALL", "W0OTH"), ("MY_SOTA_REF", "W7W/KG-002")]);
    records(&[&s2s, &a, &ft8, &other_summit])
}

#[test]
fn sota_csv_v2_matches_golden_with_summit_to_summit_and_quoted_notes() {
    let log = sota_log();
    let plan = plan(PlanActivity::Sota, &["W7W/KG-001"], "2026-11-05");
    let p = export::build(&request(&plan, &log, false));
    assert_eq!(p.files.len(), 1);
    assert_eq!(p.files[0].name, "K9TST_P@W7W_KG-001-20261105.csv");
    assert_eq!(
        p.files[0].contents,
        include_str!("fixtures/export/sota.csv")
    );
    assert!(p.warnings.join("\n").contains("3 of 4 distinct"));
}

#[test]
fn sota_band_names_are_mhz_style_or_the_frequency() {
    let at = |band: &str, freq: &str| {
        let r = row(&[
            ("CALL", "W7XYZ"),
            ("MY_SOTA_REF", "W7W/KG-001"),
            ("BAND", band),
            ("FREQ", freq),
        ]);
        let log = records(&[&r]);
        let plan = plan(PlanActivity::Sota, &["W7W/KG-001"], "2026-11-05");
        let p = export::build(&request(&plan, &log, false));
        p.files[0].contents.split(',').nth(5).unwrap().to_string()
    };
    for (band, want) in [
        ("160m", "1.8MHz"),
        ("80m", "3.5MHz"),
        ("60m", "5MHz"),
        ("40m", "7MHz"),
        ("30M", "10MHz"),
        ("20m", "14MHz"),
        ("17m", "18MHz"),
        ("15m", "21MHz"),
        ("12m", "24MHz"),
        ("10m", "28MHz"),
        ("6m", "50MHz"),
        ("2m", "144MHz"),
        ("70cm", "432MHz"),
        ("23cm", "1240MHz"),
    ] {
        assert_eq!(at(band, "7.030000"), want, "{band}");
    }
    // Not in SOTA's list: the actual frequency, trimmed.
    assert_eq!(at("33cm", "902.100000"), "902.1MHz");
    assert_eq!(at("", "7.032000"), "7.032MHz");
}

#[test]
fn a_night_sota_activation_across_utc_midnight_stays_in_one_file() {
    let late = row(&[
        ("CALL", "W7XYZ"),
        ("TIME_ON", "233000"),
        ("MY_SOTA_REF", "W7W/KG-001"),
    ]);
    let after = row(&[
        ("CALL", "W7ABC"),
        ("QSO_DATE", "20261106"),
        ("TIME_ON", "003000"),
        ("MY_SOTA_REF", "W7W/KG-001"),
    ]);
    let log = records(&[&after, &late]);
    let plan = plan(PlanActivity::Sota, &["W7W/KG-001"], "2026-11-05");
    let p = export::build(&request(&plan, &log, true));
    assert_eq!(p.files.len(), 1);
    assert_eq!(p.files[0].name, "K9TST_P@W7W_KG-001-20261105.csv");
    assert_eq!(p.files[0].contacts, 2);
    assert!(p.files[0].contents.contains("W7XYZ") && p.files[0].contents.contains("W7ABC"));
}

#[test]
fn sota_splits_the_file_only_when_the_summit_changes() {
    let day1 = row(&[("CALL", "W7XYZ"), ("MY_SOTA_REF", "W7W/KG-001")]);
    let elsewhere = row(&[
        ("CALL", "W7OTH"),
        ("TIME_ON", "160000"),
        ("MY_SOTA_REF", "W7W/KG-002"),
    ]);
    let day2 = row(&[
        ("CALL", "W7ABC"),
        ("QSO_DATE", "20261106"),
        ("TIME_ON", "160000"),
        ("MY_SOTA_REF", "W7W/KG-001"),
    ]);
    let log = records(&[&day2, &elsewhere, &day1]);
    let plan = plan(PlanActivity::Sota, &["W7W/KG-001"], "2026-11-05");
    let p = export::build(&request(&plan, &log, true));
    let names: Vec<&str> = p.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "K9TST_P@W7W_KG-001-20261105.csv",
            "K9TST_P@W7W_KG-001-20261106.csv"
        ]
    );
    assert_eq!(p.files[0].contacts, 1);
    assert!(p.files[1].contents.contains("W7ABC"));
    assert!(!p.files[0].contents.contains("W7ABC"));
}

#[test]
fn sota_modes_collapse_to_the_six_names() {
    for (mode, want) in [
        ("CW", "CW"),
        ("ssb", "SSB"),
        ("USB", "SSB"),
        ("FM", "FM"),
        ("AM", "AM"),
        ("FT8", "Data"),
        ("RTTY", "Data"),
        ("C4FM", "Other"),
    ] {
        assert_eq!(export::sota_mode(mode), want, "{mode}");
    }
}

#[test]
fn a_contact_missing_a_required_field_warns_and_is_still_written() {
    let a = row(&[
        ("CALL", "W1ABC"),
        ("MY_SIG", "POTA"),
        ("MY_SIG_INFO", "US-1234"),
    ]);
    let mut b = row(&[
        ("CALL", "W2DEF"),
        ("MY_SIG", "POTA"),
        ("MY_SIG_INFO", "US-1234"),
    ]);
    b.retain(|(n, _)| *n != "BAND");
    let log = records(&[&a, &b]);
    let plan = plan(PlanActivity::Pota, &["US-1234"], "2026-11-05");
    let p = export::build(&request(&plan, &log, false));
    assert_eq!(p.files[0].contacts, 2);
    assert!(p.warnings.join("\n").contains("W2DEF"), "{:?}", p.warnings);
}

#[test]
fn a_contact_with_no_start_time_is_named_not_dropped() {
    let mut a = row(&[
        ("CALL", "W1ABC"),
        ("MY_SIG", "POTA"),
        ("MY_SIG_INFO", "US-1234"),
    ]);
    a.retain(|(n, _)| *n != "TIME_ON");
    let log = records(&[&a]);
    let plan = plan(PlanActivity::Pota, &["US-1234"], "2026-11-05");
    let p = export::build(&request(&plan, &log, false));
    assert!(p.files.is_empty());
    assert_eq!(p.excluded.len(), 1);
    assert!(p.excluded[0].contains("no valid start time"));
}

#[test]
fn nothing_is_written_until_write_and_only_into_the_directory() {
    let log = pota_log();
    let plan = plan(PlanActivity::Pota, &["US-1234"], "2026-11-05");
    let p = export::build(&request(&plan, &log, false));
    let dir = temp_dir("out").join("nested").join("exports");
    assert!(!dir.exists());
    let written = export::write(&dir, &p, false).unwrap();
    assert_eq!(written, vec![dir.join("K9TST_P@US-1234-20261105.adi")]);
    assert_eq!(
        std::fs::read_to_string(&written[0]).unwrap(),
        p.files[0].contents
    );
    let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
    assert_eq!(entries.len(), 1);
}

#[test]
fn an_existing_file_stops_the_write_unless_overwrite_is_given() {
    let log = pota_log();
    let plan = plan(PlanActivity::Pota, &["US-1234"], "2026-11-05");
    let p = export::build(&request(&plan, &log, false));
    let dir = temp_dir("exists");
    let path = dir.join(&p.files[0].name);
    std::fs::write(&path, "mine").unwrap();
    assert_eq!(export::existing(&dir, &p), vec![p.files[0].name.clone()]);
    match export::write(&dir, &p, false) {
        Err(WriteError::Exists(names)) => assert_eq!(names, vec![p.files[0].name.clone()]),
        other => panic!("{other:?}"),
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "mine");
    export::write(&dir, &p, true).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), p.files[0].contents);
}

#[test]
fn a_file_name_that_could_leave_the_directory_is_refused() {
    let mut p = export::Preview::default();
    for name in ["../evil.adi", "a/b.adi", ".hidden", ""] {
        p.files = vec![export::ExportFile {
            name: name.into(),
            contents: "x".into(),
            contacts: 1,
            head: vec![],
        }];
        let dir = temp_dir("evil");
        assert!(matches!(
            export::write(&dir, &p, true),
            Err(WriteError::BadName(_))
        ));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }
}

#[test]
fn building_never_changes_the_plan() {
    let log = pota_log();
    let plan = plan(PlanActivity::Pota, &["US-1234"], "2026-11-05");
    let before = plan.clone();
    let _ = export::build(&request(&plan, &log, true));
    assert_eq!(plan, before);
}

#[test]
fn the_unverified_uploader_behaviours_are_listed() {
    let text = export::UNVERIFIED.join("\n");
    assert!(text.contains("multi-state"));
    assert!(text.contains("7.032MHz"));
}
