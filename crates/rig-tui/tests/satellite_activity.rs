//! The Satellite Activity inside the host `App`, with no rig, terminal or network. Everything
//! about the FT-847 here is untested on hardware: the FT-847 profile only supplies the
//! `sat_mode` capability from the static table.
//!
//! Skyfield look angles (see the orbit crate's tests): the ISS is over Middlebury from 13:14:10
//! to 13:20:55Z on 2026-10-26, approaching at 13:15, so the downlink is heard high and the
//! uplink is sent low.

mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, TimeZone, Utc};
use common::*;
use hamtools_config::{RigKind, RigProfile, SatelliteSettings};
use orbit::{Observer, parse_tles};
use ratatui::crossterm::event::KeyCode;
use rig_tui::satellite::{Satellite, SatelliteData};
use rig_tui::{App, Command, RigEvent, RigMode};
use sat_tui::satnogs::parse_transmitters;
use sat_tui::tracker::RigCommand;

const SATELLITE: usize = 1;

fn t(h: u32, m: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, h, m, s).unwrap()
}

fn profile(name: &str, kind: RigKind) -> RigProfile {
    RigProfile {
        name: name.into(),
        addr: "127.0.0.1:1".into(),
        kind,
        cw: None,
        split: None,
        sat_mode: None,
    }
}

fn satellite() -> Satellite {
    let data = SatelliteData {
        sats: parse_tles(include_str!("fixtures/sample.tle")).unwrap(),
        transmitters: parse_transmitters(include_str!("fixtures/satnogs_transmitters.json"))
            .unwrap(),
        tle_fetched: Some(t(10, 0, 0)),
        transmitters_fetched: Some(t(8, 0, 0)),
    };
    let observer = Observer {
        lat_deg: 41.67,
        lon_deg: -85.71,
        alt_m: 250.0,
    };
    Satellite::new(data, observer, SatelliteSettings::default(), t(12, 0, 0))
}

fn temp_log() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rig-tui-satellite-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("log.adi")
}

/// `kx3` first, `ft847` second, with the Satellite Activity hosted (index 1).
fn app(start_on: &str) -> App {
    let mut app = App::with_callsign(Some("W1AW".to_string()))
        .with_log_path(Some(temp_log()))
        .with_rigs(
            &[
                profile("kx3", RigKind::Kx3),
                profile("ft847", RigKind::Ft847),
            ],
            Some(start_on),
        );
    assert_eq!(app.host_activity(Box::new(satellite())), SATELLITE);
    app.set_utc(SystemTime::from(t(13, 15, 0)));
    app
}

fn connected(mut app: App) -> App {
    app.on_rig(RigEvent::Connected);
    app
}

fn doppler(commands: &[Command]) -> Vec<RigCommand> {
    commands
        .iter()
        .filter_map(|c| match c {
            Command::Doppler(step) => Some(*step),
            _ => None,
        })
        .collect()
}

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

#[test]
fn satellite_is_unavailable_on_a_rig_without_sat_mode_and_says_why() {
    let mut app = app("kx3");

    assert_eq!(app.activity_name(), "General");
    let why = app.activity_unavailable(SATELLITE).expect("unavailable");
    assert!(
        why.contains("Satellite") && why.contains("sat mode") && why.contains("kx3"),
        "{why}"
    );
    app.set_foreground(SATELLITE);
    assert_eq!(app.activity_name(), "General");
    assert_eq!(app.status(), Some(why.as_str()));

    // On the FT-847 profile it is available.
    let app = self::app("ft847");
    assert_eq!(app.activity_unavailable(SATELLITE), None);
}

#[test]
fn the_go_to_palette_lists_satellite_with_its_next_pass() {
    let mut app = connected(app("ft847"));
    app.set_utc(SystemTime::from(t(13, 0, 0)));
    app.tick(Instant::now());
    app.on_key(ctrl('g'));

    let rows = screen(&app, 100, 30);

    assert!(has(&rows, "Satellite"), "{rows:#?}");
    assert!(has(&rows, "next pass ISS in 14:10"), "{rows:#?}");
}

