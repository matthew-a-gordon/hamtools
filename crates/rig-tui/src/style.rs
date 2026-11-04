//! The few styles every screen shares, so a colour is chosen in one place.

use ratatui::style::{Color, Style};

/// Secondary text: hints, hairline notes, placeholders.
pub(crate) fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

/// Errors.
pub(crate) fn red() -> Style {
    Style::default().fg(Color::Red)
}

/// Warnings, and text sent on the air.
pub(crate) fn yellow() -> Style {
    Style::default().fg(Color::Yellow)
}

/// The border of the focused pane.
pub(crate) fn focus_border(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    }
}
