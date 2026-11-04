mod common;

use std::collections::BTreeMap;

use common::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use ratatui::style::Modifier;
use rig_tui::{App, RigEvent, render};

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

#[test]
fn shows_the_transcript_and_the_input_line() {
    let mut app = cw_app();
    rx(&mut app, "TB014CQ CQ DE W9XYZ;");
    type_str(&mut app, "de me");

    let rows = screen(&app, 40, 14);

    assert!(has(&rows, "CQ CQ DE W9XYZ"), "{rows:#?}");
    assert!(has(&rows, "> DE ME"), "{rows:#?}");
}

#[test]
fn long_text_wraps_and_the_newest_text_stays_visible() {
    let mut app = cw_app();
    let text: String = "0123456789".repeat(4);
    rx(&mut app, &format!("TB0{}{text};", text.len()));

    // 20 wide: 18 usable columns inside the border. 12 high: the body is 6 rows, 1 of transcript.
    let rows = screen(&app, 20, 12);

    assert!(has(&rows, &text[36..40]), "tail missing: {rows:#?}");
    assert!(!has(&rows, &text[0..18]), "head should have scrolled off");
}

#[test]
fn shows_a_status_message_such_as_a_rig_error() {
    let mut app = cw_app();

    app.set_status(Some("rig: connection closed".to_string()));
    assert!(has(&screen(&app, 60, 14), "rig: connection closed"));

    app.set_status(None);
    assert!(!has(&screen(&app, 60, 14), "rig: connection closed"));
}

#[test]
fn a_mode_without_a_panel_says_so_in_the_body() {
    let app = connected_app_in(App::new(), "USB");

    let rows = screen(&app, 60, 14);

    assert!(has(&rows, "No panel for USB"), "{rows:#?}");
}

mod hint_line {
    use super::*;

    #[test]
    fn it_lists_the_keys_for_the_focus() {
        let mut app = connected_app_in(App::new(), "CW");
        let rows = screen(&app, 80, 14);
        assert!(has(&rows, "Tab input"), "{rows:#?}");
        assert!(has(&rows, "q quit"), "{rows:#?}");

        app.on_key(key(KeyCode::Tab));
        let rows = screen(&app, 80, 14);
        assert!(has(&rows, "Enter send"), "{rows:#?}");
        assert!(!has(&rows, "q quit"), "{rows:#?}");
    }

    #[test]
    fn it_names_the_log_list_and_config_keys_at_80_columns() {
        let mut app = connected_app_in(App::new(), "CW");
        let rows = screen(&app, 80, 14);
        assert!(has(&rows, "l log"), "{rows:#?}");
        assert!(has(&rows, "L list"), "{rows:#?}");
        assert!(has(&rows, "c config"), "{rows:#?}");
        assert!(has(&rows, "q quit"), "{rows:#?}");

        app.on_key(key(KeyCode::Tab));
        let rows = screen(&app, 80, 14);
        assert!(has(&rows, "Ctrl-L log"), "{rows:#?}");
    }

    #[test]
    fn it_does_not_offer_the_input_line_where_there_is_none() {
        let app = connected_app_in(App::new(), "USB");

        let rows = screen(&app, 80, 14);

        assert!(has(&rows, "q quit"), "{rows:#?}");
        assert!(!has(&rows, "Tab"), "{rows:#?}");
    }
}

mod soft_key_bar {
    use super::*;

    fn app(keys: BTreeMap<u8, String>) -> App {
        connected_app_in(
            App::with_callsign(Some("KD0RHV".to_string())).with_soft_keys(keys),
            "CW",
        )
    }

    #[test]
    fn bound_function_keys_are_listed_with_their_text() {
        let app = app(BTreeMap::from([
            (1, "CQ CQ CQ DE {CALL} K".to_string()),
            (2, "73 TU".to_string()),
        ]));

        let rows = screen(&app, 80, 14);

        let bar = rows.iter().find(|r| r.contains("F1 ")).expect("no bar");
        assert!(bar.contains("F1 CQ CQ CQ DE KD0RHV K"), "{bar:?}");
        assert!(bar.contains("F2 73 TU"), "{bar:?}");
        assert!(!bar.contains("F3"), "{bar:?}");
    }

    #[test]
    fn the_bar_is_drawn_in_reverse_video_under_the_panel() {
        let app = app(BTreeMap::from([(1, "CQ".to_string())]));
        let mut terminal = Terminal::new(TestBackend::new(40, 14)).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
        let buffer = terminal.backend().buffer();

        let row = (0..14u16)
            .find(|&y| (0..40u16).any(|x| buffer[(x, y)].symbol() == "F") && y > 4)
            .expect("no bar row");

        assert!(
            buffer[(0, row)].modifier.contains(Modifier::REVERSED),
            "row {row} is not reverse video"
        );
    }

    #[test]
    fn long_messages_are_shortened_so_every_key_still_fits() {
        let app = app(BTreeMap::from([
            (1, "A".repeat(100)),
            (2, "73 TU".to_string()),
        ]));

        let rows = screen(&app, 60, 14);

        let bar = rows.iter().find(|r| r.contains("F1 ")).expect("no bar");
        assert!(bar.contains("F2 73 TU"), "{bar:?}");
    }

    #[test]
    fn no_bar_without_soft_keys() {
        let app = app(BTreeMap::new());

        assert!(!has(&screen(&app, 80, 14), "F1 "));
    }

    #[test]
    fn the_bar_is_hidden_with_the_panel() {
        let mut app = app(BTreeMap::from([(1, "CQ".to_string())]));
        app.on_rig(mode_event("USB"));

        assert!(!has(&screen(&app, 80, 14), "F1 CQ"));
    }
}

#[test]
fn the_disconnected_strip_says_why() {
    let mut app = connected_app_in(App::new(), "CW");
    app.on_rig(RigEvent::Disconnected("gone".to_string()));

    let rows = screen(&app, 60, 14);

    assert!(has(&rows, "disconnected"), "{rows:#?}");
    assert!(has(&rows, "gone"), "{rows:#?}");
}
