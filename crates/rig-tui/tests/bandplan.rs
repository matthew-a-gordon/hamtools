//! The Ctrl-B band plan modal, driven through the App: keys in, commands out, buffer read back.

mod common;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::*;
use hamtools_config::LicenseClass;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use rig_tui::{
    Activity, ActivityCtx, ActivityOutput, App, Command, LogDefaults, LogMode, RigEvent, RigMode,
};

/// 2026-11-05 00:00 UTC, the day after the data's `as_of`.
const FRESH: u64 = 1_793_836_800;
/// 2028-01-01 00:00 UTC: more than a year after `as_of`.
const STALE: u64 = 1_830_297_600;

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

fn row_with<'a>(rows: &'a [String], needle: &str) -> &'a str {
    rows.iter()
        .find(|r| r.contains(needle))
        .unwrap_or_else(|| panic!("no row with {needle:?}: {rows:#?}"))
}

/// Connected on 14.074 MHz in CW, class General, with the plan open.
fn open_app() -> App {
    let mut app = connected_app_in(
        App::new().with_license_class(Some(LicenseClass::General)),
        "CW",
    );
    app.set_utc(at(FRESH));
    assert_eq!(app.on_key(ctrl('b')), vec![]);
    app
}

#[test]
fn ctrl_b_opens_on_the_band_of_the_rig_frequency() {
    let app = open_app();

    let rows = screen(&app, 80, 24);

    assert!(has(&rows, "20m"), "{rows:#?}");
    assert!(has(&rows, "14.000.000"), "{rows:#?}");
    assert!(has(&rows, "14.350.000"), "{rows:#?}");
}

#[test]
fn ctrl_b_opens_from_a_text_area_without_typing_into_it() {
    let mut app = cw_app();

    assert_eq!(app.on_key(ctrl('b')), vec![]);

    let rows = screen(&app, 80, 24);
    assert!(has(&rows, "14.350.000"), "{rows:#?}");
    assert!(!has(&rows, "> B"), "{rows:#?}");
}

#[test]
fn outside_every_band_it_opens_on_the_nearest_band_below() {
    let mut app = connected_app_in(App::new(), "CW");
    app.on_rig(RigEvent::Freq(13_000_000));
    app.on_key(ctrl('b'));

    // Between 30 m (10.1 MHz) and 20 m: the band below is 30 m.
    assert!(has(&screen(&app, 80, 24), "30m"));
}

#[test]
fn right_and_left_step_bands_and_so_do_the_brackets() {
    let mut app = open_app();

    app.on_key(key(KeyCode::Right));
    assert!(has(&screen(&app, 80, 24), "17m"));
    app.on_key(key(KeyCode::Char(']')));
    assert!(has(&screen(&app, 80, 24), "15m"));
    app.on_key(key(KeyCode::Left));
    app.on_key(key(KeyCode::Char('[')));
    assert!(has(&screen(&app, 80, 24), "20m"));
}

#[test]
fn the_receive_frequency_is_marked() {
    let app = open_app();

    let rows = screen(&app, 80, 24);

    assert!(row_with(&rows, "Rig").contains('R'), "{rows:#?}");
}

#[test]
fn with_split_on_the_transmit_frequency_gets_its_own_marker() {
    let mut app = open_app();
    app.on_rig(RigEvent::Split {
        enabled: true,
        tx_vfo: "VFOB".to_string(),
    });
    app.on_rig(RigEvent::SplitFreq(14_080_000));

    let rows = screen(&app, 80, 24);

    let marks = row_with(&rows, "Rig");
    assert!(marks.contains('R') && marks.contains('T'), "{rows:#?}");
}

#[test]
fn enter_sets_the_receive_frequency_only_and_the_modal_stays_open() {
    let mut app = open_app();

    app.on_key(key(KeyCode::Up));
    let commands = app.on_key(key(KeyCode::Enter));

    // 20 m steps 5 kHz.
    assert_eq!(commands, vec![Command::SetFreq(14_079_000)]);
    assert!(has(&screen(&app, 80, 24), "14.000.000"));
}

#[test]
fn down_moves_the_cursor_below_the_start() {
    let mut app = open_app();

    app.on_key(key(KeyCode::Down));

    assert_eq!(
        app.on_key(key(KeyCode::Enter)),
        // 5 kHz down from 14.074 would be 14.069, but the RTTY segment starts at 14.070.
        vec![Command::SetFreq(14_070_000)]
    );
}

#[test]
fn the_cursor_lands_on_a_segment_start_it_would_step_over() {
    let mut app = connected_app_in(App::new(), "CW");
    app.on_rig(RigEvent::Freq(14_068_000));
    app.on_key(ctrl('b'));

    // One 5 kHz step from 14.068 would pass the RTTY segment start at 14.070.
    app.on_key(key(KeyCode::Up));

    assert_eq!(
        app.on_key(key(KeyCode::Enter)),
        vec![Command::SetFreq(14_070_000)]
    );
}