#[test]
fn the_screens_are_the_old_sat_tui_ones_under_the_shared_strip() {
    let mut app = connected(app("ft847"));
    app.set_foreground(SATELLITE);
    app.tick(Instant::now());

    let rows = screen(&app, 110, 32);

    assert!(has(&rows, "Satellite | ft847"), "{rows:#?}");
    assert!(has(&rows, "Passes (UTC)"), "{rows:#?}");
    assert!(has(&rows, "ISS (ZARYA)"), "{rows:#?}");
    assert!(has(&rows, "Live"), "{rows:#?}");
    assert!(
        has(&rows, "Transmitters (Tab, t track, u unsupported)"),
        "{rows:#?}"
    );
    assert!(has(&rows, "TLEs 3h"), "{rows:#?}");
    assert!(has(&rows, "SatNOGS 5h"), "{rows:#?}");
    // One clock, in the strip.
    assert_eq!(rows.iter().filter(|r| r.contains("13:15:00Z")).count(), 1);
}

#[test]
fn it_fits_the_80_by_24_floor() {
    let mut app = connected(app("ft847"));
    app.set_foreground(SATELLITE);
    app.tick(Instant::now());

    let rows = screen(&app, 80, 24);

    assert!(has(&rows, "Passes (UTC)"), "{rows:#?}");
    assert!(has(&rows, "Z SAT off"), "{rows:#?}");
    assert!(!has(&rows, "Window too small"), "{rows:#?}");
}

#[test]
fn doppler_is_issued_only_while_satellite_is_in_the_foreground() {
    let start = Instant::now();
    let mut app = connected(app("ft847"));
    app.set_foreground(SATELLITE);
    app.on_key(key(KeyCode::Char('t')));
    // Back in General: nothing is ticked, so nothing is planned.
    app.set_foreground(0);
    assert!(
        app.status()
            .is_some_and(|s| s.contains("Doppler tracking stopped"))
    );
    let from_general = app.tick(start);
    assert_eq!(doppler(&from_general), []);

    // In the foreground and tracking, the first tick sets the radio up.
    app.set_foreground(SATELLITE);
    app.on_key(key(KeyCode::Char('t')));
    let commands = app.tick(start + Duration::from_secs(1));
    let steps = doppler(&commands);
    assert_eq!(steps[0], RigCommand::SatMode(true), "{steps:?}");
    assert!(steps.iter().any(|s| matches!(s, RigCommand::RxFreq(_))));
    assert!(steps.iter().any(|s| matches!(s, RigCommand::TxFreq(_))));
    // None of it can key the transmitter.
    assert!(commands.iter().all(|c| !c.transmits()));
}

#[test]
fn leaving_satellite_stops_tracking_says_so_and_does_not_retune() {
    let start = Instant::now();
    let mut app = connected(app("ft847"));
    app.set_foreground(SATELLITE);
    app.on_key(key(KeyCode::Char('t')));
    app.tick(start);

    app.set_foreground(0);

    let status = app.status().expect("status");
    assert!(status.contains("Doppler tracking stopped"), "{status}");
    // Nothing is sent to the radio to restore it, now or later.
    for step in 1..4 {
        let commands = app.tick(start + Duration::from_secs(step));
        assert_eq!(doppler(&commands), []);
        assert!(
            commands.iter().all(|c| !matches!(
                c,
                Command::SetFreq(_)
                    | Command::SetMode { .. }
                    | Command::SetSplit { .. }
                    | Command::SetSplitFreq(_)
            )),
            "{commands:?}"
        );
    }
    // Back in Satellite, it is not tracking until asked.
    app.set_foreground(SATELLITE);
    assert_eq!(doppler(&app.tick(start + Duration::from_secs(9))), []);
}

