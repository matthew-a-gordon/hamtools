use rig_tui::{App, render};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn screen(app: &App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render(app, frame)).unwrap();
    let buffer = terminal.backend().buffer();
    buffer
        .content()
        .chunks(width as usize)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
}

#[test]
fn shows_the_transcript_and_the_input_line() {
    let mut app = App::new();
    app.on_rx("CQ CQ DE W9XYZ");
    type_str(&mut app, "de me");

    let rows = screen(&app, 40, 10);

    assert!(
        rows.iter().any(|r| r.contains("CQ CQ DE W9XYZ")),
        "{rows:#?}"
    );
    assert!(rows.iter().any(|r| r.contains("> DE ME")), "{rows:#?}");
}

#[test]
fn long_text_wraps_and_the_newest_text_stays_visible() {
    let mut app = App::new();
    let text: String = "0123456789".repeat(10);
    app.on_rx(&text);

    // 30 wide: 28 usable columns inside the border. 8 high: 3 rows of transcript.
    let rows = screen(&app, 30, 8);

    assert!(
        rows.iter().any(|r| r.contains(&text[84..100])),
        "tail missing: {rows:#?}"
    );
    assert!(
        !rows.iter().any(|r| r.contains(&text[0..28])),
        "head should have scrolled off"
    );
}

#[test]
fn shows_a_status_message_such_as_a_rig_error() {
    let mut app = App::new();

    app.set_status(Some("rig: connection closed".to_string()));
    let rows = screen(&app, 60, 8);
    assert!(
        rows.iter().any(|r| r.contains("rig: connection closed")),
        "{rows:#?}"
    );

    app.set_status(None);
    let rows = screen(&app, 60, 8);
    assert!(
        !rows.iter().any(|r| r.contains("rig: connection closed")),
        "{rows:#?}"
    );
}

mod macro_hints {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn bound_function_keys_are_listed_with_their_text() {
        let app = App::with_callsign(Some("KD0RHV".to_string())).with_macros(BTreeMap::from([
            (1, "CQ CQ CQ DE {CALL} K".to_string()),
            (2, "73 TU".to_string()),
        ]));

        let rows = screen(&app, 80, 10);

        let hint = rows
            .iter()
            .find(|r| r.contains("F1 "))
            .expect("no hint row");
        assert!(hint.contains("F1 CQ CQ CQ DE KD0RHV K"), "{hint:?}");
        assert!(hint.contains("F2 73 TU"), "{hint:?}");
        assert!(!hint.contains("F3"), "{hint:?}");
    }

    #[test]
    fn long_macros_are_shortened_so_every_key_still_fits() {
        let app = App::new().with_macros(BTreeMap::from([
            (1, "A".repeat(100)),
            (2, "73 TU".to_string()),
        ]));

        let rows = screen(&app, 60, 10);

        let hint = rows
            .iter()
            .find(|r| r.contains("F1 "))
            .expect("no hint row");
        assert!(hint.contains("F2 73 TU"), "{hint:?}");
    }

    #[test]
    fn no_hint_row_without_macros() {
        let rows = screen(&App::new(), 80, 10);

        assert!(!rows.iter().any(|r| r.contains("F1 ")), "{rows:#?}");
    }
}
