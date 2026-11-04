//! The Ctrl-K keybindings modal. It is built from the same key tables the host dispatches from
//! ([`keys::shared`] and the Activity's own [`KeyEntry`]s), so it cannot disagree with them.
//!
//! Three groups: the current Activity and focus, the shared strip keys, and the app-level
//! Chords (each Chord is followed by its alias). Blocked keys are greyed with a short reason.

use hamtools_tui::modal::Modal;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::OverlayAction;
use crate::keys::{self, Action, Key, KeyEntry, Scope};
use crate::style::{dim, yellow};

/// One line of the modal: a key on the left, its action on the right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub group: String,
    pub key: String,
    pub action: String,
    /// Why the key does nothing right now; the row is greyed.
    pub blocked: Option<String>,
}

/// How a key is written in the modal: `Ctrl-G`, `Right`, `F1`, `f`.
pub fn key_name(key: &Key) -> String {
    let base = match key.code {
        KeyCode::Char(c) => c.to_ascii_uppercase().to_string(),
        KeyCode::F(n) => format!("F{n}"),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        other => format!("{other:?}"),
    };
    if key.mods.contains(KeyModifiers::CONTROL) {
        return format!("Ctrl-{base}");
    }
    match key.code {
        // A plain letter is shown as typed, so `s` and `S` stay apart.
        KeyCode::Char(c) => c.to_string(),
        _ => base,
    }
}

const TEXT_FOCUS: &str = "text area has focus";

/// The modal's rows for the foreground Activity. `text_focus` greys the plain keys, which a
/// text area would swallow.
pub fn rows(activity: &str, activity_keys: &[KeyEntry], text_focus: bool) -> Vec<Row> {
    let focus = if text_focus { "text area" } else { "strip" };
    let own = format!("{activity} ({focus} focus)");
    let strip = "Shared strip keys".to_string();
    let chords = "App-level Chords".to_string();

    let blocked_for = |entry: &KeyEntry| -> Option<String> {
        let reason = entry.blocked.map(String::from);
        let plain = matches!(entry.scope, Scope::Plain | Scope::Alias);
        if plain && text_focus {
            return Some(TEXT_FOCUS.to_string());
        }
        reason
    };
    let row = |group: &String, key: String, action: &str, blocked: Option<String>| Row {
        group: group.clone(),
        key,
        action: action.to_string(),
        blocked,
    };

    let mut out = Vec::new();
    for e in activity_keys {
        out.push(row(&own, key_name(&e.key), e.label, blocked_for(e)));
    }
    // Focus keys and Soft keys belong to the text area's focus, so they sit with the Activity.
    out.push(row(&own, "F1-F6".into(), "soft keys 1-6", None));
    for e in keys::shared().iter().filter(|e| {
        matches!(e.action, Action::Activity("focus")) && matches!(e.key.code, KeyCode::Tab)
    }) {
        out.push(row(&own, key_name(&e.key), e.label, None));
    }
    for e in keys::shared().iter().filter(|e| {
        matches!(e.action, Action::Activity("focus")) && matches!(e.key.code, KeyCode::Esc)
    }) {
        out.push(row(&own, key_name(&e.key), e.label, None));
    }

    let mut digits = false;
    for e in keys::shared() {
        if e.scope != Scope::Plain || matches!(e.action, Action::Activity(_)) {
            continue;
        }
        if matches!(e.action, Action::FreqDigit(_)) {
            if !digits {
                digits = true;
                out.push(row(&strip, "0-9".into(), e.label, blocked_for(e)));
            }
            continue;
        }
        let mut blocked = blocked_for(e);
        if e.action == Action::Quit && activity == "General" && !text_focus {
            blocked = None;
        }
        out.push(row(&strip, key_name(&e.key), e.label, blocked));
    }

    for e in keys::shared().iter().filter(|e| e.scope == Scope::Chord) {
        out.push(row(&chords, key_name(&e.key), e.label, None));
        for alias in keys::shared()
            .iter()
            .filter(|a| a.scope == Scope::Alias && a.action == e.action)
        {
            out.push(row(
                &chords,
                key_name(&alias.key),
                alias.label,
                blocked_for(alias),
            ));
        }
    }
    out
}

pub(crate) struct KeybindingsModal {
    rows: Vec<Row>,
    filter: String,
    editing: bool,
    scroll: usize,
}

impl KeybindingsModal {
    pub(crate) fn new(rows: Vec<Row>) -> Self {
        Self {
            rows,
            filter: String::new(),
            editing: false,
            scroll: 0,
        }
    }

    fn matches(&self, row: &Row) -> bool {
        let needle = self.filter.to_lowercase();
        needle.is_empty()
            || [&row.key, &row.action, &row.group]
                .iter()
                .any(|s| s.to_lowercase().contains(&needle))
            || row
                .blocked
                .as_ref()
                .is_some_and(|b| b.to_lowercase().contains(&needle))
    }

    /// The lines to show: a heading before each group that has a visible row.
    fn lines(&self) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        let mut group: Option<&str> = None;
        for row in self.rows.iter().filter(|r| self.matches(r)) {
            if group != Some(row.group.as_str()) {
                group = Some(row.group.as_str());
                out.push(Line::styled(
                    row.group.clone(),
                    yellow().add_modifier(ratatui::style::Modifier::BOLD),
                ));
            }
            let mut text = format!("  {:<10}{}", row.key, row.action);
            let style = match &row.blocked {
                Some(reason) => {
                    text = format!("{text:<38}{reason}");
                    dim()
                }
                None => ratatui::style::Style::default(),
            };
            out.push(Line::from(Span::styled(text, style)));
        }
        out
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> OverlayAction {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('k' | 'K')) {
            return OverlayAction::Close;
        }
        if self.editing {
            match key.code {
                KeyCode::Esc => {
                    self.filter.clear();
                    self.editing = false;
                }
                KeyCode::Enter => self.editing = false,
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                    self.filter.push(c);
                }
                _ => return self.scroll_key(key),
            }
            self.scroll = 0;
            return OverlayAction::Keep;
        }
        match key.code {
            KeyCode::Esc => return OverlayAction::Close,
            KeyCode::Char('/') => self.editing = true,
            _ => return self.scroll_key(key),
        }
        OverlayAction::Keep
    }

    fn scroll_key(&mut self, key: KeyEvent) -> OverlayAction {
        let last = self.lines().len().saturating_sub(1);
        match key.code {
            KeyCode::Down => self.scroll = (self.scroll + 1).min(last),
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageDown => self.scroll = (self.scroll + 10).min(last),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            _ => {}
        }
        OverlayAction::Keep
    }

    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        let inner = Modal::new("Keybindings", 78, 22).render(frame, area);
        let head = if self.editing || !self.filter.is_empty() {
            Line::from(format!("/ {}", self.filter))
        } else {
            Line::styled("/ filter   Up/Down scroll   Esc close", dim())
        };
        let lines = self.lines();
        let room = usize::from(inner.height.saturating_sub(1));
        // Keep the last page full when scrolled to the end.
        let start = self.scroll.min(lines.len().saturating_sub(room));
        let mut text = vec![head];
        text.extend(lines.into_iter().skip(start).take(room));
        frame.render_widget(Paragraph::new(text), inner);
    }
}