#[test]
fn the_strip_keeps_saying_tracking_stopped_until_satellite_is_back_and_t_resumes() {
    let start = Instant::now();
    let mut app = connected(app("ft847"));
    app.set_foreground(SATELLITE);
    app.on_key(key(KeyCode::Char('t')));
    app.tick(start);
    assert!(!has(&screen(&app, 100, 30), "Doppler stopped"));

    app.set_foreground(0);
    // The one-shot status goes; the strip still says it.
    app.set_status(None);
    assert!(has(&screen(&app, 100, 30), "Doppler stopped"));

    // Back in Satellite: the strip is clear and the status says what `t` picks up.
    app.set_foreground(SATELLITE);
    app.tick(start + Duration::from_secs(1));
    assert!(!has(&screen(&app, 100, 30), "Doppler stopped"));
    assert!(
        app.status().is_some_and(|s| s.contains("t resumes ISS")),
        "{:?}",
        app.status()
    );
    // Still nothing is sent until the operator asks.
    assert_eq!(doppler(&app.tick(start + Duration::from_secs(2))), []);
}

#[test]
fn switching_to_a_rig_without_sat_mode_drops_to_general_and_stops_tracking() {
    let start = Instant::now();
    let mut app = connected(app("ft847"));
    app.set_foreground(SATELLITE);
    app.on_key(key(KeyCode::Char('t')));
    app.tick(start);

    app.switch_rig(0);

    assert_eq!(app.activity_name(), "General");
    let status = app.status().expect("status");
    assert!(status.contains("back to General"), "{status}");
    assert!(status.contains("Doppler tracking stopped"), "{status}");
    assert_eq!(doppler(&app.tick(start + Duration::from_secs(2))), []);
}

#[test]
fn the_dial_is_followed_from_the_strips_receive_frequency() {
    let start = Instant::now();
    let mut app = connected(app("ft847"));
    app.set_foreground(SATELLITE);
    // The RS-44 transponder: the pass at 19:40, but any time works for the nudge.
    app.on_key(key(KeyCode::Down));
    app.on_key(key(KeyCode::Tab));
    app.on_key(key(KeyCode::Down));
    app.on_key(key(KeyCode::Char('t')));
    app.set_utc(SystemTime::from(t(19, 44, 0)));
    let setup = app.tick(start);
    let rx0 = doppler(&setup)
        .into_iter()
        .find_map(|s| match s {
            RigCommand::RxFreq(hz) => Some(hz),
            _ => None,
        })
        .expect("rx frequency");

    // The operator turns the dial up 150 Hz; the strip reads it back.
    app.on_rig(RigEvent::Freq(rx0 + 150));
    let next = app.tick(start + Duration::from_secs(1));

    let rx1 = doppler(&next)
        .into_iter()
        .find_map(|s| match s {
            RigCommand::RxFreq(hz) => Some(hz),
            _ => None,
        })
        .expect("rx frequency");
    assert!((rx1 as i64 - (rx0 + 150) as i64).abs() <= 25, "{rx0} {rx1}");
}

#[test]
fn tuning_from_the_band_plan_is_refused_while_tracking() {
    let start = Instant::now();
    let mut app = connected(app("ft847"));
    app.on_rig(RigEvent::Freq(145_900_000));
    app.set_foreground(SATELLITE);
    app.on_key(key(KeyCode::Char('t')));
    app.tick(start);

    app.on_key(ctrl('b'));
    let commands = app.on_key(key(KeyCode::Enter));

    assert_eq!(commands, []);
    assert!(
        app.status().is_some_and(|s| s.contains("Doppler tracking")),
        "{:?}",
        app.status()
    );
}

#[test]
fn quitting_while_tracking_warns_and_never_refuses() {
    let mut app = connected(app("ft847"));
    app.set_foreground(SATELLITE);
    app.on_key(key(KeyCode::Char('t')));

    assert_eq!(app.on_key(ctrl('c')), []);
    let rows = screen(&app, 100, 30);
    assert!(has(&rows, "Doppler tracking is running"), "{rows:#?}");

    assert_eq!(app.on_key(ctrl('c')), vec![Command::Quit]);
}

