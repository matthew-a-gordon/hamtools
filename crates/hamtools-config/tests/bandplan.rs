use hamtools_config::LicenseClass;
use hamtools_config::bandplan::{BandPlan, Layer};
use logbook::BANDS;

fn layers(plan: &BandPlan) -> [(&'static str, &Layer); 3] {
    [
        ("privilege", &plan.privilege),
        ("arrl", &plan.arrl),
        ("satellite", &plan.satellite),
    ]
}

#[test]
fn the_embedded_files_parse_and_every_layer_has_rows() {
    let plan = BandPlan::embedded();
    for (name, layer) in layers(plan) {
        assert!(!layer.rows.is_empty(), "{name} has no rows");
    }
    assert!(!plan.channels.is_empty());
}

#[test]
fn segments_are_sorted_and_do_not_overlap_within_a_layer() {
    for (name, layer) in layers(BandPlan::embedded()) {
        for pair in layer.rows.windows(2) {
            assert!(
                pair[0].hi_hz <= pair[1].lo_hz,
                "{name}: {}-{} then {}-{}",
                pair[0].lo_hz,
                pair[0].hi_hz,
                pair[1].lo_hz,
                pair[1].hi_hz
            );
        }
        for row in &layer.rows {
            assert!(row.lo_hz <= row.hi_hz, "{name}: {row:?}");
        }
    }
}

#[test]
fn every_row_has_a_source_and_every_layer_an_as_of_date() {
    let plan = BandPlan::embedded();
    for (name, layer) in layers(plan) {
        let bytes = layer.as_of.as_bytes();
        assert!(
            bytes.len() == 10 && bytes[4] == b'-' && bytes[7] == b'-',
            "{name} as_of {:?}",
            layer.as_of
        );
        for row in &layer.rows {
            assert!(!row.source.trim().is_empty(), "{name}: {row:?}");
        }
    }
    for channel in &plan.channels {
        assert!(!channel.source.trim().is_empty());
    }
}

#[test]
fn edges_lie_inside_their_band() {
    let plan = BandPlan::embedded();
    for (name, layer) in layers(plan) {
        for row in &layer.rows {
            let band = BANDS
                .iter()
                .find(|b| b.name == row.band)
                .unwrap_or_else(|| panic!("{name}: unknown band {}", row.band));
            assert!(
                row.lo_hz >= band.lower_hz && row.hi_hz <= band.upper_hz,
                "{name}: {row:?} outside {band:?}"
            );
        }
    }
    for channel in &plan.channels {
        let band = BANDS.iter().find(|b| b.name == channel.band).unwrap();
        assert!((band.lower_hz..=band.upper_hz).contains(&channel.carrier_hz));
    }
}

#[test]
fn privileges_follow_the_license_class() {
    let plan = BandPlan::embedded();
    // 14.200 MHz: Extra and Advanced only.
    assert!(plan.in_privilege(14_200_000, LicenseClass::Extra));
    assert!(plan.in_privilege(14_200_000, LicenseClass::Advanced));
    assert!(!plan.in_privilege(14_200_000, LicenseClass::General));
    assert!(!plan.in_privilege(14_200_000, LicenseClass::Technician));
    // 14.250 MHz: General too.
    assert!(plan.in_privilege(14_250_000, LicenseClass::General));
    // Outside every band.
    assert!(!plan.in_privilege(13_000_000, LicenseClass::Extra));
    // 146.52 MHz: everyone.
    assert!(plan.in_privilege(146_520_000, LicenseClass::Technician));
}

#[test]
fn sixty_metres_is_a_channel_list_not_a_band_edge() {
    let plan = BandPlan::embedded();
    // Between channels, outside the contiguous segment: not a privilege.
    assert!(!plan.in_privilege(5_340_000, LicenseClass::Extra));
    assert!(plan.in_privilege(5_330_500, LicenseClass::General));
    assert!(!plan.in_privilege(5_330_500, LicenseClass::Technician));
}

#[test]
fn bands_lists_the_us_bands_in_frequency_order_without_4m() {
    let names: Vec<_> = BandPlan::embedded()
        .bands()
        .iter()
        .map(|b| b.name)
        .collect();
    assert_eq!(names.first(), Some(&"160m"));
    assert!(names.contains(&"60m") && names.contains(&"20m"));
    assert!(!names.contains(&"4m"));
    let lows: Vec<_> = BandPlan::embedded()
        .bands()
        .iter()
        .map(|b| b.lower_hz)
        .collect();
    assert!(lows.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn the_oldest_layer_date_is_reported() {
    assert_eq!(BandPlan::embedded().as_of(), "2026-11-04");
}

const PRIV: &str = include_str!("../data/privileges.toml");
const ARRL: &str = include_str!("../data/arrl.toml");
const SAT: &str = include_str!("../data/satellite.toml");

#[test]
fn a_row_with_no_source_is_rejected() {
    let bad = SAT.replacen("source = \"fcc-97.207(c)\"", "source = \"\"", 1);
    let err = BandPlan::parse(PRIV, ARRL, &bad).unwrap_err();
    assert!(err.to_string().contains("source"), "{err}");
}

#[test]
fn overlapping_or_unsorted_rows_are_rejected() {
    let bad = ARRL.replacen("lo_hz = 28_150_000", "lo_hz = 28_050_000", 1);
    let err = BandPlan::parse(PRIV, &bad, SAT).unwrap_err();
    assert!(
        err.to_string().contains("overlap") || err.to_string().contains("sorted"),
        "{err}"
    );
}

#[test]
fn an_edge_outside_its_band_is_rejected() {
    let bad = SAT.replacen("hi_hz = 14_250_000", "hi_hz = 14_400_000", 1);
    let err = BandPlan::parse(PRIV, ARRL, &bad).unwrap_err();
    assert!(err.to_string().contains("band"), "{err}");
}
