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
use crate::softkeys::{Live, ResolvedKey};

/// What a panel is told about the app on every call.
#[derive(Debug, Clone, Copy)]
pub struct PanelCtx<'a> {
    pub connected: bool,
    /// The live values Soft key variables expand from, read at press time.
    pub live: Live<'a>,
    /// Soft keys F1..=F6 after layering, each with its layer and validity.
    pub soft_keys: &'a BTreeMap<u8, ResolvedKey>,
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

/// Who is logging, which decides the Contact form's fields and the reference extras it writes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    General,
    Satellite,
    SotaActivator,
    SotaChaser,
    PotaActivator,
    PotaHunter,
}

impl Role {
    /// Activators log in quick-entry style: Enter saves and the form reopens on Call.
    pub fn is_activator(self) -> bool {
        matches!(self, Role::SotaActivator | Role::PotaActivator)
    }

    /// The Role has a "Their ref" field (and no Grid or Name).
    pub fn has_their_ref(self) -> bool {
        !matches!(self, Role::General | Role::Satellite)
    }
}

/// What a panel or Activity adds to a Contact being logged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogDefaults {
    pub mode: Option<LogMode>,
    pub rst: Option<String>,
    pub extra_adif: Vec<(String, String)>,
    pub role: Role,
    /// My references from the active Activation plan, never typed per Contact. More than one is
    /// a POTA n-fer; SOTA uses the first.
    pub my_refs: Vec<String>,
    /// Their reference to prefill (a Hunter's spot).
    pub their_ref: Option<String>,
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

    /// Whether a send from this panel may still be going out; blocks switching the Active rig.
    fn sending(&self) -> bool {
        false
    }

    /// Typed input that has not been sent, which quitting would lose.
    fn has_unsent_input(&self) -> bool {
        false
    }

    fn log_defaults(&self) -> LogDefaults {
        LogDefaults::default()
    }
}
