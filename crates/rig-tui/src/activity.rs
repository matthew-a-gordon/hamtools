//! The Activity seam: what the host `App` needs from the thing that owns the body of the screen.
//!
//! An Activity does no I/O. Key events, ticks and rig replies go in; [`Command`]s (a closed
//! enum that `main` performs) and status text come out. The App hosts exactly one Activity in
//! the foreground; only that one is ticked. Activities are registered statically in this crate.

use std::collections::BTreeMap;
use std::time::Instant;

use chrono::{DateTime, Utc};
use hamtools_config::LicenseClass;
use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;

use crate::keys::KeyEntry;
use crate::layout::Panes;
use crate::logging::Logging;
use crate::panel::{LogDefaults, LogMode, Role};
use crate::rig::{Command, RigMode};
use crate::rigs::Capability;
use crate::softkeys::{Live, ResolvedKey};
use crate::tracker::ActivationView;

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
    /// The Contact form the host keeps. An Activity that [embeds](Activity::embeds_contact_form)
    /// it draws it in its body through this.
    pub form: FormPane<'a>,
    /// The active plan, its banner and the tracker, for the Activator screens (filled by the host
    /// only while one is in the foreground).
    pub activation: Option<&'a ActivationView>,
    /// Plans the operator could launch on this screen (filled for rendering and plan picking).
    pub plans: &'a [PlanChoice],
    /// The UTC time the strip shows.
    pub utc: DateTime<Utc>,
    /// The Observer position (latitude, longitude in degrees) from the Locator, if one is set.
    pub observer: Option<(f64, f64)>,
    pub license_class: Option<LicenseClass>,
    /// Contacts in the Log dated today (UTC), read each time. Empty unless the Activity says
    /// [`Activity::uses_log`].
    pub worked: &'a [Worked],
    /// Contacts logged this run.
    pub logged: usize,
    /// The receive frequency in Hz, only if the rig reported it within the last two seconds.
    pub freq: Option<u64>,
}

/// A stored plan offered in a picker; `index` is its place in the stored list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanChoice {
    pub index: usize,
    pub label: String,
}

/// A read-only view of the host's Contact form.
#[derive(Clone, Copy)]
pub struct FormPane<'a>(&'a Logging);

impl<'a> FormPane<'a> {
    pub(crate) fn new(logging: &'a Logging) -> Self {
        Self(logging)
    }

    /// Whether the form is open (it needs a rig reading to open).
    pub fn open(&self) -> bool {
        self.0.form_open()
    }

    /// Whether a key has edited the form since it opened.
    pub fn dirty(&self) -> bool {
        self.0.form_dirty()
    }

    /// The call, mode and transmit frequency typed or captured so far.
    pub fn call(&self) -> String {
        self.0
            .view()
            .map(|v| v.their_call.to_string())
            .unwrap_or_default()
    }

    pub fn mode(&self) -> String {
        self.0.form_mode()
    }

    pub fn hz(&self) -> Option<u64> {
        self.0.form_hz()
    }

    /// Draw the form compactly in `area`.
    pub fn render(&self, frame: &mut Frame, area: Rect, focused: bool) {
        self.0.render_embedded(frame, area, focused);
    }
}

impl std::fmt::Debug for FormPane<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormPane")
            .field("open", &self.open())
            .finish()
    }
}

/// Where an Activity wants a key that reached an embedded Contact form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormRoute {
    /// Not about the form: offer it to [`Activity::on_key`], then to the shared strip keys.
    Pass,
    /// Type it into the form.
    Form,
    /// Discard the half-typed Contact and reopen the form fresh.
    Discard,
    /// The Activity used it (a focus change); nothing more to do.
    Handled,
}

/// One Contact from today's Log, as much as "worked today" needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worked {
    /// Uppercase.
    pub call: String,
    /// ADIF band name, e.g. `20m`.
    pub band: String,
    /// ADIF mode, e.g. `CW`, `SSB`, `FT8`.
    pub mode: String,
}

/// Open the Contact form on this Call (and Their ref). Never tunes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogPrefill {
    pub call: String,
    pub their_ref: Option<String>,
}

/// Tune the receive frequency to `hz`, and the mode when one is mapped. The host applies its
/// gates (not connected, Doppler tracking). Never touches split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuneRequest {
    pub hz: u64,
    pub mode: Option<RigMode>,
    /// The name the source gave the mode, for the note when none is mapped.
    pub mode_name: String,
}

/// A request an Activity makes of the host after a key, fetched with [`Activity::take_request`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRequest {
    OpenLog(LogPrefill),
    Tune(TuneRequest),
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

    /// The Activity has just come to the foreground.
    fn on_enter(&mut self) {}

    /// Whether the Activity wants [`ActivityCtx::worked`] filled (it reads the Log).
    fn uses_log(&self) -> bool {
        false
    }

    /// A request the last key made of the host (open the Contact form, tune). Taking it clears it.
    fn take_request(&mut self) -> Option<HostRequest> {
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

    /// The active Activation plan changed. `role` is the Activator Role of the plan's Activity
    /// (`Role::General` when no plan is active) and `my_refs` its references, first primary.
    /// The Contact form's own references (`LogDefaults::my_refs`) come from here.
    fn set_plan(&mut self, _role: Role, _my_refs: &[String]) {}

    /// Whether this Activity draws the Contact form itself, always open, instead of the host
    /// showing it as a modal. The host then keeps the form open (when the rig has reported what
    /// a Contact needs) and asks [`Self::form_key`] where each key goes.
    fn embeds_contact_form(&self) -> bool {
        false
    }

    /// Route a key between the embedded form and the rest of the screen. Only asked when
    /// [`Self::embeds_contact_form`] is true and no modal is open.
    fn form_key(&mut self, _key: KeyEvent, _ctx: &ActivityCtx) -> FormRoute {
        FormRoute::Pass
    }

    /// One line for each thing quitting now would lose (unsent CW input, active Doppler
    /// tracking). Empty when nothing would be lost.
    fn quit_warnings(&self) -> Vec<String> {
        Vec::new()
    }
}
