use orbit::parse_tles;

const SAMPLE: &str = include_str!("fixtures/sample.tle");

#[test]
fn parses_celestrak_three_line_sets() {
    let sats = parse_tles(SAMPLE).unwrap();

    let names: Vec<_> = sats.iter().map(|s| s.name()).collect();
    assert_eq!(names, ["ISS (ZARYA)", "RS-44 & BREEZE-KM R/B"]);
    let ids: Vec<_> = sats.iter().map(|s| s.norad_id()).collect();
    assert_eq!(ids, [25544, 44909]);
}

#[test]
fn rejects_a_line_with_a_bad_checksum() {
    // Last digit of ISS line 1 changed from 5 to 6.
    let corrupted = SAMPLE.replacen("0  9995", "0  9996", 1);

    let err = parse_tles(&corrupted)
        .err()
        .expect("checksum must be rejected");

    assert!(matches!(err, orbit::ParseError::Tle { ref name, .. } if name == "ISS (ZARYA)"));
}

#[test]
fn rejects_a_line_1_with_no_line_2() {
    let truncated = SAMPLE.lines().take(2).collect::<Vec<_>>().join("\n");

    let err = parse_tles(&truncated)
        .err()
        .expect("missing line 2 must be rejected");

    assert!(matches!(err, orbit::ParseError::MissingLine2 { .. }));
}