#[test]
fn z_is_sat_mode_off_s_is_split_and_m_is_the_shared_mode_key() {
    let start = Instant::now();
    let mut app = connected(app("ft847"));
    app.on_rig(RigEvent::Mode {
        mode: RigMode::Usb,
        passband: 0,
    });
    app.set_foreground(SATELLITE);

    app.on_key(key(KeyCode::Char('Z')));
    let steps = doppler(&app.tick(start));
    assert_eq!(steps, [RigCommand::SatMode(false)]);

    // `m` is the shared mode key, not the transmitter filter.
    let commands = app.on_key(key(KeyCode::Char('m')));
    assert!(
        matches!(commands.as_slice(), [Command::SetMode { .. }]),
        "{commands:?}"
    );
}

#[test]
fn the_activity_keys_clash_with_no_reserved_key() {
    assert_eq!(app("ft847").check_activity_keys(), Ok(()));
}

mod contact {
    use super::*;

    /// Track the ISS FM transmitter and feed the rig's read-back of what was sent: receive
    /// frequency, mode, split on and the transmit frequency.
    fn tracked_and_read_back(start: Instant) -> (App, u64, u64) {
        let mut app = connected(app("ft847"));
        app.set_foreground(SATELLITE);
        app.on_key(key(KeyCode::Char('t')));
        let steps = doppler(&app.tick(start));
        let find = |want: fn(&RigCommand) -> Option<u64>| steps.iter().find_map(want).unwrap();
        let rx = find(|s| match s {
            RigCommand::RxFreq(hz) => Some(*hz),
            _ => None,
        });
        let tx = find(|s| match s {
            RigCommand::TxFreq(hz) => Some(*hz),
            _ => None,
        });
        app.on_rig(RigEvent::Freq(rx));
        app.on_rig(RigEvent::Mode {
            mode: RigMode::Fm,
            passband: 0,
        });
        app.on_rig(RigEvent::Split {
            enabled: true,
            tx_vfo: "VFOB".to_string(),
        });
        app.on_rig(RigEvent::SplitFreq(tx));
        (app, rx, tx)
    }

    fn adif_field(adif: &str, name: &str) -> Option<String> {
        let start = adif.find(&format!("<{name}:"))?;
        let rest = &adif[start..];
        let value = &rest[rest.find('>')? + 1..];
        Some(
            value[..value.find('<').unwrap_or(value.len())]
                .trim()
                .to_string(),
        )
    }

    #[test]
    fn a_satellite_contact_is_prefilled_through_the_shared_form() {
        let (mut app, rx, tx) = tracked_and_read_back(Instant::now());
        let path = app.log_path().unwrap().to_path_buf();

        app.on_key(key(KeyCode::Char('l')));
        let rows = screen(&app, 100, 30);
        assert!(has(&rows, "Log contact"), "{rows:#?}");
        type_str(&mut app, "k1abc");
        app.on_key(key(KeyCode::Enter));

        assert_eq!(app.logged_count(), 1, "{:?}", app.status());
        let adif = std::fs::read_to_string(path).unwrap();
        assert_eq!(adif_field(&adif, "CALL").as_deref(), Some("K1ABC"));
        assert_eq!(
            adif_field(&adif, "SAT_NAME").as_deref(),
            Some("ISS (ZARYA)")
        );
        assert_eq!(adif_field(&adif, "PROP_MODE").as_deref(), Some("SAT"));
        assert_eq!(adif_field(&adif, "MODE").as_deref(), Some("FM"));
        // FREQ is the uplink (the transmit frequency under split), FREQ_RX the downlink.
        let mhz = |hz: u64| format!("{}.{:06}", hz / 1_000_000, hz % 1_000_000);
        assert_eq!(adif_field(&adif, "FREQ").as_deref(), Some(mhz(tx).as_str()));
        assert_eq!(
            adif_field(&adif, "FREQ_RX").as_deref(),
            Some(mhz(rx).as_str())
        );
    }

