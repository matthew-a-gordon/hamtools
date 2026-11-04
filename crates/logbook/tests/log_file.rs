mod common;

use std::fs;
use std::path::PathBuf;

use common::*;
use logbook::{AppendError, Log};

fn setup(tag: &str) -> (Log, PathBuf) {
    let path = temp_dir(tag).join("log.adi");
    (Log::new(&path, operator()), path)
}

fn contact(call: &str) -> logbook::Contact {
    simple_contact(call, at(4, 12, 0, 0), at(4, 12, 1, 0))
}

#[test]
fn new_file_gets_one_header_and_later_appends_none() {
    let (log, path) = setup("header");
    log.append(&contact("K1ABC")).unwrap();
    log.append(&contact("K2DEF")).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(text.matches("<EOH>").count(), 1);
    assert!(text.starts_with("ADIF export"));
    assert_eq!(read_adi(&text).len(), 2);
}

#[test]
fn creates_missing_directories() {
    let dir = temp_dir("mkdir");
    let path = dir.join("a/b/log.adi");
    Log::new(&path, operator())
        .append(&contact("K1ABC"))
        .unwrap();
    assert!(path.exists());
}

#[test]
fn empty_existing_file_gets_a_header() {
    let (log, path) = setup("empty");
    fs::write(&path, "").unwrap();
    log.append(&contact("K1ABC")).unwrap();
    assert!(fs::read_to_string(&path).unwrap().contains("<EOH>"));
}

#[test]
fn torn_tail_is_refused_with_offset_and_file_untouched() {
    let (log, path) = setup("torn");
    log.append(&contact("K1ABC")).unwrap();
    let good_len = fs::metadata(&path).unwrap().len();
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(b"<STATION_CALLSIGN:5>K9TST<CALL:5>K2D");
    fs::write(&path, &bytes).unwrap();

    let err = log.append(&contact("K3GHI")).unwrap_err();
    match &err {
        AppendError::PartialRecord { offset, .. } => assert_eq!(*offset, good_len),
        other => panic!("unexpected {other:?}"),
    }
    assert!(err.to_string().contains(&good_len.to_string()));
    assert!(err.to_string().contains("delete everything after"));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(matches!(
        log.check(),
        Err(AppendError::PartialRecord { .. })
    ));
}

#[test]
fn torn_header_only_file_is_refused() {
    let (log, path) = setup("tornhdr");
    fs::write(&path, "ADIF export\n<ADIF_VER:5>3.1.4\n<EOH>\n<CALL:5>K1").unwrap();
    assert!(matches!(
        log.append(&contact("K1ABC")),
        Err(AppendError::PartialRecord { offset: 36, .. })
    ));
}

#[test]
fn non_adif_file_is_refused_and_untouched() {
    let (log, path) = setup("notadif");
    fs::write(&path, "just some notes\nnot a log\n").unwrap();
    assert!(matches!(
        log.append(&contact("K1ABC")),
        Err(AppendError::NotAdif { .. })
    ));
    assert!(matches!(log.check(), Err(AppendError::NotAdif { .. })));
    assert_eq!(fs::read(&path).unwrap(), b"just some notes\nnot a log\n");
}

#[test]
fn large_file_without_a_marker_in_the_tail_is_refused() {
    let (log, path) = setup("nomarker");
    let mut bytes = b"<CALL:5>".to_vec();
    bytes.extend(vec![b'x'; 70 * 1024]);
    fs::write(&path, bytes).unwrap();
    assert!(matches!(
        log.check(),
        Err(AppendError::NoMarkerInTail { .. })
    ));
}

#[test]
fn marker_just_inside_the_window_of_a_large_file_is_found() {
    let (log, path) = setup("bigok");
    log.append(&contact("K1ABC")).unwrap();
    let mut text = fs::read_to_string(&path).unwrap();
    text.insert_str(0, &"; padding\n".repeat(8000)); // text before the header, > 64 KiB
    fs::write(&path, &text).unwrap();
    log.append(&contact("K2DEF")).unwrap();
    assert_eq!(read_adi(&fs::read_to_string(&path).unwrap()).len(), 2);
}

