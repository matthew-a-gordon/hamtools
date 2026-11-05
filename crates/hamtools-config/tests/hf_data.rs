//! The corrected privilege rows and the HF rows of the ARRL layer (docs/research/missing-data.md).

use hamtools_config::LicenseClass;
use hamtools_config::bandplan::BandPlan;

#[test]
fn novice_has_no_privileges_on_vhf_uhf_except_the_97_301_e_segments() {
    let plan = BandPlan::embedded();
    for hz in [
        50_100_000,
        146_520_000,
        446_000_000,
        915_000_000,
        1_250_000_000,
        1_297_000_000,
    ] {
        assert!(!plan.in_privilege(hz, LicenseClass::Novice), "{hz}");
        assert!(plan.in_privilege(hz, LicenseClass::Technician), "{hz}");
    }
    assert!(plan.in_privilege(223_500_000, LicenseClass::Novice));
    assert!(plan.in_privilege(1_270_000_000, LicenseClass::Novice));
    assert!(plan.in_privilege(1_294_999_999, LicenseClass::Novice));
    assert!(!plan.in_privilege(1_295_000_001, LicenseClass::Novice));
}

#[test]
fn the_stale_sixty_metre_channel_count_comment_is_gone() {
    let privileges = include_str!("../data/privileges.toml");
    assert!(!privileges.contains("disagrees"));
    assert!(privileges.contains("91 FR 1430"));
}

fn arrl_mode_at(hz: u64) -> Option<String> {
    BandPlan::embedded()
        .arrl
        .rows
        .iter()
        .find(|r| (r.lo_hz..=r.hi_hz).contains(&hz))
        .and_then(|r| r.mode.clone())
}

#[test]
fn every_hf_band_has_a_usual_mode_for_cw_and_phone() {
    for (cw, phone) in [
        (1_820_000, 1_900_000),
        (3_520_000, 3_850_000),
        (7_020_000, 7_200_000),
        (10_110_000, 0),
        (14_030_000, 14_250_000),
        (18_080_000, 18_130_000),
        (21_030_000, 21_300_000),
        (24_900_000, 24_950_000),
        (28_030_000, 28_400_000),
    ] {
        assert_eq!(arrl_mode_at(cw).as_deref(), Some("CW"), "{cw}");
        if phone != 0 {
            assert_eq!(arrl_mode_at(phone).as_deref(), Some("phone"), "{phone}");
        }
    }
}

#[test]
fn rows_whose_cw_edge_is_not_on_the_arrl_page_are_labelled_unverified() {
    let rows = &BandPlan::embedded().arrl.rows;
    let unverified = |hz: u64| {
        let r = rows
            .iter()
            .find(|r| (r.lo_hz..=r.hi_hz).contains(&hz))
            .unwrap();
        r.label.contains("UNVERIFIED") && r.source.contains("UNVERIFIED")
    };
    for hz in [
        1_820_000, 3_520_000, 7_020_000, 10_110_000, 14_030_000, 18_080_000, 21_030_000, 24_900_000,
    ] {
        assert!(unverified(hz), "{hz}");
    }
    assert!(!unverified(28_030_000));
}