#[test]
fn m_applies_the_segments_usual_mode_with_passband_zero() {
    let mut app = open_app();

    // 14.074 is inside the ARRL RTTY segment.
    let commands = app.on_key(key(KeyCode::Char('m')));

    assert_eq!(
        commands,
        vec![Command::SetMode {
            mode: RigMode::Rtty,
            passband: 0
        }]
    );
}

#[test]
fn m_in_the_mode_the_rig_already_has_sends_nothing_so_the_passband_stays() {
    let mut app = connected_app_in(
        App::new().with_license_class(Some(LicenseClass::General)),
        "RTTY",
    );
    app.on_key(ctrl('b'));

    assert_eq!(app.on_key(key(KeyCode::Char('m'))), vec![]);
}

#[test]
fn m_on_60m_chooses_upper_sideband() {
    let mut app = connected_app_in(App::new(), "CW");
    app.on_rig(RigEvent::Freq(5_357_000));
    app.on_key(ctrl('b'));

    assert_eq!(
        app.on_key(key(KeyCode::Char('m'))),
        vec![Command::SetMode {
            mode: RigMode::Usb,
            passband: 0
        }]
    );
}

#[test]
fn m_with_no_usual_mode_here_says_so_and_does_nothing() {
    // 14.100 MHz is the NCDXF beacon point, a row with no usual mode.
    let mut app = connected_app_in(
        App::new().with_license_class(Some(LicenseClass::General)),
        "CW",
    );
    app.on_rig(RigEvent::Freq(14_100_000));
    app.on_key(ctrl('b'));

    assert_eq!(app.on_key(key(KeyCode::Char('m'))), vec![]);
    assert!(app.status().unwrap().contains("no usual mode"));
}

#[test]
fn t_needs_split_on_and_then_sets_the_transmit_frequency_only() {
    let mut app = open_app();
    app.on_rig(RigEvent::Split {
        enabled: false,
        tx_vfo: "VFOA".to_string(),
    });
    assert_eq!(app.on_key(key(KeyCode::Char('T'))), vec![]);
    assert!(app.status().unwrap().contains("split"));

    app.on_rig(RigEvent::Split {
        enabled: true,
        tx_vfo: "VFOB".to_string(),
    });
    app.on_key(key(KeyCode::Up));

    assert_eq!(
        app.on_key(key(KeyCode::Char('T'))),
        vec![Command::SetSplitFreq(14_079_000)]
    );
}

#[test]
fn esc_closes_the_modal_and_the_strip_keys_work_again() {
    let mut app = open_app();

    assert_eq!(app.on_key(key(KeyCode::Esc)), vec![]);

    assert!(!has(&screen(&app, 80, 24), "14.350.000"));
    assert_eq!(
        app.on_key(key(KeyCode::Right)),
        vec![Command::SetFreq(14_075_000)]
    );
}

#[test]
fn ctrl_c_asks_to_quit_over_the_modal_and_cancelling_returns_to_it() {
    let mut app = open_app();

    assert_eq!(app.on_key(ctrl('c')), vec![]);
    assert_eq!(app.on_key(key(KeyCode::Esc)), vec![]);

    assert!(has(&screen(&app, 80, 24), "14.350.000"));
    assert_eq!(app.on_key(ctrl('c')), vec![]);
    assert_eq!(app.on_key(ctrl('c')), vec![Command::Quit]);
}

#[test]
fn nothing_the_modal_does_can_transmit() {
    let mut app = open_app();
    app.on_rig(RigEvent::Split {
        enabled: true,
        tx_vfo: "VFOB".to_string(),
    });
    for code in ['m', 'T', 'x'] {
        for command in app.on_key(key(KeyCode::Char(code))) {
            assert!(!command.transmits(), "{command:?}");
        }
    }
    for command in app.on_key(key(KeyCode::Enter)) {
        assert!(!command.transmits(), "{command:?}");
    }
}

#[test]
fn out_of_privilege_is_a_soft_warning_and_still_tunes() {
    let mut app = connected_app_in(
        App::new().with_license_class(Some(LicenseClass::Technician)),
        "CW",
    );
    app.on_key(ctrl('b'));

    let commands = app.on_key(key(KeyCode::Enter));

    assert_eq!(commands, vec![Command::SetFreq(14_074_000)]);
    assert!(
        app.status().unwrap().contains("privileges"),
        "{:?}",
        app.status()
    );
}

#[test]
fn in_privilege_tuning_has_no_warning() {
    let mut app = open_app();

    app.on_key(key(KeyCode::Enter));

    assert_eq!(app.status(), None);
}

#[test]
fn privilege_state_has_a_plus_or_minus_cue() {
    let app = open_app();

    let priv_row = screen(&app, 80, 24);
    let line = row_with(&priv_row, "Priv");

    // General on 20 m: out at 14.000-14.025, in from 14.025.
    assert!(line.contains('+') && line.contains('-'), "{line}");
}

