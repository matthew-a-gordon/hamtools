//! Window size through `App`: the "Window too small" notice, ignored keys, and General's
//! today's-contacts pane at the candidate sizes. Contacts go to a temp-dir Log.

mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use common::*;
use ratatui::crossterm::event::KeyCode;
use rig_tui::{App, Command, RigEvent};

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

fn temp_log() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rig-tui-size-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("log.adi")
}

/// Connected in USB (no panel), split off, one Contact logged: W1AW.
fn app_with_a_contact() -> App {
    let mut app = connected_app_in(
        App::with_callsign(Some("K9TST".to_string()))
            .with_log_path(Some(temp_log()))
            .with_locator(Some("EN71DQ".to_string())),
        "USB",
    );
    app.on_rig(RigEvent::Split {
        enabled: false,
        tx_vfo: "VFOA".to_string(),
    });
    app.on_key(key(KeyCode::Char('l')));
    type_str(&mut app, "w1aw");
    app.on_key(key(KeyCode::Enter));
    app.set_status(None);
    app
}

/// A panel mode, so the contacts are not already shown by the no-panel body.
fn cw_app_with_a_contact() -> App {
    let mut app = app_with_a_contact();
    app.on_rig(mode_event("CW"));
    app
}

fn pane_rows(rows: &[String]) -> Vec<&String> {
    rows.iter()
        .filter(|r| r.contains("Today's contacts"))
        .collect()
}

mod too_small {
    use super::*;

    #[test]
    fn shows_only_the_notice_with_needed_and_current_size() {
        let app = app_with_a_contact();

        let rows = screen(&app, 79, 30);

        assert!(
            has(&rows, "Window too small: needs 80x24, have 79x30"),
            "{rows:#?}"
        );
        assert!(
            !has(&rows, "logged"),
            "the strip should be hidden: {rows:#?}"
        );
    }

    #[test]
    fn short_windows_get_the_notice_too() {
        let app = app_with_a_contact();

        let rows = screen(&app, 120, 23);

        assert!(
            has(&rows, "Window too small: needs 80x24, have 120x23"),
            "{rows:#?}"
        );
    }

    #[test]
    fn recovers_by_itself_when_the_window_grows() {
        let app = app_with_a_contact();
        assert!(has(&screen(&app, 79, 24), "Window too small"));

        let rows = screen(&app, 80, 24);

        assert!(!has(&rows, "Window too small"), "{rows:#?}");
        assert!(has(&rows, "1 logged"), "{rows:#?}");
    }

    #[test]
    fn keys_are_ignored_except_the_quit_chord() {
        let mut app = app_with_a_contact();
        app.set_size(79, 24);
        assert!(app.too_small());

        for code in [KeyCode::Char('f'), KeyCode::Char('l'), KeyCode::Char('q')] {
            assert!(app.on_key(key(code)).is_empty());
        }
        app.on_key(ctrl('l'));
        assert!(!app.has_overlay());
        assert_eq!(app.status(), None);

        assert_eq!(app.on_key(ctrl('c')), vec![Command::Quit]);
    }

    #[test]
    fn keys_work_again_once_the_window_is_big_enough() {
        let mut app = app_with_a_contact();
        app.set_size(79, 24);
        app.on_key(key(KeyCode::Char('L')));
        assert!(!app.has_overlay());

        app.set_size(80, 24);
        app.on_key(key(KeyCode::Char('L')));

        assert!(app.has_overlay());
    }
}

mod todays_contacts {
    use super::*;

    #[test]
    fn absent_at_the_floor() {
        let app = cw_app_with_a_contact();

        assert!(pane_rows(&screen(&app, 80, 24)).is_empty());
        assert!(pane_rows(&screen(&app, 127, 39)).is_empty());
    }

    #[test]
    fn a_column_from_128_columns() {
        let app = cw_app_with_a_contact();

        let rows = screen(&app, 128, 24);

        assert_eq!(pane_rows(&rows).len(), 1, "{rows:#?}");
        assert!(has(&rows, "W1AW"), "{rows:#?}");
        // It is beside the panel (just under the strip), not below it.
        let title = rows.iter().position(|r| r.contains("Today's contacts"));
        assert_eq!(title, Some(4), "{rows:#?}");
    }

    #[test]
    fn stacked_from_40_rows() {
        let app = cw_app_with_a_contact();

        let rows = screen(&app, 83, 47);

        let title = rows.iter().position(|r| r.contains("Today's contacts"));
        assert!(title.is_some_and(|t| t > 20), "{rows:#?}");
        assert!(has(&rows, "W1AW"), "{rows:#?}");
    }

    #[test]
    fn at_the_operators_measured_sizes() {
        let app = cw_app_with_a_contact();

        for (w, h) in [(83, 47), (172, 48)] {
            let rows = screen(&app, w, h);
            assert_eq!(pane_rows(&rows).len(), 1, "{w}x{h}: {rows:#?}");
        }
    }

    #[test]
    fn a_third_column_changes_nothing_for_general() {
        let app = cw_app_with_a_contact();

        let rows = screen(&app, 240, 24);

        assert_eq!(pane_rows(&rows).len(), 1, "{rows:#?}");
        assert!(has(&rows, "1 logged"));
    }

    #[test]
    fn says_so_when_empty() {
        let app = cw_app();

        assert!(has(&screen(&app, 128, 24), "none yet"));
    }

    #[test]
    fn the_no_panel_body_does_not_repeat_the_list() {
        let app = app_with_a_contact();

        let rows = screen(&app, 128, 24);

        let hits = rows.iter().filter(|r| r.contains("W1AW")).count();
        assert_eq!(hits, 1, "{rows:#?}");
    }

    #[test]
    fn the_strip_and_hints_keep_their_place() {
        let app = cw_app_with_a_contact();

        let small = screen(&app, 80, 24);
        let wide = screen(&app, 172, 48);

        assert!(has(&small, "General | "));
        assert!(has(&wide, "General | "));
        assert!(has(&wide, "f freq"));
    }
}
