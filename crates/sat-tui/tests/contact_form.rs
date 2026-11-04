//! Key-event tests for logging a Contact. Passes are Skyfield's (see tests/app.rs): the ISS pass
//! of 13:14:10 to about 13:20 is under way at 13:15, with the FM voice repeater on 145.825 MHz.

use std::path::PathBuf;

use chrono::{DateTime, TimeZone, Utc};
use logbook::{Callsign, Log};
use orbit::{Observer, parse_tles};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sat_tui::app::{App, Command, Logging, Settings};
use sat_tui::satnogs::parse_transmitters;
use sat_tui::tracker::RigCommand;

fn at(h: u32, m: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, h, m, s).unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sat-tui-form-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn bare_app() -> App {
    App::new(
        parse_tles(include_str!("fixtures/sample.tle")).unwrap(),
        parse_transmitters(include_str!("fixtures/satnogs_transmitters.json")).unwrap(),
        Observer {
            lat_deg: 41.67,
            lon_deg: -85.71,
            alt_m: 250.0,
        },
        Settings {
            watch: vec![],
            horizon_hours: 24,
            min_elevation_deg: 10.0,
            short_pass_secs: 180,
        },
        at(12, 0, 0),
    )
}

fn logging(path: &std::path::Path) -> Logging {
    Logging::new(Log::new(path, Callsign::new("W1AW").unwrap()))
}