#[test]
fn missing_final_newline_gets_a_leading_newline() {
    let (log, path) = setup("nonewline");
    log.append(&contact("K1ABC")).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.trim_end()).unwrap();
    log.check().unwrap();
    log.append(&contact("K2DEF")).unwrap();
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("<EOR>\n<STATION_CALLSIGN"));
    assert_eq!(read_adi(&after).len(), 2);
}

#[test]
fn existing_bytes_are_never_modified() {
    let (log, path) = setup("prefix");
    log.append(&contact("K1ABC")).unwrap();
    let before = fs::read(&path).unwrap();
    log.append(&contact("K2DEF")).unwrap();
    assert!(fs::read(&path).unwrap().starts_with(&before));
}

#[test]
fn check_on_a_missing_or_good_file_passes_and_writes_nothing() {
    let (log, path) = setup("check");
    log.check().unwrap();
    assert!(!path.exists());
    log.append(&contact("K1ABC")).unwrap();
    let before = fs::read(&path).unwrap();
    log.check().unwrap();
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn concurrent_appends_stay_intact() {
    let (_, path) = setup("threads");
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let path = path.clone();
            std::thread::spawn(move || {
                let log = Log::new(&path, operator());
                for i in 0..25 {
                    log.append(&contact(&format!("K{t}T{i}"))).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(text.matches("<EOH>").count(), 1);
    let records = read_adi(&text);
    assert_eq!(records.len(), 200);
    let mut calls: Vec<_> = records
        .iter()
        .map(|r| r.iter().find(|(n, _)| n == "CALL").unwrap().1.clone())
        .collect();
    calls.sort();
    calls.dedup();
    assert_eq!(calls.len(), 200);
}

#[test]
fn non_adif_file_with_a_stray_marker_in_its_tail_is_refused() {
    for stray in ["<EOR>", "<eor>\n", "<EOR>\n<EOR>"] {
        let (log, path) = setup("stray");
        let bytes = format!("just some notes\nnot a log {stray}");
        fs::write(&path, &bytes).unwrap();
        assert!(
            matches!(
                log.append(&contact("K1ABC")),
                Err(AppendError::NotAdif { .. })
            ),
            "append accepted {stray:?}"
        );
        assert!(matches!(log.check(), Err(AppendError::NotAdif { .. })));
        assert_eq!(fs::read(&path).unwrap(), bytes.as_bytes());
    }
}

#[test]
fn a_headerless_file_starting_with_a_field_is_still_accepted() {
    let (log, path) = setup("headerless");
    fs::write(&path, "<CALL:5>K1ABC<EOR>\n").unwrap();
    log.append(&contact("K2DEF")).unwrap();
    assert_eq!(read_adi(&fs::read_to_string(&path).unwrap()).len(), 2);
}

#[test]
fn check_and_append_refuse_the_same_bad_files_identically() {
    let good = {
        let (log, path) = setup("same-good");
        log.append(&contact("K1ABC")).unwrap();
        fs::read(&path).unwrap()
    };
    let mut torn = good.clone();
    torn.extend_from_slice(b"<CALL:5>K2D");
    let mut big_lt = vec![b'<'];
    big_lt.extend(vec![b'x'; 70 * 1024]);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("torn", torn),
        ("non-adi", b"just some notes\n".to_vec()),
        ("non-adi-stray", b"notes not adif <EOR>".to_vec()),
        ("big-no-marker", vec![b'x'; 70 * 1024]),
        ("big-no-marker-lt", big_lt),
        ("good", good),
        ("empty", Vec::new()),
    ];
    for (name, bytes) in cases {
        let (log, path) = setup(name);
        fs::write(&path, &bytes).unwrap();
        let checked = log.check().map_err(|e| e.to_string());
        assert_eq!(fs::read(&path).unwrap(), bytes, "check wrote to {name}");
        let appended = log.append(&contact("K3GHI")).map_err(|e| e.to_string());
        assert_eq!(
            checked.is_err(),
            appended.is_err(),
            "{name}: check {checked:?}, append {appended:?}"
        );
        if let (Err(c), Err(a)) = (checked, appended) {
            assert_eq!(c, a, "{name}");
            assert_eq!(fs::read(&path).unwrap(), bytes, "append wrote to {name}");
        }
    }
}
