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