fn press(app: &mut App, code: KeyCode) -> Option<Command> {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

/// Tracking the ISS repeater with a live rig, one tick in at 13:15:00: ready to log.
fn tracking_app(log: &std::path::Path) -> (App, Vec<RigCommand>) {
    let mut app = bare_app();
    app.set_logging(Some(logging(log)));
    press(&mut app, KeyCode::Char('t'));
    app.set_rig_live(true);
    app.set_clock(at(13, 15, 0));
    let sent = app.tick(at(13, 15, 0)).unwrap();
    (app, sent)
}

fn tx_hz(commands: &[RigCommand]) -> u64 {
    commands
        .iter()
        .find_map(|c| match c {
            RigCommand::TxFreq(hz) => Some(*hz),
            _ => None,
        })
        .unwrap()
}

fn rx_hz(commands: &[RigCommand]) -> u64 {
    commands
        .iter()
        .find_map(|c| match c {
            RigCommand::RxFreq(hz) => Some(*hz),
            _ => None,
        })
        .unwrap()
}

mod opening {
    use super::*;

    #[test]
    fn l_opens_the_form_while_tracking_with_a_live_rig_and_sent_frequencies() {
        let dir = scratch("opens");
        let (mut app, _) = tracking_app(&dir.join("log.adi"));

        press(&mut app, KeyCode::Char('l'));

        let form = app.form().expect("form open");
        assert_eq!(form.satellite, "ISS (ZARYA)");
        assert_eq!(form.started, at(13, 15, 0));
        assert_eq!(form.rst_sent, "59");
        assert_eq!(form.rst_rcvd, "59");
    }

    #[test]
    fn not_tracking_it_says_why() {
        let dir = scratch("not-tracking");
        let mut app = bare_app();
        app.set_logging(Some(logging(&dir.join("log.adi"))));
        app.set_rig_live(true);

        press(&mut app, KeyCode::Char('l'));

        assert!(app.form().is_none());
        assert!(
            app.status().unwrap().contains("tracking"),
            "{:?}",
            app.status()
        );
    }

    #[test]
    fn without_a_live_rig_it_says_why() {
        let dir = scratch("no-rig");
        let (mut app, _) = tracking_app(&dir.join("log.adi"));
        app.set_rig_live(false);

        press(&mut app, KeyCode::Char('l'));

        assert!(app.form().is_none());
        assert!(app.status().unwrap().contains("rig"), "{:?}", app.status());
    }

    #[test]
    fn before_any_frequency_has_been_sent_it_says_why() {
        let dir = scratch("no-freq");
        let mut app = bare_app();
        app.set_logging(Some(logging(&dir.join("log.adi"))));
        press(&mut app, KeyCode::Char('t'));
        app.set_rig_live(true);

        press(&mut app, KeyCode::Char('l'));

        assert!(app.form().is_none());
        assert!(
            app.status().unwrap().contains("frequenc"),
            "{:?}",
            app.status()
        );
    }

    #[test]
    fn without_a_callsign_logging_is_off_and_the_status_names_the_config_key() {
        let mut app = bare_app();
        app.set_logging(None);
        press(&mut app, KeyCode::Char('t'));
        app.set_rig_live(true);
        app.tick(at(13, 15, 0)).unwrap();

        press(&mut app, KeyCode::Char('l'));

        assert!(app.form().is_none());
        assert!(
            app.status().unwrap().contains("callsign"),
            "{:?}",
            app.status()
        );
    }

    #[test]
    fn a_damaged_log_refuses_to_open_the_form_and_the_reason_is_shown() {
        let dir = scratch("damaged");
        let path = dir.join("log.adi");
        std::fs::write(&path, "<EOH>\n<CALL:4>W1AW<BAND:2>2m").unwrap();
        let (mut app, _) = tracking_app(&path);

        press(&mut app, KeyCode::Char('l'));

        assert!(app.form().is_none());
        assert!(
            app.status().unwrap().contains("partial record"),
            "{:?}",
            app.status()
        );
    }
}

mod editing {
    use super::*;

    fn open(name: &str) -> (App, PathBuf, Vec<RigCommand>) {
        let path = scratch(name).join("log.adi");
        let (mut app, sent) = tracking_app(&path);
        press(&mut app, KeyCode::Char('l'));
        assert!(app.form().is_some());
        (app, path, sent)
    }

    #[test]
    fn typing_goes_to_the_focused_field_and_call_and_grid_are_uppercased() {
        let (mut app, _, _) = open("typing");

        type_text(&mut app, "k9xyz");
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "en52");
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "Pat");
        press(&mut app, KeyCode::Backspace);

        let form = app.form().unwrap();
        assert_eq!(form.call, "K9XYZ");
        assert_eq!(form.grid, "EN52");
        assert_eq!(form.name, "Pa");
    }

    #[test]
    fn non_ascii_in_an_uppercased_field_is_rejected_naming_field_and_character() {
        let (mut app, _, _) = open("non-ascii");
        type_text(&mut app, "K9");

        // Uppercasing \u{df} would give "SS", which is ASCII and would slip through.
        type_text(&mut app, "\u{df}");

        let form = app.form().unwrap();
        assert_eq!(form.call, "K9");
        let error = form.error.as_deref().unwrap();
        assert!(
            error.contains("Call") && error.contains('\u{df}'),
            "{error}"
        );

        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "\u{e9}");
        let form = app.form().unwrap();
        assert_eq!(form.grid, "");
        assert!(form.error.as_deref().unwrap().contains("Grid"));
    }

    #[test]
    fn tab_and_shift_tab_cycle_through_every_field() {
        let (mut app, _, _) = open("cycle");
        let mut seen = vec![app.form().unwrap().field];

        for _ in 0..6 {
            press(&mut app, KeyCode::Tab);
            seen.push(app.form().unwrap().field);
        }
        assert_eq!(seen.first(), seen.last());
        seen.pop();
        seen.dedup();
        assert_eq!(seen.len(), 6);

        press(&mut app, KeyCode::BackTab);
        assert_ne!(app.form().unwrap().field, seen[0]);
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.form().unwrap().field, seen[0]);
    }

    #[test]
    fn the_form_captures_every_key_so_q_is_typed_not_quit() {
        let (mut app, _, _) = open("capture");

        let command = press(&mut app, KeyCode::Char('q'));

        assert_eq!(command, None);
        assert_eq!(app.form().unwrap().call, "Q");
        for c in ['r', 't', 's', 'l', 'L'] {
            press(&mut app, KeyCode::Char(c));
        }
        assert!(app.tracking().is_some());
        assert_eq!(app.form().unwrap().call, "QRTSLL");
    }

    #[test]
    fn escape_cancels_and_writes_nothing() {
        let (mut app, path, _) = open("cancel");
        type_text(&mut app, "K9XYZ");

        let command = press(&mut app, KeyCode::Esc);

        assert_eq!(command, None);
        assert!(app.form().is_none());
        assert!(!path.exists());
        assert!(app.session().is_empty());
    }

    #[test]
    fn enter_with_no_call_stays_open_and_says_so() {
        let (mut app, path, _) = open("no-call");

        press(&mut app, KeyCode::Enter);

        let form = app.form().expect("still open");
        assert_eq!(form.error.as_deref(), Some("Call is required"));
        assert!(!path.exists());
    }

    #[test]
    fn a_contact_error_keeps_the_form_open_with_the_error() {
        let (mut app, path, _) = open("contact-error");
        type_text(&mut app, "K9XYZ");
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "Jos\u{e9}");

        press(&mut app, KeyCode::Enter);

        let form = app.form().expect("still open");
        let error = form.error.as_deref().unwrap();
        assert!(
            error.contains("NAME") && error.contains('\u{e9}'),
            "{error}"
        );
        assert_eq!(form.name, "Jos\u{e9}");
        assert!(!path.exists());
    }
}

