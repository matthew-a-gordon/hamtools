//! The Ctrl-G go-to palette: a type-to-filter list over the live Activity. Rows are built by the
//! host; today they are the hosted Activities, each with a one-line live state. Later rows
//! (upcoming Activation plans) go in front of these by adding a [`Target`].

use hamtools_tui::modal::Modal;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::style::dim;

/// What choosing a row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    /// Bring the hosted Activity at this index to the foreground.
    Activity(usize),
}

#[derive(Debug, Clone)]
pub(crate) struct PaletteRow {
    pub target: Target,
    pub name: String,
    /// One line of live state: a pass countdown, a plan's date.
    pub state: String,
    /// Why the Active rig cannot run it. Such a row cannot be chosen.
    pub unavailable: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PaletteOutcome {
    Keep,
    Close,
    Choose(Target),
}

pub(crate) struct Palette {
    rows: Vec<PaletteRow>,
    filter: String,
    selected: usize,
}

impl Palette {
    pub(crate) fn new(rows: Vec<PaletteRow>) -> Self {
        Self {
            rows,
            filter: String::new(),
            selected: 0,
        }
    }

    fn visible(&self) -> Vec<&PaletteRow> {
        let needle = self.filter.to_lowercase();
        self.rows
            .iter()
            .filter(|r| {
                needle.is_empty()
                    || r.name.to_lowercase().contains(&needle)
                    || r.state.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> PaletteOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let count = self.visible().len();
        match key.code {
            KeyCode::Char('g' | 'G') if ctrl => return PaletteOutcome::Close,
            KeyCode::Esc => return PaletteOutcome::Close,
            KeyCode::Enter => {
                let chosen = self
                    .visible()
                    .get(self.selected)
                    .map(|r| (r.target, r.unavailable.is_some()));
                if let Some((target, false)) = chosen {
                    return PaletteOutcome::Choose(target);
                }
            }
            KeyCode::Down => self.selected = (self.selected + 1).min(count.saturating_sub(1)),
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Backspace => {
                self.filter.pop();
                self.selected = 0;
            }
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                self.filter.push(c);
                self.selected = 0;
            }
            _ => {}
        }
        PaletteOutcome::Keep
    }

    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        let visible = self.visible();
        let height = (visible.len() as u16 + 4).max(6);
        let inner = Modal::new("Go to", 64, height).render(frame, area);
        let mut text = vec![Line::from(format!("> {}", self.filter))];
        if visible.is_empty() {
            text.push(Line::styled("nothing matches", dim()));
        }
        for (i, row) in visible.iter().enumerate() {
            let marker = if i == self.selected { '>' } else { ' ' };
            let mut line = format!("{marker} {:<10} {}", row.name, row.state);
            let mut style = Style::default();
            if let Some(reason) = &row.unavailable {
                line = format!("{marker} {:<10} {reason}", row.name);
                style = dim();
            } else if i == self.selected {
                style = style.add_modifier(Modifier::BOLD);
            }
            text.push(Line::styled(line, style));
        }
        frame.render_widget(Paragraph::new(text), inner);
    }
}
