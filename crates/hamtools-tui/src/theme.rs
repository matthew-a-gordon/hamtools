//! The shared look: one place that decides colours, borders and chips, so every screen and
//! dialog agrees. Semantics, not hues, are named: `error` is not "red", it is what an error
//! looks like.
//!
//! Rules: secondary text is dark gray; the focused pane and every dialog has a yellow border,
//! the rest dark gray; the selected row is black on yellow; green/yellow/red mean good/warn/bad;
//! cyan marks a value that is a state rather than a number (VFO, mode in a list); magenta is the
//! app name only. Lamps (`RX`, `GPS`) are lit in a colour or dim gray.

use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};

/// Secondary text: hints, notes, placeholders, labels.
pub fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

/// Errors and bad states.
pub fn error() -> Style {
    Style::new().fg(Color::Red)
}

/// Warnings, and text sent on the air.
pub fn warn() -> Style {
    Style::new().fg(Color::Yellow)
}

/// Good states.
pub fn ok() -> Style {
    Style::new().fg(Color::Green)
}

/// A state value, or a changed field.
pub fn accent() -> Style {
    Style::new().fg(Color::Cyan)
}

/// The application name.
pub fn brand() -> Style {
    Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD)
}

/// The selected row of a list, or the focused field's label.
pub fn selected() -> Style {
    Style::new().fg(Color::Black).bg(Color::Yellow)
}

/// A border: yellow when focused, dark gray otherwise.
pub fn focus_border(focused: bool) -> Style {
    Style::new().fg(if focused {
        Color::Yellow
    } else {
        Color::DarkGray
    })
}

/// A pane: rounded, dark gray, yellow when focused.
pub fn panel<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    let title: Line = title.into();
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(focus_border(focused))
        .title(title.style(if focused {
            warn()
        } else {
            Style::new().fg(Color::Gray)
        }))
}

/// A dialog: thick and yellow, so it reads as sitting above the panes.
pub fn dialog<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
    let title: Line = title.into();
    Block::new()
        .borders(Borders::ALL)
        .border_set(border::THICK)
        .border_style(warn())
        .title(title.style(warn().add_modifier(Modifier::BOLD)))
}

/// A coloured chip such as ` USB 2400 `.
pub fn chip(text: &str, fg: Color, bg: Color) -> Span<'static> {
    Span::styled(format!(" {text} "), Style::new().fg(fg).bg(bg))
}

/// A lamp: lit in `color`, or dim gray when off.
pub fn lamp(label: &str, on: bool, color: Color) -> Span<'static> {
    if on {
        Span::styled(label.to_owned(), Style::new().fg(color).bold())
    } else {
        Span::styled(label.to_owned(), dim().add_modifier(Modifier::DIM))
    }
}

/// Key hints in one line: `key label  key label`, double-space separated. The first word of
/// each chunk is the key (plain, bold) and the rest its label (gray).
pub fn hint_line(text: &str) -> Line<'static> {
    let mut spans = Vec::new();
    for chunk in text.split("  ").filter(|c| !c.is_empty()) {
        match chunk.split_once(' ') {
            Some((key, label)) => {
                spans.push(Span::styled(key.to_owned(), Style::new().bold()));
                spans.push(Span::styled(format!(" {label}"), dim()));
            }
            None => spans.push(Span::styled(chunk.to_owned(), dim())),
        }
        spans.push(Span::raw("  "));
    }
    Line::from(spans)
}
