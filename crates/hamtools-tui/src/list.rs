//! A scrollable list of text lines with a selection.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

/// What a key did to the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListEvent {
    /// The selection moved (or the key was ignored).
    Moved,
    /// Enter on the item at this index.
    Select(usize),
    Close,
}

pub struct ListOverlay {
    items: Vec<String>,
    selected: usize,
}

impl ListOverlay {
    pub fn new(items: Vec<String>) -> Self {
        Self { items, selected: 0 }
    }

    /// The selected index, or `None` for an empty list.
    pub fn selected(&self) -> Option<usize> {
        (!self.items.is_empty()).then_some(self.selected)
    }

    /// Up / `k` and Down / `j` move, Enter selects, Esc closes.
    pub fn handle_key(&mut self, key: KeyEvent) -> ListEvent {
        match key.code {
            KeyCode::Esc => return ListEvent::Close,
            KeyCode::Enter => {
                if let Some(i) = self.selected() {
                    return ListEvent::Select(i);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.items.len().saturating_sub(1));
            }
            _ => {}
        }
        ListEvent::Moved
    }

    /// Draw the items that fit in `area`, scrolled so the selection is visible; the selected
    /// line is marked `>` and reversed.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let height = usize::from(area.height);
        let top = (self.selected + 1).saturating_sub(height);
        let lines: Vec<Line> = self
            .items
            .iter()
            .enumerate()
            .skip(top)
            .take(height)
            .map(|(i, item)| {
                if i == self.selected {
                    Line::styled(
                        format!("> {item}"),
                        Style::default().add_modifier(Modifier::REVERSED),
                    )
                } else {
                    Line::from(format!("  {item}"))
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), area);
    }
}
