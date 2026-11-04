mod common;

use chrono::{TimeZone, Utc};
use common::FakeRigctld;
use orbit::{Observer, parse_tles};
use rigctld_client::{Client, Error};
use sat_tui::rig::apply;
use sat_tui::satnogs::{RigMode, parse_transmitters};
use sat_tui::tracker::{RigCommand, Tracker};

async fn connect(rig: &FakeRigctld) -> Client {
    Client::connect(rig.addr).await.unwrap()
}

#[tokio::test]
async fn commands_become_the_hamlib_calls_that_drive_the_ft847_satellite_vfos() {
    // Hamlib's ft847 backend: split on = SAT mode on; plain set_freq/set_mode = SAT RX VFO;
    // set_split_freq/set_split_mode = SAT TX VFO.
    let rig = FakeRigctld::start().await;
    let mut client = connect(&rig).await;

    apply(
        &mut client,
        &[
            RigCommand::SatMode(true),
            RigCommand::RxMode(RigMode::Usb),
            RigCommand::TxMode(RigMode::Lsb),
            RigCommand::RxFreq(435_646_320),
            RigCommand::TxFreq(145_962_880),
            RigCommand::SatMode(false),
        ],
    )
    .await
    .unwrap();

    assert_eq!(
        rig.lines(),
        [
            "+S 1 VFOB",
            "+M USB 0",
            "+X LSB 0",
            "+F 435646320",
            "+I 145962880",
            "+S 0 VFOA",
        ]
    );
}

#[tokio::test]
async fn the_first_failure_stops_the_batch_and_is_reported() {
    let rig = FakeRigctld::start_failing_on("+M ").await;
    let mut client = connect(&rig).await;

    let err = apply(
        &mut client,
        &[
            RigCommand::SatMode(true),
            RigCommand::RxMode(RigMode::Fm),
            RigCommand::RxFreq(145_800_000),
        ],
    )
    .await
    .unwrap_err();

    assert!(matches!(err, Error::Rig(-1)), "{err:?}");
    assert_eq!(rig.lines(), ["+S 1 VFOB", "+M FM 0"]);
}

#[tokio::test]
async fn a_tracker_step_drives_the_rig_end_to_end() {
    let sat = parse_tles(include_str!("fixtures/sample.tle"))
        .unwrap()
        .remove(0);
    let mut tracker = Tracker::new(
        sat,
        Observer {
            lat_deg: 41.67,
            lon_deg: -85.71,
            alt_m: 250.0,
        },
        0.0,
    );
    let plan = parse_transmitters(include_str!("fixtures/satnogs_transmitters.json"))
        .unwrap()
        .into_iter()
        .find(|t| t.norad_id == 25544 && t.description == "Mode V/V FM (crew R2+3)")
        .unwrap()
        .plan(0)
        .unwrap();
    let rig = FakeRigctld::start().await;
    let mut client = connect(&rig).await;

    let commands = tracker
        .step(
            &plan,
            Utc.with_ymd_and_hms(2026, 10, 26, 13, 15, 0).unwrap(),
        )
        .unwrap();
    apply(&mut client, &commands).await.unwrap();

    let lines = rig.lines();
    assert_eq!(&lines[..3], ["+S 1 VFOB", "+M FM 0", "+X FM 0"]);
    let freq = |prefix: &str| -> f64 {
        lines
            .iter()
            .find_map(|l| l.strip_prefix(prefix))
            .unwrap()
            .parse()
            .unwrap()
    };
    // Same Skyfield-derived expectations as the tracker tests.
    assert!((freq("+F ") - 145_803_189.04).abs() <= 25.0);
    assert!((freq("+I ") - 144_486_839.61).abs() <= 25.0);
}
