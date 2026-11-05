//! Logging a Contact through `App`: the `l` gate, the snapshot form, mode and RST mapping,
//! split, saving without a rig, and the session list. Contacts go to a temp-dir Log, never a real
//! one.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

use common::*;
use ratatui::crossterm::event::KeyCode;
use rig_tui::{App, RigEvent};

fn temp_log(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rig-tui-logging-{}-{}-{tag}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("log.adi")
}

fn utc(secs: u64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(86_400 * 19_000 + secs)
}

const T_START: u64 = 12 * 3600 + 34 * 60 + 56; // 12:34:56
const T_END: u64 = 12 * 3600 + 40 * 60; // 12:40:00

fn base_app(log: &Path, locator: Option<&str>) -> App {
    let mut app = App::with_callsign(Some("K9TST".to_string()))
        .with_log_path(Some(log.to_path_buf()))
        .with_locator(locator.map(String::from));
    app.set_utc(utc(T_START));
    app
}

/// Connected, 14.074 MHz, in `mode`, with a logging setup writing to `log`.
fn app_in(mode: &str, log: &Path) -> App {
    let mut app = connected_app_in(base_app(log, Some("EN71DQ")), mode);
    app.on_rig(RigEvent::Split {
        enabled: false,
        tx_vfo: "VFOA".to_string(),
    });
    app
}

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

fn open_form(app: &mut App) {
    app.on_key(key(KeyCode::Char('l')));
    assert!(
        has(&screen(app, 80, 24), "Log contact"),
        "form did not open: {:?}",
        app.status()
    );
}

fn tab(app: &mut App, times: usize) {
    for _ in 0..times {
        app.on_key(key(KeyCode::Tab));
    }
}

fn adif(log: &Path) -> String {
    std::fs::read_to_string(log).unwrap_or_default()
}

/// Open the form, type `call`, save.
fn log_call(app: &mut App, call: &str) {
    open_form(app);
    type_str(app, call);
    app.on_key(key(KeyCode::Enter));
}

mod gate {
    use super::*;

    fn refused(app: &mut App, why: &str) {
        app.on_key(key(KeyCode::Char('l')));
        let status = app.status().unwrap_or_default().to_string();
        assert!(status.contains(why), "status {status:?} lacks {why:?}");
        assert!(!has(&screen(app, 80, 24), "Log contact"));
    }

    #[test]
    fn refuses_when_disconnected() {
        let mut app = base_app(&temp_log("disc"), None);
        refused(&mut app, "not connected");
    }

    #[test]
    fn refuses_when_frequency_and_mode_are_stale() {
        let mut app = app_in("USB", &temp_log("stale"));
        app.tick(Instant::now() + Duration::from_secs(3));
        refused(&mut app, "frequency and mode");
    }

    #[test]
    fn refuses_outside_the_band_table() {
        let mut app = app_in("USB", &temp_log("band"));
        app.on_rig(RigEvent::Freq(13_000_000));
        refused(&mut app, "outside");
    }

    #[test]
    fn refuses_without_a_callsign_naming_the_key() {
        let log = temp_log("nocall");
        let mut app = connected_app_in(App::new().with_log_path(Some(log.to_path_buf())), "USB");
        app.on_rig(RigEvent::Split {
            enabled: false,
            tx_vfo: "VFOA".to_string(),
        });
        refused(&mut app, "callsign");
    }

    #[test]
    fn refuses_when_the_log_is_unusable() {
        let log = temp_log("junk");
        std::fs::write(&log, "this is not an ADIF file").unwrap();
        let mut app = app_in("USB", &log);
        refused(&mut app, "log");
        assert_eq!(adif(&log), "this is not an ADIF file");
    }

    #[test]
    fn opens_from_the_strip_and_with_ctrl_l_from_the_input_line() {
        let log = temp_log("open");
        let mut app = app_in("USB", &log);
        open_form(&mut app);

        let mut app = cw_app_with_logging(&log);
        app.on_key(ctrl('l'));
        assert!(has(&screen(&app, 80, 24), "Log contact"));
    }

