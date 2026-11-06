//! The styles every screen shares. The look itself lives in `hamtools_tui::theme`; this
//! re-exports the short names rig-tui uses.

use hamtools_tui::theme;
pub(crate) use hamtools_tui::theme::{chip, dim, focus_border, hint_line, panel, selected};
use ratatui::style::Style;

/// Errors.
pub(crate) fn red() -> Style {
    theme::error()
}

/// Warnings, and text sent on the air.
pub(crate) fn yellow() -> Style {
    theme::warn()
}
