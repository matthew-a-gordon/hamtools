//! Fixture: real SatNOGS DB `/api/transmitters/` records for ISS, SO-50, AO-7 and RS-44,
//! downloaded 2026-10-26. Expected values below are read off those records by hand
//! (frequency ranges, modes, `invert`), not computed by the code under test.

use sat_tui::satnogs::{Kind, RigMode, Transmitter, parse_transmitters};

const JSON: &str = include_str!("fixtures/satnogs_transmitters.json");

fn all() -> Vec<Transmitter> {
    parse_transmitters(JSON).unwrap()
}

fn find(norad: u32, description: &str) -> Transmitter {
    all()
        .into_iter()
        .find(|t| t.norad_id == norad && t.description == description)
        .unwrap_or_else(|| panic!("no transmitter {norad} {description:?}"))
}

#[test]
fn parses_every_record_in_the_real_api_response() {
    assert_eq!(all().len(), 64);
}

#[test]
fn reads_the_rs44_inverting_transponder() {
    let t = find(44909, "Mode V/U - Transponder");

    assert_eq!(t.kind, Kind::Transponder);
    assert!(t.invert);
    assert_eq!(t.uplink.as_ref().unwrap().low_hz, 145_935_000);
    assert_eq!(t.uplink.as_ref().unwrap().high_hz, Some(145_995_000));
    assert_eq!(t.downlink.low_hz, 435_610_000);
    assert_eq!(t.downlink.high_hz, Some(435_670_000));
}

#[test]
fn only_alive_active_transmitters_are_usable() {
    assert!(find(25544, "Mode V/V FM (crew R2+3)").is_usable());
    assert!(!find(25544, "Tomsk-TPU-120").is_usable());
}

#[test]
fn an_fm_repeater_plans_one_frequency_each_way() {
    let plan = find(25544, "Mode V/V FM (crew R2+3)").plan(0).unwrap();

    assert_eq!(plan.downlink_hz, 145_800_000);
    assert_eq!(plan.uplink_hz, Some(144_490_000));
    assert_eq!(plan.rx_mode, RigMode::Fm);
    assert_eq!(plan.tx_mode, Some(RigMode::Fm));
}

#[test]
fn so50_plans_v_up_u_down_fm() {
    let plan = find(27607, "Mode V/U FM Voice CTCSS 67.0 Hz")
        .plan(0)
        .unwrap();

    assert_eq!(plan.downlink_hz, 436_795_000);
    assert_eq!(plan.uplink_hz, Some(145_850_000));
}

#[test]
fn an_inverting_transponder_starts_at_the_centre_with_opposite_sidebands() {
    let plan = find(44909, "Mode V/U - Transponder").plan(0).unwrap();

    // Centre of 435.610-435.670 is 435.640; inverting, so it pairs with 145.995 - 30 kHz.
    assert_eq!(plan.downlink_hz, 435_640_000);
    assert_eq!(plan.uplink_hz, Some(145_965_000));
    assert_eq!(plan.rx_mode, RigMode::Usb);
    assert_eq!(plan.tx_mode, Some(RigMode::Lsb));
}

#[test]
fn nudging_moves_the_uplink_the_opposite_way_on_an_inverting_transponder() {
    let plan = find(44909, "Mode V/U - Transponder").plan(1_000).unwrap();

    assert_eq!(plan.downlink_hz, 435_641_000);
    assert_eq!(plan.uplink_hz, Some(145_964_000));
}

#[test]
fn nudging_moves_both_the_same_way_on_a_non_inverting_transponder() {
    // AO-7 mode A: 145.850-145.950 up, 29.400-29.500 down, USB both ways.
    let t = find(7530, "Mode V/A (A) Lin SSB");

    let centre = t.plan(0).unwrap();
    assert_eq!(centre.downlink_hz, 29_450_000);
    assert_eq!(centre.uplink_hz, Some(145_900_000));

    let nudged = t.plan(1_000).unwrap();
    assert_eq!(nudged.downlink_hz, 29_451_000);
    assert_eq!(nudged.uplink_hz, Some(145_901_000));
}

#[test]
fn nudging_stops_at_the_edge_of_the_passband() {
    let plan = find(44909, "Mode V/U - Transponder")
        .plan(1_000_000)
        .unwrap();

    assert_eq!(plan.downlink_hz, 435_670_000);
    assert_eq!(plan.uplink_hz, Some(145_935_000));
}

#[test]
fn a_beacon_is_receive_only() {
    let plan = find(44909, "Mode U - Beacon").plan(0).unwrap();

    assert_eq!(plan.downlink_hz, 435_605_000);
    assert_eq!(plan.uplink_hz, None);
    assert_eq!(plan.rx_mode, RigMode::Cw);
    assert_eq!(plan.tx_mode, None);
}

#[test]
fn modes_the_radio_cannot_demodulate_have_no_plan() {
    let bpsk = find(44909, "Mode U Audio");

    assert_eq!(bpsk.plan(0), None);
}