    #[test]
    fn chords_stay_blocked_over_the_log_form() {
        let log = temp_log("blocked");
        let mut app = app_in("USB", &log);
        open_form(&mut app);
        for c in ['o', 'r', 'g', 'k', 'j'] {
            assert_eq!(app.on_key(ctrl(c)), vec![]);
            let rows = screen(&app, 80, 24);
            assert!(has(&rows, "Log contact"), "Ctrl-{c} closed the form");
            assert!(!has(&rows, "Config") && !has(&rows, "Rigs"), "Ctrl-{c}");
        }
    }

    fn cw_app_with_logging(log: &Path) -> App {
        focus_input(app_in("CW", log))
    }

    #[test]
    fn ctrl_l_works_while_typing_in_the_cw_input() {
        let log = temp_log("ctrl");
        let mut app = cw_app_with_logging(&log);
        type_str(&mut app, "l");
        app.on_key(ctrl('l'));
        assert!(has(&screen(&app, 80, 24), "Log contact"));
    }
}

mod form {
    use super::*;

    #[test]
    fn saves_a_contact_with_station_fields_and_the_end_time() {
        let log = temp_log("save");
        let mut app = app_in("USB", &log);
        open_form(&mut app);
        type_str(&mut app, "w1aw");
        app.set_utc(utc(T_END));
        app.on_key(key(KeyCode::Enter));

        let text = adif(&log);
        assert!(text.contains("<CALL:4>W1AW"), "{text}");
        assert!(text.contains("<STATION_CALLSIGN:5>K9TST"), "{text}");
        assert!(text.contains("<OPERATOR:5>K9TST"), "{text}");
        assert!(text.contains("<MY_GRIDSQUARE:6>EN71DQ"), "{text}");
        assert!(text.contains("<FREQ:9>14.074000"), "{text}");
        assert!(text.contains("<TIME_ON:6>123456"), "{text}");
        assert!(text.contains("<TIME_OFF:6>124000"), "{text}");
        assert_eq!(app.status(), Some("logged W1AW"));
        assert!(!has(&screen(&app, 80, 24), "Log contact"));
    }

    #[test]
    fn is_a_snapshot_while_the_strip_keeps_updating() {
        let log = temp_log("snap");
        let mut app = app_in("USB", &log);
        open_form(&mut app);
        app.on_rig(RigEvent::Freq(7_030_000));
        app.on_rig(mode_event("CW"));

        let rows = screen(&app, 80, 24);
        assert!(
            has(&rows, "7.030.000"),
            "strip should follow the rig: {rows:#?}"
        );

        type_str(&mut app, "w1aw");
        app.on_key(key(KeyCode::Enter));
        let text = adif(&log);
        assert!(text.contains("<FREQ:9>14.074000"), "{text}");
        assert!(text.contains("<SUBMODE:3>USB"), "{text}");
    }

    #[test]
    fn call_is_required_and_esc_cancels() {
        let log = temp_log("req");
        let mut app = app_in("USB", &log);
        open_form(&mut app);
        app.on_key(key(KeyCode::Enter));

        assert!(has(&screen(&app, 80, 24), "Call is required"));
        assert_eq!(adif(&log), "");

        app.on_key(key(KeyCode::Esc));
        assert!(!has(&screen(&app, 80, 24), "Log contact"));
        assert_eq!(adif(&log), "");
    }

