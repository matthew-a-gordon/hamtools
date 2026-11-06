//! A bordered box centred over whatever is already drawn.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Clear;

use crate::theme;

/// A rectangle of at most `width` x `height`, centred in `area`.
pub fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let (width, height) = (width.min(area.width), height.min(area.height));
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

/// A titled, bordered modal of a wanted size (clamped to the screen).
pub struct Modal {
    title: String,
    width: u16,
    height: u16,
}

impl Modal {
    pub fn new(title: impl Into<String>, width: u16, height: u16) -> Self {
        Self {
            title: title.into(),
            width,
            height,
        }
    }

    /// Clear the modal's area, draw its border and title, and return the inner rectangle
    /// for the caller's content.
    pub fn render(&self, frame: &mut Frame, area: Rect) -> Rect {
        let rect = centred(area, self.width, self.height);
        let block = theme::dialog(format!(" {} ", self.title));
        let inner = block.inner(rect);
        frame.render_widget(Clear, rect);
        frame.render_widget(block, rect);
        inner
    }
}