    #[test]
    fn the_name_map_in_the_config_sets_sat_name() {
        let start = Instant::now();
        let mut settings = SatelliteSettings::default();
        settings
            .names
            .insert("ISS (ZARYA)".to_string(), "ISS".to_string());
        let data = SatelliteData {
            sats: parse_tles(include_str!("fixtures/sample.tle")).unwrap(),
            transmitters: parse_transmitters(include_str!("fixtures/satnogs_transmitters.json"))
                .unwrap(),
            ..SatelliteData::default()
        };
        let observer = Observer {
            lat_deg: 41.67,
            lon_deg: -85.71,
            alt_m: 250.0,
        };
        let mut app = App::with_callsign(Some("W1AW".to_string()))
            .with_log_path(Some(temp_log()))
            .with_rigs(&[profile("ft847", RigKind::Ft847)], None);
        app.host_activity(Box::new(Satellite::new(
            data,
            observer,
            settings,
            t(12, 0, 0),
        )));
        app.set_utc(SystemTime::from(t(13, 15, 0)));
        app.on_rig(RigEvent::Connected);
        app.set_foreground(SATELLITE);
        app.on_key(key(KeyCode::Char('t')));
        let steps = doppler(&app.tick(start));
        let (rx, tx) = (
            steps.iter().find_map(|s| match s {
                RigCommand::RxFreq(hz) => Some(*hz),
                _ => None,
            }),
            steps.iter().find_map(|s| match s {
                RigCommand::TxFreq(hz) => Some(*hz),
                _ => None,
            }),
        );
        app.on_rig(RigEvent::Freq(rx.unwrap()));
        app.on_rig(RigEvent::Mode {
            mode: RigMode::Fm,
            passband: 0,
        });
        app.on_rig(RigEvent::Split {
            enabled: true,
            tx_vfo: "VFOB".to_string(),
        });
        app.on_rig(RigEvent::SplitFreq(tx.unwrap()));
        let path = app.log_path().unwrap().to_path_buf();

        app.on_key(key(KeyCode::Char('l')));
        type_str(&mut app, "k1abc");
        app.on_key(key(KeyCode::Enter));

        let adif = std::fs::read_to_string(path).unwrap();
        assert_eq!(adif_field(&adif, "SAT_NAME").as_deref(), Some("ISS"));
    }

    #[test]
    fn it_refuses_without_tracking_with_the_reason() {
        let mut app = connected(app("ft847"));
        app.set_foreground(SATELLITE);

        app.on_key(key(KeyCode::Char('l')));

        assert!(
            app.status().is_some_and(|s| s.contains("start tracking")),
            "{:?}",
            app.status()
        );
        let rows = screen(&app, 100, 30);
        assert!(!has(&rows, "Log contact"), "{rows:#?}");
    }

    #[test]
    fn the_shared_freshness_refusal_holds_when_the_rig_has_gone_quiet() {
        let start = Instant::now();
        let (mut app, ..) = tracked_and_read_back(start);

        // The readings are older than two seconds.
        app.tick(start + Duration::from_secs(5));
        app.on_key(key(KeyCode::Char('l')));

        assert!(
            app.status().is_some_and(|s| s.contains("2 s")),
            "{:?}",
            app.status()
        );
        assert!(!has(&screen(&app, 100, 30), "Log contact"));
    }

    #[test]
    fn under_split_the_form_logs_the_transmit_frequency_not_the_receive_frequency() {
        let (mut app, rx, tx) = tracked_and_read_back(Instant::now());
        assert_ne!(rx, tx);
        // With the transmit frequency unknown the form refuses rather than log the wrong place.
        app.on_rig(RigEvent::Split {
            enabled: true,
            tx_vfo: "VFOB".to_string(),
        });
        let mut quiet = connected(self::app("ft847"));
        quiet.set_foreground(SATELLITE);
        quiet.on_key(key(KeyCode::Char('t')));
        quiet.tick(Instant::now());
        quiet.on_rig(RigEvent::Freq(rx));
        quiet.on_rig(RigEvent::Mode {
            mode: RigMode::Fm,
            passband: 0,
        });
        quiet.on_rig(RigEvent::Split {
            enabled: true,
            tx_vfo: "VFOB".to_string(),
        });

        quiet.on_key(key(KeyCode::Char('l')));

        assert!(
            quiet
                .status()
                .is_some_and(|s| s.contains("transmit frequency not known")),
            "{:?}",
            quiet.status()
        );
    }
}