    #[test]
    fn a_non_ascii_comment_names_the_field_and_keeps_the_form() {
        let log = temp_log("ascii");
        let mut app = app_in("USB", &log);
        open_form(&mut app);
        type_str(&mut app, "w1aw");
        tab(&mut app, 7); // Comment
        app.on_key(key(KeyCode::Char('\u{e9}')));
        app.on_key(key(KeyCode::Enter));

        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "COMMENT"), "{rows:#?}");
        assert!(has(&rows, "Log contact"));
        assert_eq!(adif(&log), "");
    }

    #[test]
    fn saves_even_when_the_rig_is_lost_while_the_form_is_open() {
        let log = temp_log("lost");
        let mut app = app_in("CW", &log);
        open_form(&mut app);
        type_str(&mut app, "dl1abc");
        app.on_rig(RigEvent::Disconnected("rigctld gone".to_string()));
        app.on_key(key(KeyCode::Enter));

        let text = adif(&log);
        assert!(text.contains("<CALL:6>DL1ABC"), "{text}");
        assert!(text.contains("<FREQ:9>14.074000"), "{text}");
    }

    #[test]
    fn no_locator_is_in_the_title_and_the_field_is_omitted_but_it_still_opens() {
        let log = temp_log("noloc");
        let mut app = connected_app_in(base_app(&log, None), "USB");
        app.on_rig(RigEvent::Split {
            enabled: false,
            tx_vfo: "VFOA".to_string(),
        });
        open_form(&mut app);
        assert!(has(&screen(&app, 80, 24), "no locator"));
        type_str(&mut app, "w1aw");
        app.on_key(key(KeyCode::Enter));
        assert!(!adif(&log).contains("MY_GRIDSQUARE"));
    }
}

mod modes {
    use super::*;

    fn logged(mode: &str) -> String {
        let log = temp_log("mode");
        let mut app = app_in(mode, &log);
        log_call(&mut app, "w1aw");
        adif(&log)
    }

    #[test]
    fn maps_rig_modes_to_adif_with_default_rst() {
        let cases = [
            (
                "USB",
                "<MODE:3>SSB",
                Some("<SUBMODE:3>USB"),
                "<RST_SENT:2>59",
            ),
            (
                "LSB",
                "<MODE:3>SSB",
                Some("<SUBMODE:3>LSB"),
                "<RST_SENT:2>59",
            ),
            ("CW", "<MODE:2>CW", None, "<RST_SENT:3>599"),
            ("CWR", "<MODE:2>CW", None, "<RST_RCVD:3>599"),
            ("AM", "<MODE:2>AM", None, "<RST_RCVD:2>59"),
            ("FM", "<MODE:2>FM", None, "<RST_SENT:2>59"),
            ("RTTY", "<MODE:4>RTTY", None, "<RST_SENT:3>599"),
            ("RTTYR", "<MODE:4>RTTY", None, "<RST_RCVD:3>599"),
        ];
        for (rig, mode, submode, rst) in cases {
            let text = logged(rig);
            assert!(text.contains(mode), "{rig}: {text}");
            assert!(text.contains(rst), "{rig}: {text}");
            match submode {
                Some(s) => assert!(text.contains(s), "{rig}: {text}"),
                None => assert!(!text.contains("<SUBMODE"), "{rig}: {text}"),
            }
        }
    }

    #[test]
    fn a_data_mode_needs_a_typed_mode_and_has_blank_rst() {
        let log = temp_log("data");
        let mut app = app_in("PKTUSB", &log);
        open_form(&mut app);
        type_str(&mut app, "w1aw");
        app.on_key(key(KeyCode::Enter));

        assert!(has(&screen(&app, 80, 24), "Mode is required"));
        assert_eq!(adif(&log), "");

        tab(&mut app, 1); // Mode
        type_str(&mut app, "mfsk");
        tab(&mut app, 1); // Submode
        type_str(&mut app, "ft4");
        app.on_key(key(KeyCode::Enter));

        let text = adif(&log);
        assert!(text.contains("<MODE:4>MFSK"), "{text}");
        assert!(text.contains("<SUBMODE:3>FT4"), "{text}");
        assert!(!text.contains("RST_SENT"), "{text}");
        assert!(!text.contains("RST_RCVD"), "{text}");
    }

