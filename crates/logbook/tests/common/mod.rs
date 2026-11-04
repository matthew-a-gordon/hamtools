//! Test helpers: unique temp directories, Contact builders and an independent ADI reader.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use chrono::{DateTime, TimeZone, Utc};
use logbook::{Callsign, Contact, ContactInput, Mode, satellite};

pub fn temp_dir(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "logbook-test-{}-{}-{tag}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn operator() -> Callsign {
    Callsign::new("K9TST").unwrap()
}

pub fn at(d: u32, h: u32, m: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, d, h, m, s).unwrap()
}

pub fn satellite_contact() -> Contact {
    Contact::new(ContactInput {
        call: "k1abc".into(),
        grid: Some("fn42".into()),
        name: Some("Ann".into()),
        comment: Some("nice pass".into()),
        rst_sent: Some("59".into()),
        rst_rcvd: Some("59".into()),
        start: at(4, 12, 0, 0),
        end: at(4, 12, 3, 30),
        freq_hz: 435_250_000,
        mode: Mode::Fm,
        own_grid: Some("EN71DQ".into()),
        station_callsign: None,
        extras: satellite::extras("AO-91", 435_250_000, 145_960_000).unwrap(),
    })
    .unwrap()
}

/// A minimal Contact between the given times.
pub fn simple_contact(call: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> Contact {
    Contact::new(ContactInput {
        call: call.into(),
        grid: None,
        name: None,
        comment: None,
        rst_sent: None,
        rst_rcvd: None,
        start,
        end,
        freq_hz: 145_900_000,
        mode: Mode::Fm,
        own_grid: None,
        station_callsign: None,
        extras: vec![],
    })
    .unwrap()
}

/// Parse ADI text, checking that every length prefix matches its value exactly.
/// Returns the records as ordered (name, value) lists; the header is skipped.
pub fn read_adi(text: &str) -> Vec<Vec<(String, String)>> {
    let mut rest = text;
    if !rest.starts_with('<') {
        let eoh = rest.find("<EOH>").expect("header without <EOH>");
        rest = &rest[eoh + 5..];
    }
    let mut records = Vec::new();
    let mut current = Vec::new();
    loop {
        rest = rest.trim_start_matches(['\n', '\r', ' ']);
        if rest.is_empty() {
            assert!(current.is_empty(), "record without <EOR>");
            return records;
        }
        assert!(rest.starts_with('<'), "unexpected text: {rest:.40}");
        let close = rest.find('>').expect("unterminated tag");
        let tag = &rest[1..close];
        rest = &rest[close + 1..];
        if tag.eq_ignore_ascii_case("EOR") {
            records.push(std::mem::take(&mut current));
            continue;
        }
        let (name, len) = tag.split_once(':').expect("field without length");
        let len: usize = len.parse().expect("length is plain digits");
        assert!(rest.len() >= len, "value of {name} shorter than its length");
        let (value, after) = rest.split_at(len);
        assert!(
            after.is_empty() || after.starts_with('<'),
            "length of {name} too short: next is {after:.20}"
        );
        current.push((name.to_string(), value.to_string()));
        rest = after;
    }
}