#[test]
fn with_no_license_class_the_privilege_row_is_hidden_with_a_note() {
    let mut app = connected_app_in(App::new(), "CW");
    app.on_key(ctrl('b'));

    let rows = screen(&app, 80, 24);

    assert!(has(&rows, "license_class"), "{rows:#?}");
    assert!(!row_with(&rows, "Priv").contains('+'), "{rows:#?}");
}

#[test]
fn at_the_smallest_supported_screen_every_row_fits() {
    let app = open_app();

    let rows = screen(&app, 80, 24);

    for needle in ["Priv", "ARRL", "Sat ", "Rig", "Cur", "Enter rx", "as_of"] {
        assert!(has(&rows, needle), "{needle}: {rows:#?}");
    }
}

#[test]
fn the_footer_shows_as_of_with_a_stale_marker_only_when_old() {
    let mut app = open_app();
    let fresh = screen(&app, 80, 24);
    assert!(has(&fresh, "as_of 2026-11-04"), "{fresh:#?}");
    assert!(!has(&fresh, "stale"));

    app.set_utc(at(STALE));
    assert!(has(&screen(&app, 80, 24), "stale"));
}

mod sixty_metres {
    use super::*;

    fn on_sixty() -> App {
        let mut app = open_app();
        for _ in 0..3 {
            app.on_key(key(KeyCode::Left));
        }
        app
    }

    #[test]
    fn it_shows_the_channels_and_the_unverified_note() {
        let app = on_sixty();

        let rows = screen(&app, 80, 24);

        assert!(has(&rows, "60m"), "{rows:#?}");
        assert!(has(&rows, "Chan"), "{rows:#?}");
        assert!(has(&rows, "unverified"), "{rows:#?}");
    }

    #[test]
    fn the_cursor_starts_on_a_channel_and_snaps_between_channels() {
        let mut app = on_sixty();
        assert_eq!(
            app.on_key(key(KeyCode::Enter)),
            vec![Command::SetFreq(5_330_500)]
        );

        app.on_key(key(KeyCode::Up));
        assert_eq!(
            app.on_key(key(KeyCode::Enter)),
            vec![Command::SetFreq(5_346_500)]
        );
        app.on_key(key(KeyCode::Down));
        app.on_key(key(KeyCode::Down));
        assert_eq!(
            app.on_key(key(KeyCode::Enter)),
            vec![Command::SetFreq(5_330_500)]
        );
    }

    #[test]
    fn the_unverified_note_is_only_on_sixty() {
        let app = open_app();

        assert!(!has(&screen(&app, 80, 24), "unverified"));
    }
}

/// An Activity that is "tracking": it blocks tuning from the modal.
struct Tracking;

impl Activity for Tracking {
    fn name(&self) -> &'static str {
        "Tracking"
    }
    fn on_key(&mut self, _key: KeyEvent, _ctx: &ActivityCtx) -> Option<ActivityOutput> {
        None
    }
    fn on_tick(&mut self, _now: std::time::Instant, _ctx: &ActivityCtx) -> Vec<Command> {
        Vec::new()
    }
    fn on_reply(&mut self, _request: &str, _reply: Result<String, String>) -> Result<(), String> {
        Ok(())
    }
    fn on_mode(&mut self, _ctx: &ActivityCtx) {}
    fn text_focus(&self, _ctx: &ActivityCtx) -> bool {
        false
    }
    fn has_panel(&self, _ctx: &ActivityCtx) -> bool {
        false
    }
    fn log_hints(&self, _mode: &RigMode) -> (Option<LogMode>, LogDefaults) {
        (None, LogDefaults::default())
    }
    fn hint(&self, _ctx: &ActivityCtx) -> &'static str {
        ""
    }
    fn render(&self, _frame: &mut Frame, _area: Rect, _ctx: &ActivityCtx) {}
    fn tuning_blocked(&self) -> Option<&'static str> {
        Some("Doppler tracking")
    }
}

#[test]
fn tuning_from_the_modal_is_refused_while_doppler_tracking() {
    let mut app = App::new();
    let tracking = app.host_activity(Box::new(Tracking));
    app.set_foreground(tracking);
    let mut app = connected_app_in(app, "USB");
    app.on_rig(RigEvent::Split {
        enabled: true,
        tx_vfo: "VFOB".to_string(),
    });
    app.on_rig(RigEvent::SplitFreq(14_080_000));
    app.on_key(ctrl('b'));

    for code in ['m', 'T'] {
        assert_eq!(app.on_key(key(KeyCode::Char(code))), vec![]);
        assert!(app.status().unwrap().contains("Doppler tracking"));
    }
    app.set_status(None);
    assert_eq!(app.on_key(key(KeyCode::Enter)), vec![]);
    assert!(app.status().unwrap().contains("Doppler tracking"));
}

#[test]
fn the_modal_opens_over_the_general_activity_too() {
    // General is the default Activity and has no tuning block.
    let mut app = connected_app_in(App::new(), "CW");
    app.on_key(ctrl('b'));
    assert!(app.has_overlay());
}
