//! Expected frequencies: nominal frequency from the SatNOGS fixture, Doppler from Skyfield's range
//! rate at the same instant (see the orbit crate's tests), applied with awk using
//! f_rx = f (1 - v/c), f_tx = f (1 + v/c), c = 299792.458 km/s. Tolerance 25 Hz covers the small
//! difference between propagators plus rounding to the radio's 10 Hz step.

use chrono::{DateTime, TimeZone, Utc};
use orbit::{Observer, parse_tles};
use sat_tui::satnogs::{RigMode, Transmitter, parse_transmitters};
use sat_tui::tracker::{RigCommand, Tracker};

const OBSERVER: Observer = Observer {
    lat_deg: 41.67,
    lon_deg: -85.71,
    alt_m: 250.0,
};

fn at(h: u32, m: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, h, m, s).unwrap()
}

fn transmitter(norad: u32, description: &str) -> Transmitter {
    parse_transmitters(include_str!("fixtures/satnogs_transmitters.json"))
        .unwrap()
        .into_iter()
        .find(|t| t.norad_id == norad && t.description == description)
        .unwrap()
}

/// The ISS (index 0) or RS-44 (index 1) from the shared TLE fixture.
fn tracker(index: usize) -> Tracker {
    let sat = parse_tles(include_str!("fixtures/sample.tle"))
        .unwrap()
        .remove(index);
    Tracker::new(sat, OBSERVER)
}

fn iss_fm() -> Transmitter {
    transmitter(25544, "Mode V/V FM (crew R2+3)")
}

fn rx(commands: &[RigCommand]) -> Option<u64> {
    commands.iter().find_map(|c| match c {
        RigCommand::RxFreq(hz) => Some(*hz),
        _ => None,
    })
}

fn tx(commands: &[RigCommand]) -> Option<u64> {
    commands.iter().find_map(|c| match c {
        RigCommand::TxFreq(hz) => Some(*hz),
        _ => None,
    })
}

fn close(got: u64, want: f64) {
    assert!(
        (got as f64 - want).abs() <= 25.0,
        "got {got}, want about {want}"
    );
    assert_eq!(got % 10, 0, "{got} is not on the radio's 10 Hz grid");
}

#[test]
fn the_first_step_sets_up_the_radio_with_doppler_corrected_frequencies() {
    let mut tracker = tracker(0);
    let plan = iss_fm().plan(0).unwrap();

    let commands = tracker.step(&plan, at(13, 15, 0)).unwrap();

    // Approaching at 6.56 km/s: downlink pushed up, uplink pulled down.
    assert_eq!(
        &commands[..3],
        [
            RigCommand::SatMode(true),
            RigCommand::RxMode(RigMode::Fm),
            RigCommand::TxMode(RigMode::Fm)
        ]
    );
    close(rx(&commands).unwrap(), 145_803_189.04);
    close(tx(&commands).unwrap(), 144_486_839.61);
}

#[test]
fn an_unchanged_situation_sends_nothing() {
    let mut tracker = tracker(0);
    let plan = iss_fm().plan(0).unwrap();
    tracker.step(&plan, at(13, 15, 0)).unwrap();

    let commands = tracker.step(&plan, at(13, 15, 0)).unwrap();

    assert_eq!(commands, []);
}

#[test]
fn as_the_satellite_moves_only_the_frequencies_are_resent() {
    let mut tracker = tracker(0);
    let plan = iss_fm().plan(0).unwrap();
    tracker.step(&plan, at(13, 15, 0)).unwrap();

    let commands = tracker.step(&plan, at(13, 19, 0)).unwrap();

    // Receding now at 5.73 km/s.
    assert_eq!(commands.len(), 2, "{commands:?}");
    close(rx(&commands).unwrap(), 145_797_213.83);
    close(tx(&commands).unwrap(), 144_492_761.14);
}

#[test]
fn below_the_horizon_the_radio_is_set_up_once_then_left_alone() {
    let mut tracker = tracker(0);
    let plan = iss_fm().plan(0).unwrap();

    let first = tracker.step(&plan, at(12, 0, 0)).unwrap();
    let later = tracker.step(&plan, at(12, 0, 30)).unwrap();

    close(rx(&first).unwrap(), 145_797_243.97);
    assert_eq!(first.len(), 5, "{first:?}");
    assert_eq!(later, []);
}

#[test]
fn updates_resume_when_the_satellite_rises() {
    let mut tracker = tracker(0);
    let plan = iss_fm().plan(0).unwrap();
    tracker.step(&plan, at(12, 0, 0)).unwrap();

    let commands = tracker.step(&plan, at(13, 15, 0)).unwrap();

    close(rx(&commands).unwrap(), 145_803_189.04);
}

#[test]
fn nudging_the_transponder_resends_only_the_frequencies_that_moved() {
    let linear = transmitter(44909, "Mode V/U - Transponder");
    let mut tracker = tracker(1);
    tracker
        .step(&linear.plan(0).unwrap(), at(19, 44, 0))
        .unwrap();

    let commands = tracker
        .step(&linear.plan(1_000).unwrap(), at(19, 44, 0))
        .unwrap();

    assert_eq!(commands.len(), 2, "{commands:?}");
    close(rx(&commands).unwrap(), 435_647_322.72);
    close(tx(&commands).unwrap(), 145_961_881.54);
}

#[test]
fn a_receive_only_plan_never_touches_satellite_mode_or_the_uplink() {
    let beacon = transmitter(44909, "Mode U - Beacon");
    let mut tracker = tracker(1);

    let commands = tracker
        .step(&beacon.plan(0).unwrap(), at(19, 44, 0))
        .unwrap();

    assert_eq!(commands.len(), 2, "{commands:?}");
    assert_eq!(commands[0], RigCommand::RxMode(RigMode::Cw));
    close(rx(&commands).unwrap(), 435_611_322.20);
    assert_eq!(tx(&commands), None);
}

#[test]
fn switching_to_a_different_transmitter_resends_its_modes() {
    let mut tracker = tracker(1);
    tracker
        .step(
            &transmitter(44909, "Mode U - Beacon").plan(0).unwrap(),
            at(19, 44, 0),
        )
        .unwrap();
    let linear = transmitter(44909, "Mode V/U - Transponder")
        .plan(0)
        .unwrap();

    let commands = tracker.step(&linear, at(19, 44, 0)).unwrap();

    assert!(commands.contains(&RigCommand::SatMode(true)));
    assert!(commands.contains(&RigCommand::RxMode(RigMode::Usb)));
    assert!(commands.contains(&RigCommand::TxMode(RigMode::Lsb)));
    close(rx(&commands).unwrap(), 435_646_322.71);
    close(tx(&commands).unwrap(), 145_962_881.52);
}
