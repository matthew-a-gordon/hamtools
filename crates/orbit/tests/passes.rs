//! Expected passes come from Skyfield 1.54 `find_events` (10 degree threshold), 2026-10-26
//! 12:00Z to 2026-10-27 12:00Z, not from this crate.

use chrono::{DateTime, TimeZone, Utc};
use orbit::{Observer, parse_tles};

const SAMPLE: &str = include_str!("fixtures/sample.tle");

const OBSERVER: Observer = Observer {
    lat_deg: 41.67,
    lon_deg: -85.71,
    alt_m: 250.0,
};

fn t(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn window() -> (DateTime<Utc>, DateTime<Utc>) {
    (
        Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 10, 27, 12, 0, 0).unwrap(),
    )
}

// (aos, tca, los, max elevation deg, aos azimuth deg, los azimuth deg)
type Expected = (&'static str, &'static str, &'static str, f64, f64, f64);

const ISS: [Expected; 5] = [
    (
        "2026-10-26T13:14:10.26Z",
        "2026-10-26T13:17:33.49Z",
        "2026-10-26T13:20:55.81Z",
        69.815,
        301.284,
        134.168,
    ),
    (
        "2026-10-27T05:57:35.31Z",
        "2026-10-27T06:00:39.48Z",
        "2026-10-27T06:03:45.35Z",
        33.757,
        203.301,
        70.651,
    ),
    (
        "2026-10-27T07:34:20.33Z",
        "2026-10-27T07:37:27.56Z",
        "2026-10-27T07:40:36.00Z",
        34.054,
        266.725,
        41.892,
    ),
    (
        "2026-10-27T09:12:50.98Z",
        "2026-10-27T09:15:00.93Z",
        "2026-10-27T09:17:11.27Z",
        15.731,
        314.356,
        33.927,
    ),
    (
        "2026-10-27T10:50:13.39Z",
        "2026-10-27T10:52:47.04Z",
        "2026-10-27T10:55:20.44Z",
        19.408,
        325.821,
        63.525,
    ),
];

const RS44: [Expected; 5] = [
    (
        "2026-10-26T19:40:27.08Z",
        "2026-10-26T19:48:50.52Z",
        "2026-10-26T19:57:00.89Z",
        48.405,
        159.148,
        18.549,
    ),
    (
        "2026-10-26T21:34:12.30Z",
        "2026-10-26T21:42:22.96Z",
        "2026-10-26T21:50:23.00Z",
        39.644,
        220.467,
        355.005,
    ),
    (
        "2026-10-27T05:35:30.02Z",
        "2026-10-27T05:39:49.46Z",
        "2026-10-27T05:43:59.35Z",
        15.384,
        25.111,
        93.601,
    ),
    (
        "2026-10-27T07:26:49.19Z",
        "2026-10-27T07:34:43.48Z",
        "2026-10-27T07:42:03.84Z",
        83.030,
        353.107,
        170.628,
    ),
    (
        "2026-10-27T09:22:21.39Z",
        "2026-10-27T09:26:56.61Z",
        "2026-10-27T09:31:23.49Z",
        17.160,
        319.809,
        245.288,
    ),
];

fn close_in_time(got: DateTime<Utc>, want: &str, what: &str) {
    let diff = (got - t(want)).num_milliseconds().abs();
    assert!(
        diff < 2000,
        "{what}: got {got}, want {want} (off by {diff} ms)"
    );
}

fn check(sat_index: usize, expected: &[Expected]) {
    let sat = &parse_tles(SAMPLE).unwrap()[sat_index];
    let (from, to) = window();

    let passes = sat.passes(&OBSERVER, from, to, 10.0).unwrap();

    assert_eq!(passes.len(), expected.len(), "{}: pass count", sat.name());
    for (p, (aos, tca, los, max_el, aos_az, los_az)) in passes.iter().zip(expected) {
        close_in_time(p.aos, aos, "AOS");
        close_in_time(p.tca, tca, "TCA");
        close_in_time(p.los, los, "LOS");
        assert!(
            (p.max_elevation_deg - max_el).abs() < 0.1,
            "max el {}",
            p.max_elevation_deg
        );
        assert!(
            (p.aos_azimuth_deg - aos_az).abs() < 0.3,
            "AOS az {}",
            p.aos_azimuth_deg
        );
        assert!(
            (p.los_azimuth_deg - los_az).abs() < 0.3,
            "LOS az {}",
            p.los_azimuth_deg
        );
    }
}

#[test]
fn iss_passes_match_skyfield() {
    check(0, &ISS);
}

#[test]
fn rs44_passes_match_skyfield() {
    check(1, &RS44);
}

// A pass is reported whole, even when the search window starts or ends part-way through it, so
// its length and peak do not depend on when we happened to look. Times are Skyfield's for the
// ISS pass 13:14:10 - 13:20:55 (peak 69.8 degrees at 13:17:33).

#[test]
fn a_pass_under_way_at_the_window_start_reports_its_true_start() {
    let sat = &parse_tles(SAMPLE).unwrap()[0];
    let from = t("2026-10-26T13:17:00Z");
    let to = t("2026-10-26T13:30:00Z");

    let passes = sat.passes(&OBSERVER, from, to, 10.0).unwrap();

    assert_eq!(passes.len(), 1);
    close_in_time(passes[0].aos, "2026-10-26T13:14:10.26Z", "AOS");
    close_in_time(passes[0].tca, "2026-10-26T13:17:33.49Z", "TCA");
    close_in_time(passes[0].los, "2026-10-26T13:20:55.81Z", "LOS");
    assert!((passes[0].max_elevation_deg - 69.815).abs() < 0.1);
    assert!((passes[0].aos_azimuth_deg - 301.284).abs() < 0.3);
}

#[test]
fn a_pass_under_way_at_the_window_end_reports_its_true_end() {
    let sat = &parse_tles(SAMPLE).unwrap()[0];
    let from = t("2026-10-26T12:00:00Z");
    let to = t("2026-10-26T13:17:00Z");

    let passes = sat.passes(&OBSERVER, from, to, 10.0).unwrap();

    assert_eq!(passes.len(), 1);
    close_in_time(passes[0].aos, "2026-10-26T13:14:10.26Z", "AOS");
    close_in_time(passes[0].los, "2026-10-26T13:20:55.81Z", "LOS");
    assert!((passes[0].los_azimuth_deg - 134.168).abs() < 0.3);
}