mod saving {
    use super::*;

    fn adif_field(name: &str, value: &str) -> String {
        format!("<{name}:{}>{value}", value.len())
    }

    fn mhz(hz: u64) -> String {
        format!("{}.{:06}", hz / 1_000_000, hz % 1_000_000)
    }

    #[test]
    fn enter_appends_the_contact_with_uplink_as_freq_and_downlink_as_freq_rx() {
        let path = scratch("saves").join("log.adi");
        let (mut app, sent) = tracking_app(&path);
        press(&mut app, KeyCode::Char('l'));
        type_text(&mut app, "k9xyz");
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "en52");
        app.set_clock(at(13, 17, 30));

        let command = press(&mut app, KeyCode::Enter);

        assert_eq!(command, None);
        assert!(app.form().is_none());
        assert_eq!(app.status(), Some("logged K9XYZ"));
        let adi = std::fs::read_to_string(&path).unwrap();
        for expected in [
            adif_field("CALL", "K9XYZ"),
            adif_field("GRIDSQUARE", "EN52"),
            adif_field("FREQ", &mhz(tx_hz(&sent))),
            adif_field("FREQ_RX", &mhz(rx_hz(&sent))),
            adif_field("BAND", "2m"),
            adif_field("BAND_RX", "2m"),
            adif_field("SAT_NAME", "ISS (ZARYA)"),
            adif_field("MODE", "FM"),
            adif_field("MY_GRIDSQUARE", "EN71DQ"),
            adif_field("TIME_ON", "131500"),
            adif_field("TIME_OFF", "131730"),
            adif_field("RST_SENT", "59"),
            adif_field("OPERATOR", "W1AW"),
        ] {
            assert!(adi.contains(&expected), "missing {expected} in\n{adi}");
        }
        assert_eq!(app.session().len(), 1);
        assert_eq!(app.session()[0].call, "K9XYZ");
    }

    #[test]
    fn the_grid_override_and_satellite_name_override_are_used() {
        let path = scratch("overrides").join("log.adi");
        let mut app = bare_app();
        let mut logging = logging(&path);
        logging.grid_override = Some("EN71DQ".to_string());
        logging
            .sat_names
            .insert("ISS (ZARYA)".to_string(), "ISS".to_string());
        app.set_logging(Some(logging));
        press(&mut app, KeyCode::Char('t'));
        app.set_rig_live(true);
        app.tick(at(13, 15, 0)).unwrap();
        press(&mut app, KeyCode::Char('l'));
        type_text(&mut app, "K9XYZ");

        press(&mut app, KeyCode::Enter);

        let adi = std::fs::read_to_string(&path).unwrap();
        assert!(
            adi.contains(&adif_field("MY_GRIDSQUARE", "EN71DQ")),
            "{adi}"
        );
        assert!(adi.contains(&adif_field("SAT_NAME", "ISS")), "{adi}");
    }

    #[test]
    fn the_snapshot_stays_fixed_while_tracking_and_doppler_carry_on() {
        let path = scratch("snapshot").join("log.adi");
        let (mut app, sent) = tracking_app(&path);
        press(&mut app, KeyCode::Char('l'));
        type_text(&mut app, "K9XYZ");

        let later = app.tick(at(13, 18, 0)).unwrap();
        app.set_clock(at(13, 18, 0));

        assert!(!later.is_empty(), "Doppler keeps moving the radio");
        assert!(app.tracking().is_some());
        let form = app.form().unwrap();
        assert_eq!(form.rx.hz, rx_hz(&sent));
        assert_eq!(form.tx.hz, tx_hz(&sent));
        assert_eq!(form.started, at(13, 15, 0));
        assert_eq!(app.radio_now_rx_hz(), Some(rx_hz(&later)));
        assert_ne!(form.rx.hz, rx_hz(&later));

        press(&mut app, KeyCode::Enter);
        let adi = std::fs::read_to_string(&path).unwrap();
        assert!(
            adi.contains(&adif_field("FREQ_RX", &mhz(rx_hz(&sent)))),
            "{adi}"
        );
        assert!(adi.contains(&adif_field("TIME_OFF", "131800")), "{adi}");
    }

    #[test]
    fn losing_the_rig_while_the_form_is_open_does_not_change_it() {
        let path = scratch("rig-lost").join("log.adi");
        let (mut app, sent) = tracking_app(&path);
        press(&mut app, KeyCode::Char('l'));
        app.set_rig_live(false);

        let form = app.form().unwrap();
        assert_eq!(form.rx.hz, rx_hz(&sent));
        press(&mut app, KeyCode::Enter);
        // Still no Call, so it stays open; the point is the snapshot survived.
        assert!(app.form().is_some());
    }

    #[test]
    fn a_refused_save_keeps_the_form_and_enter_retries_after_repair() {
        let path = scratch("retry").join("log.adi");
        let (mut app, _) = tracking_app(&path);
        press(&mut app, KeyCode::Char('l'));
        type_text(&mut app, "K9XYZ");
        // Someone else's program leaves a torn record after the form was opened.
        std::fs::write(&path, "<EOH>\n<CALL:4>W1").unwrap();

        press(&mut app, KeyCode::Enter);

        let form = app.form().expect("kept");
        assert_eq!(form.call, "K9XYZ");
        assert!(
            app.status().unwrap().contains("partial record"),
            "{:?}",
            app.status()
        );
        assert!(app.session().is_empty());

        std::fs::write(&path, "<EOH>\n").unwrap();
        press(&mut app, KeyCode::Enter);

        assert!(app.form().is_none());
        assert_eq!(app.status(), Some("logged K9XYZ"));
        assert!(std::fs::read_to_string(&path).unwrap().contains("K9XYZ"));
        assert_eq!(app.session().len(), 1);
    }
}