    #[test]
    fn a_clear_mode_is_prefilled_but_can_be_corrected() {
        let log = temp_log("edit");
        let mut app = app_in("USB", &log);
        open_form(&mut app);
        type_str(&mut app, "w1aw");
        tab(&mut app, 1); // Mode
        for _ in 0..3 {
            app.on_key(key(KeyCode::Backspace));
        }
        type_str(&mut app, "am");
        tab(&mut app, 1); // Submode
        for _ in 0..3 {
            app.on_key(key(KeyCode::Backspace));
        }
        app.on_key(key(KeyCode::Enter));

        let text = adif(&log);
        assert!(text.contains("<MODE:2>AM"), "{text}");
        assert!(!text.contains("<SUBMODE"), "{text}");
    }
}

mod split {
    use super::*;

    fn split_app(log: &Path) -> App {
        let mut app = app_in("USB", log);
        app.on_rig(RigEvent::Vfo("VFOA".to_string()));
        app.on_rig(RigEvent::Split {
            enabled: true,
            tx_vfo: "VFOB".to_string(),
        });
        app
    }

    #[test]
    fn freq_is_the_transmit_frequency() {
        let log = temp_log("split");
        let mut app = split_app(&log);
        app.on_rig(RigEvent::SplitFreq(7_074_000));
        log_call(&mut app, "w1aw");

        let text = adif(&log);
        assert!(text.contains("<FREQ:8>7.074000"), "{text}");
        assert!(text.contains("<BAND:3>40m"), "{text}");
    }

    #[test]
    fn refuses_when_split_is_on_but_no_transmit_frequency_was_reported() {
        let log = temp_log("splitnone");
        let mut app = split_app(&log);
        app.on_key(key(KeyCode::Char('l')));

        assert!(
            app.status()
                .unwrap_or_default()
                .contains("transmit frequency")
        );
        assert!(!has(&screen(&app, 80, 24), "Log contact"));
    }
}

mod session {
    use super::*;

    #[test]
    fn counts_in_the_title_and_lists_with_l_upper() {
        let log = temp_log("session");
        let mut app = app_in("USB", &log);
        assert!(has(&screen(&app, 80, 24), "0 logged"));

        log_call(&mut app, "w1aw");
        log_call(&mut app, "dl1abc");

        assert!(has(&screen(&app, 80, 24), "2 logged"));
        app.on_key(key(KeyCode::Char('L')));
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "W1AW"), "{rows:#?}");
        assert!(has(&rows, "DL1ABC"), "{rows:#?}");

        // The list is a modal: it takes the keys, and Esc closes it.
        app.on_key(key(KeyCode::Char('f')));
        assert!(!has(&screen(&app, 80, 24), "freq>"));
        app.on_key(key(KeyCode::Esc));
        assert!(!has(&screen(&app, 80, 24), "Session:"));
    }

    #[test]
    fn the_list_opens_while_disconnected() {
        let mut app = base_app(&temp_log("listdisc"), None);
        app.on_key(key(KeyCode::Char('L')));
        assert!(app.has_overlay());
    }

    #[test]
    fn recent_contacts_fill_the_body_when_no_panel_applies() {
        let log = temp_log("recent");
        let mut app = app_in("USB", &log);
        log_call(&mut app, "w1aw");

        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "W1AW"), "{rows:#?}");
    }

    #[test]
    fn a_failed_save_keeps_the_form_and_its_contents() {
        let log = temp_log("fail");
        let mut app = app_in("USB", &log);
        open_form(&mut app);
        type_str(&mut app, "w1aw");
        // Spoil the Log after the gate passed: a file that is not ADIF.
        std::fs::write(&log, "junk").unwrap();
        app.on_key(key(KeyCode::Enter));

        assert!(app.status().unwrap_or_default().contains("not logged"));
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "Log contact"));
        assert!(has(&rows, "W1AW"));
        assert!(has(&screen(&app, 80, 24), "0 logged"));
    }
}
