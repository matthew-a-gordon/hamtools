//! The Activity seam: what the host `App` needs from the thing that owns the body of the screen.
//!
//! An Activity does no I/O. Key events, ticks and rig replies go in; [`Command`]s (a closed
//! enum that `main` performs) and status text come out. The App hosts exactly one Activity in
//! the foreground; only that one is ticked. Activities are registered statically in this crate.

use std::collections::BTreeMap;
use std::time::Instant;

use chrono::{DateTime, Utc};
use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;

use crate::keys::KeyEntry;
use crate::layout::Panes;
use crate::panel::{LogDefaults, LogMode};
use crate::rig::{Command, RigMode};
use crate::rigs::Capability;
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
    /// The UTC wall clock: the same time the strip shows and Contacts are stamped with.
    pub utc: DateTime<Utc>,
    /// The receive frequency in Hz, only if the rig reported it within the last two seconds.
    pub freq: Option<u64>,
}

/// News from outside the rig and the keyboard that an Activity may care about. A closed set;
/// `main` produces these (downloads finishing), the App hands them to every hosted Activity.
#[derive(Clone)]
pub enum HostEvent {
    /// Satellite data was (re)loaded from the cache.
    SatelliteData(Box<crate::satellite::SatelliteData>),
    /// A satellite data download started or finished.
    SatelliteDownloading(bool),
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

    /// What the Active rig must be able to do for this Activity to run. A rig lacking one of
    /// these cannot run the Activity: it is unavailable, and a switch to such a rig drops to
    /// General.
    fn requires(&self) -> &'static [Capability] {
        &[]
    }

    /// Whether this Activity has a transmission in progress (a CW send not yet drained), which
    /// blocks switching the Active rig.
    fn sending(&self) -> bool {
        false
    }

    /// The Active rig changes from rig `from` to rig `to` (indices into the profile list). An
    /// Activity with per-rig state (a CW transcript, Soft key state) parks `from`'s and restores
    /// `to`'s.
    fn switch_rig(&mut self, _from: usize, _to: usize) {}

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

    /// This Activity leaves the foreground (the operator chose another, or the rig it needs went
    /// away). Stop anything that acts on the rig by itself; the radio is not restored. Returns
    /// a line for the status line when something was stopped.
    fn on_leave(&mut self) -> Option<String> {
        None
    }

    /// News from outside (see [`HostEvent`]). Delivered to every hosted Activity, foreground or
    /// not; an Activity ignores what is not its own. May return a status line.
    fn on_event(&mut self, _event: &HostEvent) -> Option<String> {
        None
    }

    /// Why a Contact cannot be logged right now from this Activity, if it cannot (Satellite
    /// needs a tracked transmitter whose frequencies were sent). Checked before the rig's own
    /// freshness checks.
    fn log_blocked(&self) -> Option<String> {
        None
    }

    /// One line for each thing quitting now would lose (unsent CW input, active Doppler
    /// tracking). Empty when nothing would be lost.
    fn quit_warnings(&self) -> Vec<String> {
        Vec::new()
    }
}
