use hamtools_tui::form::{Form, FormEvent, TextField};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_str(form: &mut Form, s: &str) {
    for c in s.chars() {
        form.handle_key(key(KeyCode::Char(c)));
    }
}

fn form() -> Form {
    Form::new(vec![
        TextField::new("Call").uppercase(),
        TextField::new("Note"),
    ])
}

#[test]
fn typing_edits_the_focused_field_and_backspace_deletes() {
    let mut f = form();
    type_str(&mut f, "ab");
    f.handle_key(key(KeyCode::Backspace));
    f.handle_key(key(KeyCode::Tab));
    type_str(&mut f, "x y");
    assert_eq!(f.value(0), "A");
    assert_eq!(f.value(1), "x y");
}

#[test]
fn uppercase_fields_uppercase_as_typed_and_others_keep_case() {
    let mut f = form();
    type_str(&mut f, "w9Xyz");
    f.handle_key(key(KeyCode::Tab));
    type_str(&mut f, "Hello");
    assert_eq!(f.value(0), "W9XYZ");
    assert_eq!(f.value(1), "Hello");
}

#[test]
fn non_ascii_in_an_uppercase_field_is_rejected_with_an_error_not_uppercased() {
    let mut f = form();
    f.handle_key(key(KeyCode::Char('\u{df}')));
    assert_eq!(f.value(0), "");
    assert!(f.error(0).is_some());
    type_str(&mut f, "a");
    assert_eq!(f.error(0), None, "typing again clears the error");
}

#[test]
fn tab_and_backtab_cycle_and_wrap() {
    let mut f = form();
    assert_eq!(f.focus(), 0);
    f.handle_key(key(KeyCode::Tab));
    assert_eq!(f.focus(), 1);
    f.handle_key(key(KeyCode::Tab));
    assert_eq!(f.focus(), 0);
    f.handle_key(key(KeyCode::BackTab));
    assert_eq!(f.focus(), 1);
}

#[test]
fn enter_submits_and_esc_cancels() {
    let mut f = form();
    assert_eq!(f.handle_key(key(KeyCode::Enter)), FormEvent::Submit);
    assert_eq!(f.handle_key(key(KeyCode::Esc)), FormEvent::Cancel);
    assert_eq!(f.handle_key(key(KeyCode::Char('a'))), FormEvent::Edited);
}

#[test]
fn prefilled_values_and_set_error_render_with_the_error_line_under_the_field() {
    let mut f = Form::new(vec![
        TextField::new("Call").with_value("W9XYZ"),
        TextField::new("Note"),
    ]);
    f.set_error(0, "Call: not valid");
    let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
    terminal
        .draw(|frame| f.render(frame, Rect::new(0, 0, 20, 4)))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..4)
        .map(|y| (0..20).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    assert_eq!(
        rows,
        [
            "Call: W9XYZ         ",
            "  Call: not valid   ",
            "Note:               ",
            "                    ",
        ]
    );
}
