use kx3_cw::{App, render};
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
