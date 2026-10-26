//! Expected values come from Skyfield 1.54 (wgs84 topocentric, same TLEs), not from this crate.
//! Observer is town-level Middlebury, IN. Range rate is a 1 s central difference of range.

use chrono::{DateTime, TimeZone, Utc};
use orbit::{Observer, parse_tles};

const SAMPLE: &str = include_str!("fixtures/sample.tle");

const OBSERVER: Observer = Observer {
    lat_deg: 41.67,
    lon_deg: -85.71,
    alt_m: 250.0,
};

fn at(h: u32, m: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, h, m, s).unwrap()
}

// (satellite index, time, azimuth deg, elevation deg, range km, range rate km/s)
#[allow(clippy::type_complexity)]
const CASES: [(usize, (u32, u32, u32), f64, f64, f64, f64); 6] = [
    (0, (13, 15, 0), 299.496, 16.344, 1183.517, -6.55728),
    (0, (13, 19, 0), 141.751, 31.142, 764.209, 5.72890),
    (0, (13, 20, 30), 134.988, 13.023, 1334.139, 6.67089),
    (1, (19, 44, 0), 147.026, 25.964, 2623.420, -4.35107),
    (1, (19, 52, 0), 41.749, 35.326, 2208.620, 3.33000),
    (1, (19, 56, 0), 21.071, 14.164, 3261.752, 5.08069),
];

#[test]
fn matches_skyfield_reference() {
    let sats = parse_tles(SAMPLE).unwrap();

    for (idx, (h, m, s), az, el, range, rr) in CASES {
        let got = sats[idx].look_angle(&OBSERVER, at(h, m, s)).unwrap();
        let ctx = format!("{} at {h:02}:{m:02}:{s:02}", sats[idx].name());
        assert!(
            (got.azimuth_deg - az).abs() < 0.05,
            "{ctx}: az {}",
            got.azimuth_deg
        );
        assert!(
            (got.elevation_deg - el).abs() < 0.05,
            "{ctx}: el {}",
            got.elevation_deg
        );
        assert!(
            (got.range_km - range).abs() < 1.0,
            "{ctx}: range {}",
            got.range_km
        );
        assert!(
            (got.range_rate_km_s - rr).abs() < 0.01,
            "{ctx}: range rate {}",
            got.range_rate_km_s
        );
    }
}
