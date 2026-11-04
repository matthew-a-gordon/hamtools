mod common;

use common::*;
use logbook::{Callsign, Contact, ContactInput, Log, Mode};

fn written(log_name: &str, contact: &Contact) -> String {
    let dir = temp_dir(log_name);
    let log = Log::new(dir.join("log.adi"), operator());
    log.append(contact).unwrap();
    std::fs::read_to_string(dir.join("log.adi")).unwrap()
}

#[test]
fn full_satellite_contact_matches_golden() {
    let text = written("sat", &satellite_contact());
    assert_eq!(text, include_str!("fixtures/satellite.adi"));
    let records = read_adi(&text);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].len(), 20);
}

#[test]
fn hf_cw_contact_with_sota_extras_and_station_override_matches_golden() {
    let contact = Contact::new(ContactInput {
        call: "W7XYZ".into(),
        grid: None,
        name: None,
        comment: None,
        rst_sent: Some("599".into()),
        rst_rcvd: Some("599".into()),
        start: at(4, 17, 30, 0),
        end: at(4, 17, 35, 10),
        freq_hz: 14_060_000,
        mode: Mode::Cw,
        own_grid: None,
        station_callsign: Some(Callsign::new("k9tst/p").unwrap()),
        extras: vec![
            ("SOTA_REF".into(), "W7W/SN-001".into()),
            ("MY_SOTA_REF".into(), "W9/SE-001".into()),
        ],
    })
    .unwrap();
    let text = written("cw", &contact);
    assert_eq!(text, include_str!("fixtures/hf_cw_sota.adi"));
    read_adi(&text);
}

#[test]
fn midnight_rollover_writes_the_end_date() {
    let c = simple_contact("K1ABC", at(4, 23, 59, 0), at(5, 0, 1, 0));
    let records = read_adi(&written("midnight", &c));
    let get = |k: &str| {
        records[0]
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(get("QSO_DATE"), Some("20261104"));
    assert_eq!(get("QSO_DATE_OFF"), Some("20261105"));
    assert_eq!(get("TIME_OFF"), Some("000100"));
}

#[test]
fn same_day_contact_has_no_end_date() {
    let c = simple_contact("K1ABC", at(4, 12, 0, 0), at(4, 12, 1, 0));
    let records = read_adi(&written("sameday", &c));
    assert!(!records[0].iter().any(|(n, _)| n == "QSO_DATE_OFF"));
}

fn pota_p2p_nfer() -> Contact {
    Contact::new(ContactInput {
        call: "W1ABC".into(),
        grid: None,
        name: None,
        comment: None,
        rst_sent: Some("59".into()),
        rst_rcvd: Some("59".into()),
        start: at(4, 15, 0, 0),
        end: at(4, 15, 1, 30),
        freq_hz: 7_200_000,
        mode: Mode::Ssb(logbook::Sideband::Lower),
        own_grid: None,
        station_callsign: None,
        extras: vec![
            ("MY_SIG".into(), "POTA".into()),
            ("MY_SIG_INFO".into(), "US-1234,US-5678".into()),
            ("SIG".into(), "POTA".into()),
            ("SIG_INFO".into(), "US-0001".into()),
        ],
    })
    .unwrap()
}

#[test]
fn pota_park_to_park_nfer_contact_matches_golden() {
    let text = written("p2p", &pota_p2p_nfer());
    assert_eq!(text, include_str!("fixtures/pota_p2p_nfer.adi"));
}

#[test]
fn reader_returns_the_extras_and_core_fields_of_what_was_written() {
    let dir = temp_dir("read");
    let log = Log::new(dir.join("log.adi"), operator());
    log.append(&pota_p2p_nfer()).unwrap();
    log.append(&simple_contact("K1ABC", at(4, 16, 0, 0), at(4, 16, 1, 0)))
        .unwrap();

    let records = log.read().unwrap();
    assert_eq!(records.len(), 2);
    let first = &records[0];
    assert_eq!(first.call(), Some("W1ABC"));
    assert_eq!(first.band(), Some("40m"));
    assert_eq!(first.mode(), Some("SSB"));
    assert_eq!(first.start(), Some(at(4, 15, 0, 0)));
    assert_eq!(first.get("my_sig_info"), Some("US-1234,US-5678"));
    assert_eq!(
        first.extras(),
        vec![
            ("MY_SIG", "POTA"),
            ("MY_SIG_INFO", "US-1234,US-5678"),
            ("SIG", "POTA"),
            ("SIG_INFO", "US-0001"),
        ]
    );
    assert!(records[1].extras().is_empty());
}

#[test]
fn reader_returns_nothing_for_a_missing_log_and_refuses_a_torn_one() {
    let dir = temp_dir("read-missing");
    let log = Log::new(dir.join("nope.adi"), operator());
    assert!(log.read().unwrap().is_empty());

    log.append(&simple_contact("K1ABC", at(4, 12, 0, 0), at(4, 12, 1, 0)))
        .unwrap();
    let mut bytes = std::fs::read(log.path()).unwrap();
    bytes.extend_from_slice(b"<CALL:5>K2D");
    std::fs::write(log.path(), bytes).unwrap();
    assert!(log.read().is_err());
}

#[test]
fn reader_reads_a_file_with_hand_edits_in_the_header_and_mixed_case() {
    let dir = temp_dir("read-hand");
    let path = dir.join("log.adi");
    std::fs::write(
        &path,
        "my notes\n<eoh>\n<call:5>k1abc<Sota_Ref:10>W7W/SN-001<qso_date:8>20261104<time_on:4>1730<eor>\n",
    )
    .unwrap();
    let records = Log::new(&path, operator()).read().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].call(), Some("k1abc"));
    assert_eq!(records[0].get("SOTA_REF"), Some("W7W/SN-001"));
    assert_eq!(records[0].start(), Some(at(4, 17, 30, 0)));
}
