use hamtools_tui::list::{ListEvent, ListOverlay};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn items(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("item {i}")).collect()
}

fn rows(list: &ListOverlay, w: u16, h: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal
        .draw(|frame| list.render(frame, Rect::new(0, 0, w, h)))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..h)
        .map(|y| (0..w).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

#[test]
fn down_and_up_move_the_selection_and_stop_at_the_ends() {
    let mut list = ListOverlay::new(items(3));
    assert_eq!(list.selected(), Some(0));
    list.handle_key(key(KeyCode::Up));
    assert_eq!(list.selected(), Some(0));
    list.handle_key(key(KeyCode::Down));
    list.handle_key(key(KeyCode::Char('j')));
    list.handle_key(key(KeyCode::Down));
    assert_eq!(list.selected(), Some(2));
    list.handle_key(key(KeyCode::Char('k')));
    assert_eq!(list.selected(), Some(1));
}

#[test]
fn enter_selects_the_current_item_and_esc_closes() {
    let mut list = ListOverlay::new(items(3));
    list.handle_key(key(KeyCode::Down));
    assert_eq!(list.handle_key(key(KeyCode::Enter)), ListEvent::Select(1));
    assert_eq!(list.handle_key(key(KeyCode::Esc)), ListEvent::Close);
    assert_eq!(list.handle_key(key(KeyCode::Down)), ListEvent::Moved);
}

#[test]
fn empty_list_has_no_selection_and_enter_does_nothing() {
    let mut list = ListOverlay::new(Vec::new());
    assert_eq!(list.selected(), None);
    assert_eq!(list.handle_key(key(KeyCode::Enter)), ListEvent::Moved);
    assert_eq!(list.handle_key(key(KeyCode::Down)), ListEvent::Moved);
    assert_eq!(list.selected(), None);
}

#[test]
fn render_scrolls_to_keep_the_selection_visible() {
    let mut list = ListOverlay::new(items(6));
    assert_eq!(
        rows(&list, 10, 3),
        ["> item 1   ", "  item 2   ", "  item 3   "].map(|s| &s[..10])
    );
    for _ in 0..4 {
        list.handle_key(key(KeyCode::Down));
    }
    assert_eq!(
        rows(&list, 10, 3),
        ["  item 3   ", "  item 4   ", "> item 5   "].map(|s| &s[..10])
    );
}
