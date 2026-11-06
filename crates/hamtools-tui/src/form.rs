//! A stack of single-line text fields with focus cycling.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// One labelled line of text.
pub struct TextField {
    label: String,
    value: String,
    uppercase: bool,
    error: Option<String>,
}

impl TextField {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: String::new(),
            uppercase: false,
            error: None,
        }
    }

    /// Uppercase ASCII letters as they are typed; a non-ASCII character is refused with an
    /// error rather than uppercased (`\u{df}` would otherwise become "SS").
    pub fn uppercase(mut self) -> Self {
        self.uppercase = true;
        self
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }
}

/// What a key did to the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormEvent {
    /// A field or the focus changed (or the key was ignored).
    Edited,
    Submit,
    Cancel,
}

pub struct Form {
    fields: Vec<TextField>,
    focus: usize,
}

impl Form {
    pub fn new(fields: Vec<TextField>) -> Self {
        Self { fields, focus: 0 }
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    pub fn value(&self, index: usize) -> &str {
        &self.fields[index].value
    }

    pub fn error(&self, index: usize) -> Option<&str> {
        self.fields[index].error.as_deref()
    }

    pub fn set_error(&mut self, index: usize, message: impl Into<String>) {
        self.fields[index].error = Some(message.into());
    }

    pub fn clear_error(&mut self, index: usize) {
        self.fields[index].error = None;
    }

    /// Replace a field's text, as when a lookup proposes a value. Not uppercased: the caller
    /// passes what it wants shown.
    pub fn set_value(&mut self, index: usize, value: impl Into<String>) {
        self.fields[index].value = value.into();
        self.fields[index].error = None;
    }

    pub fn set_focus(&mut self, index: usize) {
        self.focus = index.min(self.fields.len().saturating_sub(1));
    }

    /// The form captures every key: Tab / Shift-Tab move focus, Enter submits, Esc cancels,
    /// characters and Backspace edit the focused field.
    pub fn handle_key(&mut self, key: KeyEvent) -> FormEvent {
        let n = self.fields.len();
        match key.code {
            KeyCode::Esc => return FormEvent::Cancel,
            KeyCode::Enter => return FormEvent::Submit,
            KeyCode::Tab if n > 0 => self.focus = (self.focus + 1) % n,
            KeyCode::BackTab if n > 0 => self.focus = (self.focus + n - 1) % n,
            KeyCode::Backspace if n > 0 => {
                self.fields[self.focus].value.pop();
            }
            KeyCode::Char(c) if n > 0 => {
                let field = &mut self.fields[self.focus];
                if field.uppercase {
                    if !c.is_ascii() {
                        field.error = Some(format!("{}: {c:?} is not ASCII", field.label));
                        return FormEvent::Edited;
                    }
                    field.value.push(c.to_ascii_uppercase());
                } else {
                    field.value.push(c);
                }
                field.error = None;
            }
            _ => {}
        }
        FormEvent::Edited
    }

    /// Draw one line per field (`Label: value`), with an indented red error line under a field
    /// that has one. The focused field's label is highlighted and the cursor placed after it.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let mut lines = Vec::new();
        let mut cursor = None;
        for (i, field) in self.fields.iter().enumerate() {
            let focused = i == self.focus;
            let label = format!("{}: ", field.label);
            if focused {
                cursor = Some((
                    label.chars().count() + field.value.chars().count(),
                    lines.len(),
                ));
            }
            let style = if focused {
                crate::theme::selected().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            lines.push(Line::from(vec![
                Span::styled(label, style),
                Span::raw(field.value.clone()),
            ]));
            if let Some(error) = &field.error {
                lines.push(Line::styled(format!("  {error}"), crate::theme::error()));
            }
        }
        frame.render_widget(Paragraph::new(lines), area);
        if let Some((x, y)) = cursor {
            let (x, y) = (x as u16, y as u16);
            if x < area.width && y < area.height {
                frame.set_cursor_position((area.x + x, area.y + y));
            }
        }
    }
}
