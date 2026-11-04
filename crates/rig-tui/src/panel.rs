//! The Mode panel seam: what `App` needs from a panel, with no I/O.
//!
//! One panel is active at a time (the first whose [`ModePanel::applies`] matches the rig's mode).
//! A panel asks for rig work as data ([`ModePanel::due_requests`]) and receives the reply as data
//! ([`ModePanel::on_reply`]); `main` does the I/O. Panel state survives being hidden.

use std::collections::BTreeMap;

use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;

use crate::rig::RigMode;

/// What a panel is told about the app on every call.
#[derive(Debug, Clone, Copy)]
pub struct PanelCtx<'a> {
    pub connected: bool,
    pub callsign: Option<&'a str>,
    /// Soft key message slots, F1..=F6. Text is uppercase and may contain `{CALL}`.
    pub soft_keys: &'a BTreeMap<u8, String>,
    /// Whether the panel's own widget holds the keyboard focus.
    pub focused: bool,
}

/// What a panel asks `App` to do in response to a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelCommand {
    /// Key this text on the rig as CW. The only path that can transmit.
    Send(String),
    /// Show this in the status line.
    Status(String),
}

/// The logbook mode a rig mode maps to, with the RST to prefill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogMode {
    pub mode: String,
    pub submode: Option<String>,
    pub default_rst: Option<String>,
}

/// What a panel adds to a Contact being logged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogDefaults {
    pub mode: Option<LogMode>,
    pub rst: Option<String>,
    pub extra_adif: Vec<(String, String)>,
}

pub trait ModePanel {
    fn applies(&self, mode: &RigMode) -> bool;

    /// True when plain printable keys go to this panel's input line while it has focus.
    fn captures_text(&self) -> bool;

    fn handle_key(&mut self, key: KeyEvent, ctx: &PanelCtx) -> Vec<PanelCommand>;

    fn render(&self, frame: &mut Frame, area: Rect, ctx: &PanelCtx);

    /// Raw rig requests due now. Called once per tick, only while the panel is active.
    fn due_requests(&mut self) -> Vec<String>;

    /// The rig's reply (or the error) to one of the requests from [`Self::due_requests`].
    fn on_reply(&mut self, request: &str, reply: Result<String, String>) -> Result<(), String>;

    fn logbook_mode(&self, mode: &RigMode) -> Option<LogMode>;

    fn log_defaults(&self) -> LogDefaults {
        LogDefaults::default()
    }
}
