//! The Activity seam: what the host `App` needs from the thing that owns the body of the screen.
//!
//! An Activity does no I/O. Key events, ticks and rig replies go in; [`Command`]s (a closed
//! enum that `main` performs) and status text come out. The App hosts exactly one Activity in
//! the foreground; only that one is ticked. Activities are registered statically in this crate.

use std::collections::BTreeMap;
use std::time::Instant;

use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;

use crate::keys::KeyEntry;
use crate::layout::Panes;
use crate::panel::{LogDefaults, LogMode};
use crate::rig::{Command, RigMode};
use crate::softkeys::{Live, ResolvedKey};

/// What an Activity is told about the app on every call.
#[derive(Debug, Clone, Copy)]
pub struct ActivityCtx<'a> {
    pub connected: bool,
    /// The live values Soft key variables expand from, read at press time.
    pub live: Live<'a>,
    /// Soft keys F1..=F6 after layering, each with its layer and validity.
    pub soft_keys: &'a BTreeMap<u8, ResolvedKey>,
    /// The last mode the rig reported, however old.
    pub rig_mode: Option<&'a RigMode>,
    /// The mode only if the rig reported it within the last two seconds.
    pub fresh_mode: Option<&'a RigMode>,
    /// Recent Contacts, newest first (filled for rendering only).
    pub recent: &'a [String],
    /// The panes the window has room for (see [`crate::layout`]).
    pub panes: Panes,
}

/// What an Activity returns: rig commands from the closed set, and an optional status line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityOutput {
    pub commands: Vec<Command>,
    pub status: Option<String>,
}

pub trait Activity {
    /// The name shown in the shared top strip.
    fn name(&self) -> &'static str;

    /// A key the shared shell did not take. `None`: not handled here, so the shared strip keys
    /// get it.
    fn on_key(&mut self, key: KeyEvent, ctx: &ActivityCtx) -> Option<ActivityOutput>;

    /// Called on every tick, and only while this Activity is in the foreground.
    fn on_tick(&mut self, now: Instant, ctx: &ActivityCtx) -> Vec<Command>;

    /// The rig's reply (or the error) to a raw request this Activity asked for.
    fn on_reply(&mut self, request: &str, reply: Result<String, String>) -> Result<(), String>;

    /// The rig reported a mode.
    fn on_mode(&mut self, ctx: &ActivityCtx);

    /// Whether a text-entering area has focus, so it owns every printable key.
    fn text_focus(&self, ctx: &ActivityCtx) -> bool;

    /// Whether the Activity currently shows a Mode panel.
    fn has_panel(&self, ctx: &ActivityCtx) -> bool;

    /// The logbook mode and Contact defaults for the Contact form.
    fn log_hints(&self, mode: &RigMode) -> (Option<LogMode>, LogDefaults);

    /// The Activity's own key hint lines (one or two, split by `\n`).
    fn hint(&self, ctx: &ActivityCtx) -> &'static str;

    fn render(&self, frame: &mut Frame, area: Rect, ctx: &ActivityCtx);

    /// Why tuning from outside this Activity (the band plan modal) must be refused right now:
    /// `Some("Doppler tracking")` while Satellite is correcting the dial itself.
    fn tuning_blocked(&self) -> Option<&'static str> {
        None
    }

    /// The Activity's own plain keys, checked against the reserved table when it is hosted.
    fn keys(&self) -> Vec<KeyEntry> {
        Vec::new()
    }

    /// One line of live state for the go-to palette (a pass countdown, the mode).
    fn state_line(&self, _ctx: &ActivityCtx) -> String {
        String::new()
    }

    /// Why the Active rig cannot run this Activity, if it cannot. The palette shows the reason
    /// and will not enter it.
    fn unavailable(&self) -> Option<String> {
        None
    }

    /// One line for each thing quitting now would lose (unsent CW input, active Doppler
    /// tracking). Empty when nothing would be lost.
    fn quit_warnings(&self) -> Vec<String> {
        Vec::new()
    }
}