mod session_list {
    use super::*;

    fn log_one(app: &mut App, call: &str) {
        press(app, KeyCode::Char('l'));
        type_text(app, call);
        press(app, KeyCode::Enter);
    }

    #[test]
    fn capital_l_opens_and_closes_the_overlay_and_escape_closes_it() {
        let (mut app, _) = tracking_app(&scratch("overlay").join("log.adi"));

        press(&mut app, KeyCode::Char('L'));
        assert!(app.session_open());
        press(&mut app, KeyCode::Char('L'));
        assert!(!app.session_open());

        press(&mut app, KeyCode::Char('L'));
        assert_eq!(press(&mut app, KeyCode::Esc), None);
        assert!(!app.session_open());
    }

    #[test]
    fn q_still_quits_with_the_overlay_open() {
        let (mut app, _) = tracking_app(&scratch("overlay-q").join("log.adi"));
        press(&mut app, KeyCode::Char('L'));

        assert_eq!(press(&mut app, KeyCode::Char('q')), Some(Command::Quit));
    }

    #[test]
    fn escape_quits_when_nothing_is_open() {
        let (mut app, _) = tracking_app(&scratch("esc-quits").join("log.adi"));

        assert_eq!(press(&mut app, KeyCode::Esc), Some(Command::Quit));
    }

    #[test]
    fn contacts_accumulate_oldest_first_and_the_overlay_scrolls_back_and_forward() {
        let (mut app, _) = tracking_app(&scratch("accumulate").join("log.adi"));
        for call in ["AA1AA", "BB2BB", "CC3CC"] {
            log_one(&mut app, call);
        }
        let calls: Vec<_> = app.session().iter().map(|c| c.call.as_str()).collect();
        assert_eq!(calls, ["AA1AA", "BB2BB", "CC3CC"]);

        press(&mut app, KeyCode::Char('L'));
        assert_eq!(app.session_scroll(), 0);
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(app.session_scroll(), 2);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.session_scroll(), 2, "stops at the oldest");
        press(&mut app, KeyCode::Down);
        assert_eq!(app.session_scroll(), 1);
    }

    #[test]
    fn the_overlay_does_not_let_l_open_the_form() {
        let (mut app, _) = tracking_app(&scratch("no-form").join("log.adi"));
        press(&mut app, KeyCode::Char('L'));

        press(&mut app, KeyCode::Char('l'));

        assert!(app.form().is_none());
    }
}
