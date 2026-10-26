use kx3_cw::{App, Command, Line, Source};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        assert_eq!(app.handle_key(key(KeyCode::Char(c))), None);
    }
}

#[test]
fn typed_text_is_uppercased_and_backspace_edits_it() {
    let mut app = App::new();

    type_str(&mut app, "cq x");
    app.handle_key(key(KeyCode::Backspace));
    type_str(&mut app, "de");

    assert_eq!(app.input(), "CQ DE");
}

#[test]
fn enter_sends_the_input_clears_it_and_echoes_it_into_the_transcript() {
    let mut app = App::new();
    type_str(&mut app, "cq cq");

    let command = app.handle_key(key(KeyCode::Enter));

    assert_eq!(command, Some(Command::Send("CQ CQ".to_string())));
    assert_eq!(app.input(), "");
    assert_eq!(
        app.lines(),
        [Line {
            source: Source::Tx,
            text: "CQ CQ".to_string()
        }]
    );
}

#[test]
fn enter_on_an_empty_input_sends_nothing() {
    let mut app = App::new();

    assert_eq!(app.handle_key(key(KeyCode::Enter)), None);
    assert!(app.lines().is_empty());
}

#[test]
fn escape_and_ctrl_c_quit() {
    assert_eq!(
        App::new().handle_key(key(KeyCode::Esc)),
        Some(Command::Quit)
    );
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(App::new().handle_key(ctrl_c), Some(Command::Quit));
}

fn rx(text: &str) -> Line {
    Line {
        source: Source::Rx,
        text: text.to_string(),
    }
}

#[test]
fn decoded_text_accumulates_on_one_rx_line() {
    let mut app = App::new();

    app.on_rx("CQ CQ ");
    app.on_rx("DE W9");
    app.on_rx("XYZ");

    assert_eq!(app.lines(), [rx("CQ CQ DE W9XYZ")]);
}

#[test]
fn rx_after_a_transmission_starts_a_new_line() {
    let mut app = App::new();
    app.on_rx("CQ");
    type_str(&mut app, "de me");
    app.handle_key(key(KeyCode::Enter));

    app.on_rx("R R");

    assert_eq!(
        app.lines(),
        [
            rx("CQ"),
            Line {
                source: Source::Tx,
                text: "DE ME".to_string()
            },
            rx("R R")
        ]
    );
}

#[test]
fn empty_decoded_text_changes_nothing() {
    let mut app = App::new();

    app.on_rx("");

    assert!(app.lines().is_empty());
}

#[test]
fn call_placeholder_expands_to_the_configured_callsign_when_sending() {
    let mut app = App::with_callsign(Some("KD0RHV".to_string()));
    type_str(&mut app, "cq cq de {call} {call} k");

    let command = app.handle_key(key(KeyCode::Enter));

    assert_eq!(
        command,
        Some(Command::Send("CQ CQ DE KD0RHV KD0RHV K".to_string()))
    );
    assert_eq!(
        app.lines(),
        [Line {
            source: Source::Tx,
            text: "CQ CQ DE KD0RHV KD0RHV K".to_string()
        }]
    );
}

#[test]
fn call_placeholder_without_a_callsign_blocks_the_send_and_keeps_the_input() {
    let mut app = App::new();
    type_str(&mut app, "de {call}");

    let command = app.handle_key(key(KeyCode::Enter));

    assert_eq!(command, None);
    assert_eq!(app.input(), "DE {CALL}");
    assert!(app.lines().is_empty());
    assert!(
        app.status().is_some_and(|s| s.contains("callsign")),
        "{:?}",
        app.status()
    );
}

mod macros {
    use std::collections::BTreeMap;

    use super::*;

    fn app() -> App {
        App::with_callsign(Some("KD0RHV".to_string())).with_macros(BTreeMap::from([
            (1, "CQ CQ CQ DE {CALL} {CALL} K".to_string()),
            (2, "73 TU".to_string()),
        ]))
    }

    fn f(n: u8) -> KeyEvent {
        key(KeyCode::F(n))
    }

    #[test]
    fn a_function_key_sends_its_macro_with_the_callsign_filled_in() {
        let mut app = app();

        let command = app.handle_key(f(1));

        assert_eq!(
            command,
            Some(Command::Send("CQ CQ CQ DE KD0RHV KD0RHV K".to_string()))
        );
        assert_eq!(
            app.lines(),
            [Line {
                source: Source::Tx,
                text: "CQ CQ CQ DE KD0RHV KD0RHV K".to_string()
            }]
        );
    }

    #[test]
    fn sending_a_macro_leaves_the_half_typed_input_alone() {
        let mut app = app();
        type_str(&mut app, "de w9");

        app.handle_key(f(2));

        assert_eq!(app.input(), "DE W9");
    }

    #[test]
    fn an_unset_function_key_says_so_and_sends_nothing() {
        let mut app = app();

        assert_eq!(app.handle_key(f(3)), None);

        assert_eq!(app.status(), Some("F3 is not set"));
        assert!(app.lines().is_empty());
    }

    #[test]
    fn a_macro_using_the_callsign_is_blocked_when_none_is_configured() {
        let mut app = App::new().with_macros(BTreeMap::from([(1, "DE {CALL}".to_string())]));

        assert_eq!(app.handle_key(f(1)), None);

        assert!(
            app.status().is_some_and(|s| s.contains("callsign")),
            "{:?}",
            app.status()
        );
        assert!(app.lines().is_empty());
    }

    #[test]
    fn function_keys_beyond_f6_do_nothing() {
        let mut app = app();

        assert_eq!(app.handle_key(f(7)), None);

        assert_eq!(app.status(), None);
        assert!(app.lines().is_empty());
    }
}
