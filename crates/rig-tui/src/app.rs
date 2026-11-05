//! The pure application state: key events and rig events in, commands out. No I/O; ticks carry
//! the time (and `set_utc` the wall clock), so every behaviour is testable without a rig or a
//! terminal.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, Datelike, Utc};
use hamtools_config::{Callsign, Grid, LicenseClass, Plan, PlanActivity, RigKind, RigProfile};
use hamtools_tui::modal::Modal;
use logbook::{Log, band_for_hz};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::activity::{
    Activity, ActivityCtx, FormRoute, HostEvent, HostRequest, LogPrefill, PlanChoice, TuneRequest,
    Worked,
};
use crate::bandplan::{BandAction, BandplanModal, View as BandView};
use crate::config_dialog::{ConfigDialog, Outcome};
use crate::export_modal::ExportModal;
use crate::general::General;
use crate::geocode::GeocodeResult;
use crate::keybindings::KeybindingsModal;
use crate::keys::{self, Action, Scope};
use crate::layout::{self, Fit, Panes};
use crate::logging::{Logging, Snapshot};
use crate::palette::{Palette, PaletteOutcome, PaletteRow, Target};
use crate::panel::{LogDefaults, LogMode, Role};
use crate::plans::{self, PlanRow, Plans, PlansModal, PlansOutcome};
use crate::refdata::ReferenceData;
use crate::rig::{Command, Feature, RigEvent, RigMode};
use crate::rigs::{Capability, RigSlot, Switcher};
use crate::softkeys::{Exchange, Layers, NamedSet, Resolved, resolve};
use crate::style::{dim, red};
use crate::tracker::{self, ActivationView, PlanBanner, Program, Tracker};

mod rig_switch;

/// How long to wait between attempts to reach `rigctld`. A starting value, untested on hardware.
pub const RECONNECT_EVERY: Duration = Duration::from_secs(3);
/// How often frequency, mode, VFO, split and PTT are read (about twice a second). Untested on
/// hardware.
pub const STATE_POLL_EVERY: Duration = Duration::from_millis(500);
/// How often the S-meter is read (about four times a second). Untested on hardware.
pub const SMETER_POLL_EVERY: Duration = Duration::from_millis(250);
/// A reading older than this shows as a dash. A starting value, untested on hardware.
pub const STALE_AFTER: Duration = Duration::from_secs(2);

/// How often an Activator screen re-reads the Log for its tracker.
const ACTIVATION_EVERY: Duration = Duration::from_secs(2);

/// Tuning steps in Hz, smallest first.
pub(crate) const STEPS: [u64; 6] = [1, 10, 100, 1_000, 10_000, 100_000];
pub(crate) const DEFAULT_STEP: usize = 3;

/// The order `m` and `M` walk through.
const MODE_CYCLE: [RigMode; 8] = [
    RigMode::Lsb,
    RigMode::Usb,
    RigMode::Cw,
    RigMode::Cwr,
    RigMode::Am,
    RigMode::Fm,
    RigMode::Rtty,
    RigMode::RttyR,
];

const DEFAULT_RIG_ADDR: &str = "127.0.0.1:4532";

/// Which widget gets plain keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The rig strip: plain keys are commands.
    Strip,
    /// The active Mode panel's input line.
    Panel,
}

/// A modal that gets every key before anything else (the log form, session list and config
/// dialog plug in here).
pub trait Overlay {
    fn on_key(&mut self, key: KeyEvent) -> OverlayAction;

    fn render(&self, frame: &mut Frame, area: Rect);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayAction {
    Keep,
    Close,
}

/// A value the rig reported and when the app heard it.
#[derive(Debug, Clone)]
pub(crate) struct Reading<T> {
    pub(crate) value: T,
    pub(crate) at: Instant,
}

/// A frequency being typed on the strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Entry {
    Freq(String),
    SplitFreq(String),
}

impl Entry {
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Freq(t) | Self::SplitFreq(t) => t,
        }
    }

    fn text_mut(&mut self) -> &mut String {
        match self {
            Self::Freq(t) | Self::SplitFreq(t) => t,
        }
    }
}

/// The Activity context, built from disjoint fields so the Activity can be borrowed mutably
/// alongside it.
macro_rules! ctx {
    ($self:ident) => {
        ctx!($self, &[])
    };
    ($self:ident, $recent:expr) => {
        ctx!($self, $recent, crate::layout::Panes::MINIMAL)
    };
    ($self:ident, $recent:expr, $panes:expr) => {
        ctx!($self, $recent, $panes, &[])
    };
    ($self:ident, $recent:expr, $panes:expr, $worked:expr) => {
        ActivityCtx {
            connected: $self.connected,
            live: $self.exchange.live($self.callsign.as_deref()),
            soft_keys: &$self.resolved.keys,
            rig_mode: $self.mode.as_ref().map(|r| &r.value.0),
            fresh_mode: $self
                .mode
                .as_ref()
                .filter(|r| $self.now.saturating_duration_since(r.at) <= STALE_AFTER)
                .map(|r| &r.value.0),
            recent: $recent,
            panes: $panes,
            form: crate::activity::FormPane::new(&$self.logging),
            activation: $self.activation.as_ref(),
            plans: &[],
            utc: $self.clock(),
            observer: $self.locator.as_deref().and_then(crate::geo::grid_center),
            license_class: $self.license_class,
            worked: $worked,
            logged: $self.logging.count(),
            freq: $self
                .freq
                .as_ref()
                .filter(|r| $self.now.saturating_duration_since(r.at) <= STALE_AFTER)
                .map(|r| r.value),
        }
    };
}

/// What a cached read of the Log stands for: the file, its modified time and size, and the date.
#[derive(Clone, PartialEq, Eq)]
struct WorkedKey {
    path: PathBuf,
    modified: Option<SystemTime>,
    len: u64,
    day: chrono::NaiveDate,
}

pub struct App {
    /// Every hosted Activity keeps its in-memory state; exactly one is in the foreground.
    activities: Vec<Box<dyn Activity>>,
    foreground: usize,
    /// The configured rigs; the readings below belong to `rigs[active]`.
    rigs: Vec<RigSlot>,
    active: usize,
    /// `rig.default` as read from the config (a name), for the config dialog.
    rig_default: Option<String>,
    /// No profile was configured: `rigs` holds one stand-in named after its address.
    implicit_rig: bool,
    switcher: Option<Switcher>,
    overlay: Option<Box<dyn Overlay>>,
    /// The quit confirmation, with a line for each thing quitting would lose. It sits above
    /// every other overlay, which stay untouched underneath.
    quit_confirm: Option<Vec<String>>,
    /// The Ctrl-G go-to palette and the Ctrl-K keybindings modal.
    palette: Option<Palette>,
    keybindings: Option<KeybindingsModal>,
    /// The Activation plans: stored list, the one active this session, and their modal.
    plans: Plans,
    plans_modal: Option<PlansModal>,
    /// The export preview, opened from the plan manager.
    export_modal: Option<ExportModal>,
    export_dir: Option<PathBuf>,
    /// The Activator screens' banner, tracker and warnings, derived from the active plan and the
    /// Log; rebuilt when stale or every few seconds, never fetched.
    activation: Option<ActivationView>,
    activation_at: Option<Instant>,
    activation_stale: bool,
    refdata: Option<Arc<ReferenceData>>,
    last_resolve: Option<Instant>,
    connected: bool,
    now: Instant,
    utc: Option<SystemTime>,
    freq: Option<Reading<u64>>,
    mode: Option<Reading<(RigMode, i64)>>,
    vfo: Option<Reading<String>>,
    split: Option<Reading<(bool, String)>>,
    split_freq: Option<Reading<u64>>,
    ptt: Option<Reading<bool>>,
    strength: Option<Reading<i32>>,
    unsupported: BTreeSet<Feature>,
    step: usize,
    entry: Option<Entry>,
    /// Leaving an Activity stopped Doppler tracking; the strip says so until Satellite is back.
    doppler_stopped: bool,
    /// Today's Contacts as last read, valid while the Log file and the date are unchanged.
    worked_cache: RefCell<Option<(WorkedKey, Vec<Worked>)>>,
    status: Option<String>,
    /// The status came from a failing poll, so the next good poll clears it.
    poll_failed: bool,
    callsign: Option<String>,
    logging: Logging,
    /// The General Soft key set (`[macros]`).
    soft_keys: BTreeMap<u8, String>,
    softkey_sets: BTreeMap<String, BTreeMap<u8, String>>,
    /// The user set for the current Activity; `None` is General.
    activity_soft_keys: Option<String>,
    plan_soft_keys: Option<String>,
    plan_soft_overrides: BTreeMap<u8, String>,
    exchange: Exchange,
    /// The exchange as it was before the Contact form began publishing into it.
    exchange_saved: Option<Exchange>,
    /// The layered Soft keys, recomputed whenever a layer changes.
    resolved: Resolved,
    locator: Option<String>,
    log_path: Option<PathBuf>,
    default_log_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    config: Option<ConfigDialog>,
    /// The Ctrl-B band plan modal, when open.
    bandplan: Option<BandplanModal>,
    license_class: Option<LicenseClass>,
    last_connect: Option<Instant>,
    last_poll: Option<Instant>,
    last_smeter: Option<Instant>,
    /// The terminal size, once the loop has reported it.
    size: Option<(u16, u16)>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// An app with no rig yet: disconnected, strip focused, the CW panel registered.
    pub fn new() -> Self {
        Self {
            activities: vec![Box::new(General::new())],
            foreground: 0,
            rigs: vec![RigSlot::from_profile(
                &RigProfile {
                    name: DEFAULT_RIG_ADDR.to_string(),
                    addr: DEFAULT_RIG_ADDR.to_string(),
                    kind: RigKind::Kx3,
                    cw: None,
                    split: None,
                    sat_mode: None,
                },
                DEFAULT_STEP,
            )],
            active: 0,
            rig_default: None,
            implicit_rig: true,
            switcher: None,
            overlay: None,
            quit_confirm: None,
            palette: None,
            keybindings: None,
            plans: Plans::none(),
            plans_modal: None,
            export_modal: None,
            export_dir: None,
            activation: None,
            activation_at: None,
            activation_stale: true,
            refdata: None,
            last_resolve: None,
            connected: false,
            // Only the starting point for staleness; every tick replaces it.
            now: Instant::now(),
            utc: None,
            freq: None,
            mode: None,
            vfo: None,
            split: None,
            split_freq: None,
            ptt: None,
            strength: None,
            unsupported: BTreeSet::new(),
            step: DEFAULT_STEP,
            entry: None,
            doppler_stopped: false,
            worked_cache: RefCell::new(None),
            status: None,
            poll_failed: false,
            callsign: None,
            logging: Logging::new(),
            soft_keys: BTreeMap::new(),
            softkey_sets: BTreeMap::new(),
            activity_soft_keys: None,
            plan_soft_keys: None,
            plan_soft_overrides: BTreeMap::new(),
            exchange: Exchange::default(),
            exchange_saved: None,
            resolved: resolve(&Layers::default()),
            locator: None,
            log_path: None,
            default_log_path: None,
            config_path: None,
            config: None,
            bandplan: None,
            license_class: None,
            last_connect: None,
            last_poll: None,
            last_smeter: None,
            size: None,
        }
    }

    /// An app whose `{MYCALL}` is `callsign`.
    pub fn with_callsign(callsign: Option<String>) -> Self {
        Self {
            callsign,
            ..Self::new()
        }
    }

    /// Bind F1..F6 to canned messages: the General Soft key set (text is uppercase and may hold
    /// variables such as `{MYCALL}`).
    pub fn with_soft_keys(mut self, soft_keys: BTreeMap<u8, String>) -> Self {
        self.soft_keys = soft_keys;
        self.refresh_soft_keys();
        self
    }

    /// The named user sets from `[softkeys.<name>]`.
    pub fn with_softkey_sets(mut self, sets: BTreeMap<String, BTreeMap<u8, String>>) -> Self {
        self.softkey_sets = sets;
        self.refresh_soft_keys();
        self
    }

    /// Use the user set `name` for the Activity (`None`: General, the `[macros]` set). A name
    /// with no such set falls back to the lower layers and is flagged in the status line.
    pub fn set_activity_soft_keys(&mut self, name: Option<String>) {
        self.activity_soft_keys = name;
        self.flag_missing_soft_key_sets();
    }

    /// Use a plan's Soft keys: its named set and its single-key overrides. A missing set falls
    /// back to the lower layers and is flagged in the status line; it never blocks.
    pub fn set_plan_soft_keys(&mut self, set: Option<String>, overrides: BTreeMap<u8, String>) {
        self.plan_soft_keys = set;
        self.plan_soft_overrides = overrides;
        self.flag_missing_soft_key_sets();
    }

    /// The values `{THEIRCALL}`, `{MYREF}` and the rest expand from (`{MYCALL}` is the callsign).
    pub fn set_exchange(&mut self, exchange: Exchange) {
        self.exchange = exchange;
    }

    /// The Soft keys after layering, as the bar shows them and a key press sends them.
    pub fn resolved_soft_keys(&self) -> &Resolved {
        &self.resolved
    }

    fn compute_soft_keys(&self) -> Resolved {
        let lookup = |name: &String| NamedSet {
            name: name.clone(),
            keys: self.softkey_sets.get(name).cloned(),
        };
        resolve(&Layers {
            plan_override: self.plan_soft_overrides.clone(),
            plan_set: self.plan_soft_keys.as_ref().map(lookup),
            user_set: Some(match &self.activity_soft_keys {
                Some(name) => lookup(name),
                None => NamedSet {
                    name: "General".to_string(),
                    keys: Some(self.soft_keys.clone()),
                },
            }),
        })
    }

    fn refresh_soft_keys(&mut self) {
        self.resolved = self.compute_soft_keys();
    }

    fn flag_missing_soft_key_sets(&mut self) {
        self.refresh_soft_keys();
        let missing = self.resolved.missing_sets.clone();
        if !missing.is_empty() {
            self.status = Some(format!(
                "Soft key set {} not found: using the layers below",
                missing.join(", ")
            ));
        }
    }

    /// Where the Log lives when the config sets no `[logbook] path`. `None`: logging is off.
    pub fn with_default_log_path(mut self, path: Option<PathBuf>) -> Self {
        self.default_log_path = path;
        self
    }

    /// How many Contacts were logged this run.
    pub fn logged_count(&self) -> usize {
        self.logging.count()
    }

    /// The config file the dialog edits (`None`: no location, so the dialog is read-only).
    pub fn with_config_path(mut self, path: Option<PathBuf>) -> Self {
        self.plans = path
            .as_deref()
            .map_or_else(Plans::none, |p| Plans::load(&plans::plans_path(p)));
        self.config_path = path;
        self
    }

    /// The park and summit data plans snapshot from. Without it plans still save, unresolved.
    pub fn with_reference_data(mut self, data: Arc<ReferenceData>) -> Self {
        self.refdata = Some(data);
        self
    }

    /// The plan launched from the palette this session, if any (never stored; a restart returns
    /// it to planned).
    pub fn active_plan(&self) -> Option<&Plan> {
        self.plans.active()
    }

    /// Why plans cannot be stored (no config directory, or `plans.toml` does not parse).
    pub fn plans_error(&self) -> Option<&str> {
        self.plans.error()
    }

    /// The Locator in use (6 characters, uppercase), if any.
    pub fn with_locator(mut self, locator: Option<String>) -> Self {
        self.locator = locator;
        self
    }

    /// The operator's license class (`station.license_class`), for the band plan's privilege row.
    pub fn with_license_class(mut self, class: Option<LicenseClass>) -> Self {
        self.license_class = class;
        self
    }

    /// Where export files go (`[export] dir`); `None`: exporting is refused with a message.
    pub fn with_export_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.export_dir = dir;
        self
    }

    pub fn with_log_path(mut self, log_path: Option<PathBuf>) -> Self {
        self.log_path = log_path;
        self
    }

    /// The `rigctld` address of the Active rig (a command-line override, or the stand-in rig
    /// when no profile is configured).
    pub fn with_rig_addr(mut self, addr: impl Into<String>) -> Self {
        let slot = &mut self.rigs[self.active];
        slot.addr = addr.into();
        if self.implicit_rig {
            slot.name = slot.addr.clone();
        }
        self
    }

    /// Use these rig profiles (in config order); start on `default` or else the first. An empty
    /// list keeps the stand-in rig.
    pub fn with_rigs(mut self, profiles: &[RigProfile], default: Option<&str>) -> Self {
        if profiles.is_empty() {
            return self;
        }
        self.rigs = profiles
            .iter()
            .map(|p| RigSlot::from_profile(p, DEFAULT_STEP))
            .collect();
        self.active = default
            .and_then(|name| profiles.iter().position(|p| p.name == name))
            .unwrap_or(0);
        self.rig_default = default.map(str::to_string);
        self.implicit_rig = false;
        self
    }

    /// Start on the rig profile called `name` (`--rig NAME`). Unknown names are an error that
    /// lists the configured profiles.
    pub fn with_active_rig(mut self, name: &str) -> Result<Self, String> {
        let found = (!self.implicit_rig)
            .then(|| self.rigs.iter().position(|r| r.name == name))
            .flatten();
        match found {
            Some(index) => {
                self.active = index;
                Ok(self)
            }
            None if self.implicit_rig => Err(format!(
                "no rig profile {name:?}: the config defines no [rig.NAME] profiles"
            )),
            None => {
                let names: Vec<&str> = self.rigs.iter().map(|r| r.name.as_str()).collect();
                Err(format!(
                    "no rig profile {name:?} (configured: {})",
                    names.join(", ")
                ))
            }
        }
    }

    pub fn locator(&self) -> Option<&str> {
        self.locator.as_deref()
    }

    /// The Log path in use; `None` means the default.
    pub fn log_path(&self) -> Option<&Path> {
        self.log_path.as_deref()
    }

    /// Host another Activity (it stays in the background until [`App::set_foreground`]).
    /// Returns its index.
    pub fn host_activity(&mut self, activity: Box<dyn Activity>) -> usize {
        self.activities.push(activity);
        self.activities.len() - 1
    }

    /// Bring the Activity at `index` to the foreground. Out of range is ignored.
    pub fn set_foreground(&mut self, index: usize) {
        if index >= self.activities.len() {
            return;
        }
        if let Some(why) = self.activity_unavailable(index) {
            self.status = Some(why);
            return;
        }
        // An embedded Contact form belongs to its screen: leaving drops it (the Log keeps every
        // saved Contact; only a half-typed one is lost, and the status says so).
        let mut dropped = None;
        if self.embedded() && !self.activities[index].embeds_contact_form() {
            if self.logging.form_dirty() {
                dropped = Some("half-typed Contact discarded".to_string());
            }
            self.logging.discard();
            self.sync_form_exchange();
        }
        let left = (index != self.foreground).then(|| self.activities[self.foreground].on_leave());
        self.foreground = index;
        self.entry = None;
        let left = left.flatten();
        if left.is_some() {
            self.doppler_stopped = true;
        }
        if self.activities[index].name() == "Satellite" {
            self.doppler_stopped = false;
        }
        if let Some(note) = dropped.or(left) {
            self.status = Some(note);
        }
        self.activities[index].on_enter();
        if let Some(note) = self.activities[index].enter_note() {
            self.status = Some(note);
        }
        self.activation_stale = true;
        self.sync_embedded();
    }

    /// Hand a piece of outside news (a finished download) to every hosted Activity.
    pub fn on_host_event(&mut self, event: HostEvent) {
        for activity in &mut self.activities {
            if let Some(status) = activity.on_event(&event) {
                self.status = Some(status);
            }
        }
    }

    /// Whether the foreground Activity draws the Contact form itself.
    fn embedded(&self) -> bool {
        self.activities[self.foreground].embeds_contact_form()
    }

    /// The Contact form is open as a modal (an embedded one is drawn by its Activity).
    fn form_modal_open(&self) -> bool {
        self.logging.form_open() && !self.embedded()
    }

    /// A Logging modal has the keyboard: the form as a modal, or the session list.
    fn logging_modal_open(&self) -> bool {
        self.form_modal_open() || self.logging.list_open()
    }

    /// Stored plans of the program `name` that are not done, soonest first, for a picker.
    fn plan_choices(&self, name: &str) -> Vec<PlanChoice> {
        self.plans
            .sorted()
            .into_iter()
            .filter(|p| !p.done && plans::activity_label(p.activity) == name)
            .filter_map(|p| {
                let index = self.plans.list().iter().position(|q| q.id == p.id)?;
                Some(PlanChoice {
                    index,
                    label: format!("{} {}", plans::refs_label(p), p.date),
                })
            })
            .collect()
    }

    /// Keep an embedded Contact form open and current, and the Activation view fresh. Cheap when
    /// nothing changed; touches the Log only to open the form or rebuild the view.
    fn sync_embedded(&mut self) {
        if !self.embedded() {
            return;
        }
        if self.logging.form_open() {
            if let Some(snapshot) = self.rig_snapshot() {
                self.logging.refresh(snapshot);
            }
        } else {
            match self.log_snapshot() {
                Ok(snapshot) => {
                    self.logging.set_wait(None);
                    self.logging.open_form(snapshot);
                }
                Err(why) => self.logging.set_wait(Some(why)),
            }
        }
        self.sync_form_exchange();
        let due = self
            .activation_at
            .is_none_or(|at| self.now.saturating_duration_since(at) >= ACTIVATION_EVERY);
        if self.activation_stale || due {
            self.refresh_activation();
        }
    }

    /// Rebuild the banner, tracker and warnings from the active plan and the Log.
    fn refresh_activation(&mut self) {
        self.activation_stale = false;
        self.activation_at = Some(self.now);
        let name = self.activities[self.foreground].name();
        let program = match name {
            "POTA" => Program::Pota,
            "SOTA" => Program::Sota,
            _ => {
                self.activation = None;
                return;
            }
        };
        let Some(plan) = self
            .plans
            .active()
            .filter(|p| plans::activity_label(p.activity) == name)
            .cloned()
        else {
            self.activation = None;
            return;
        };
        let today = self.clock().date_naive();
        let mut warnings = Vec::new();
        let tracker = match self
            .current_log()
            .and_then(|l| l.read().map_err(|e| e.to_string()))
        {
            Ok(records) => {
                let began = plan
                    .date
                    .parse::<chrono::NaiveDate>()
                    .is_ok_and(|d| tracker::any_on(program, &plan.references, d, &records));
                warnings.extend(tracker::date_warning(&plan.date, today, began));
                Some(Tracker::query(program, &plan.references, today, &records))
            }
            Err(why) => {
                warnings.push(format!("tracker unavailable: {why}"));
                None
            }
        };
        let snapshot = plan
            .snapshot
            .as_ref()
            .filter(|s| Some(s.reference.as_str()) == plan.primary());
        let theirs = snapshot.and_then(|s| {
            s.lat
                .zip(s.lon)
                .or_else(|| s.grid.as_deref().and_then(crate::geo::grid_center))
        });
        let mine = self.locator.as_deref().and_then(crate::geo::grid_center);
        self.activation = Some(ActivationView {
            banner: Some(PlanBanner {
                program,
                references: plan.references.clone(),
                name: snapshot.map(|s| s.name.clone()),
                location: snapshot.and_then(|s| s.location.clone()),
                grid: snapshot.and_then(|s| s.grid.clone()),
                distance_km: mine.zip(theirs).map(|(a, b)| crate::geo::distance_km(a, b)),
                alt_m: snapshot.and_then(|s| s.alt_m),
                points: snapshot.and_then(|s| s.points),
                date: plan.date.clone(),
            }),
            tracker,
            warnings,
        });
    }

    /// The foreground Activity's name, as the shared strip shows it.
    /// Whether tracking was stopped by leaving Satellite and has not been resumed (the strip says so).
    pub fn doppler_stopped(&self) -> bool {
        self.doppler_stopped
    }

    pub fn activity_name(&self) -> &'static str {
        self.activities[self.foreground].name()
    }

    /// The answer to a [`Command::Lookup`] (run by `main` on a short-lived thread).
    pub fn on_geocode(&mut self, result: GeocodeResult) {
        if let Some(dialog) = &mut self.config {
            dialog.on_geocode(result);
        }
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// Message shown in the status line, e.g. the last rig error. `None` clears it.
    pub fn set_status(&mut self, status: Option<String>) {
        self.status = status;
    }

    pub fn connected(&self) -> bool {
        self.connected
    }

    /// The UTC wall clock, as last given to [`App::set_utc`]. The strip shows it and the log form
    /// will stamp Contacts with it, so what is seen is what is logged.
    pub fn utc(&self) -> Option<SystemTime> {
        self.utc
    }

    pub fn set_utc(&mut self, now: SystemTime) {
        self.utc = Some(now);
    }

    fn fresh<'a, T>(&self, reading: &'a Option<Reading<T>>) -> Option<&'a T> {
        reading
            .as_ref()
            .filter(|r| self.now.saturating_duration_since(r.at) <= STALE_AFTER)
            .map(|r| &r.value)
    }

    fn stamp<T>(&self, value: T) -> Option<Reading<T>> {
        Some(Reading {
            value,
            at: self.now,
        })
    }

    /// The receive frequency in Hz, if the rig reported it within the last two seconds.
    pub fn frequency(&self) -> Option<u64> {
        self.fresh(&self.freq).copied()
    }

    /// The rig's mode, if reported within the last two seconds.
    pub fn rig_mode(&self) -> Option<&RigMode> {
        self.fresh(&self.mode).map(|(mode, _)| mode)
    }

    pub fn passband(&self) -> Option<i64> {
        self.fresh(&self.mode).map(|(_, passband)| *passband)
    }

    /// The selected VFO's hamlib name (e.g. `VFOA`), if current.
    pub fn vfo(&self) -> Option<&str> {
        self.fresh(&self.vfo).map(String::as_str)
    }

    /// Whether split is on and the transmit VFO, if current.
    pub fn split(&self) -> Option<(bool, &str)> {
        self.fresh(&self.split).map(|(on, vfo)| (*on, vfo.as_str()))
    }

    pub fn ptt(&self) -> Option<bool> {
        self.fresh(&self.ptt).copied()
    }

    /// S-meter in dB relative to S9, if current.
    pub fn strength(&self) -> Option<i32> {
        self.fresh(&self.strength).copied()
    }

    pub fn tuning_step(&self) -> u64 {
        STEPS[self.step]
    }

    pub fn is_unsupported(&self, feature: Feature) -> bool {
        self.unsupported.contains(&feature)
    }

    pub(crate) fn entry(&self) -> Option<&Entry> {
        self.entry.as_ref()
    }

    /// The frequency the rig transmits on, for the strip and for the log form's `FREQ`. This is
    /// the one seam for that question.
    ///
    /// With split off it is the receive frequency. With split on it is what the rig reported for
    /// the split transmit frequency, or `None` if it reported none (or not recently): never the
    /// receive frequency, which would log the wrong place. Whether the KX3's reported split
    /// frequency is really the transmit VFO's is **untested on hardware**; see
    /// `Rig::read_tx_frequency`.
    pub fn transmit_frequency(&self) -> Option<u64> {
        match self.split() {
            Some((true, _)) => self.fresh(&self.split_freq).copied(),
            Some((false, _)) => self.frequency(),
            // Split not readable at all (unsupported): the rig transmits where it receives.
            None if self.is_unsupported(Feature::Split) => self.frequency(),
            None => None,
        }
    }

    pub fn callsign(&self) -> Option<&str> {
        self.callsign.as_deref()
    }

    pub fn soft_keys(&self) -> &BTreeMap<u8, String> {
        &self.soft_keys
    }

    pub fn has_overlay(&self) -> bool {
        self.overlay.is_some()
            || self.config.is_some()
            || self.switcher_open()
            || self.bandplan.is_some()
            || self.logging_modal_open()
            || self.quit_confirm.is_some()
            || self.palette.is_some()
            || self.plans_modal.is_some()
            || self.export_modal.is_some()
            || self.keybindings.is_some()
    }

    /// Why tuning from the band plan is refused right now (the foreground Activity is
    /// controlling the dial, as Satellite does while tracking), if it is.
    fn tuning_blocked(&self) -> Option<&'static str> {
        self.activities[self.foreground].tuning_blocked()
    }

    /// Act on what a band plan key asked for. Tuning only: none of these keys the transmitter.
    fn on_bandplan_action(&mut self, action: BandAction) -> Vec<Command> {
        match action {
            BandAction::Stay => return Vec::new(),
            BandAction::Close => {
                self.bandplan = None;
                return Vec::new();
            }
            BandAction::NoMode => {
                self.status = Some("no usual mode here".to_string());
                return Vec::new();
            }
            _ => {}
        }
        if let Some(why) = self.tuning_blocked() {
            self.status = Some(format!("not while {why}"));
            return Vec::new();
        }
        if !self.ready() {
            return Vec::new();
        }
        let hz = match action {
            BandAction::SetRx(hz) => hz,
            BandAction::SetTx(hz) => {
                if !self.require(Feature::Split)
                    || !self.require(Feature::SplitFreq)
                    || !self.require_split_on()
                {
                    return Vec::new();
                }
                hz
            }
            BandAction::SetMode(mode) => {
                self.status = None;
                // Already in the mode: send nothing, so the passband is never changed by surprise.
                if self.last_mode() == Some(&mode) {
                    return Vec::new();
                }
                return vec![Command::SetMode { mode, passband: 0 }];
            }
            BandAction::Stay | BandAction::Close | BandAction::NoMode => return Vec::new(),
        };
        // Outside the license class is advice, never a gate.
        self.status = self.license_class.and_then(|class| {
            (!hamtools_config::bandplan::BandPlan::embedded().in_privilege(hz, class)).then(|| {
                format!(
                    "{} is outside {class} privileges (advisory only)",
                    crate::ui::format_hz(hz)
                )
            })
        });
        match action {
            BandAction::SetTx(_) => vec![Command::SetSplitFreq(hz)],
            _ => vec![Command::SetFreq(hz)],
        }
    }

    pub(crate) fn quit_confirm_open(&self) -> bool {
        self.quit_confirm.is_some()
    }

    /// Check every hosted Activity's own keys against the reserved plain-key table.
    pub fn check_activity_keys(&self) -> Result<(), String> {
        self.activities
            .iter()
            .try_for_each(|a| keys::validate_activity(a.name(), &a.keys()))
    }

    /// Ask before quitting, naming each thing that would be lost.
    fn open_quit_confirm(&mut self) {
        let mut lines = Vec::new();
        if self.logging.form_dirty() {
            lines.push("Unsaved Contact text in the log form will be lost.".to_string());
        }
        for activity in &self.activities {
            lines.extend(activity.quit_warnings());
        }
        self.quit_confirm = Some(lines);
    }

    fn on_quit_confirm_key(&mut self, key: KeyEvent) -> Vec<Command> {
        let ctrl_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('y') if key.modifiers.is_empty() => vec![Command::Quit],
            _ if ctrl_c => vec![Command::Quit],
            KeyCode::Char('n') | KeyCode::Esc | KeyCode::Enter => {
                self.quit_confirm = None;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// The palette: upcoming Activation plans first (soonest first, overdue ones flagged), then
    /// one row per hosted Activity, then the way into the plan manager.
    fn palette_rows(&self) -> Vec<PaletteRow> {
        let ctx = ctx!(self);
        let today = self.today();
        let now_unix = self.now_unix();
        let mut rows: Vec<PaletteRow> = self
            .plans
            .sorted()
            .into_iter()
            .filter(|p| !p.done)
            .filter_map(|p| {
                let index = self.plans.list().iter().position(|q| q.id == p.id)?;
                let name = plans::display_name(p, self.refdata.as_deref(), now_unix);
                Some(PaletteRow {
                    target: Target::Plan(index),
                    name: format!(
                        "{} {}",
                        plans::activity_label(p.activity),
                        plans::refs_label(p)
                    ),
                    state: format!(
                        "{} {}{}",
                        p.date,
                        self.plans.status(p, &today).label(),
                        name.map(|n| format!(" {n}")).unwrap_or_default()
                    ),
                    unavailable: None,
                })
            })
            .collect();
        rows.extend(self.activities.iter().enumerate().map(|(i, a)| PaletteRow {
            target: Target::Activity(i),
            name: a.name().to_string(),
            state: a.state_line(&ctx),
            unavailable: a.unavailable().or_else(|| self.activity_unavailable(i)),
        }));
        rows.push(PaletteRow {
            target: Target::ManagePlans,
            name: "Plans".to_string(),
            state: format!(
                "{} stored: new, edit, duplicate, delete",
                self.plans.list().len()
            ),
            unavailable: None,
        });
        rows
    }

    /// Today's UTC date as `YYYY-MM-DD`.
    fn today(&self) -> String {
        let d = self.clock();
        format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
    }

    fn now_unix(&self) -> u64 {
        u64::try_from(self.clock().timestamp()).unwrap_or(0)
    }

    fn plan_rows(&self) -> Vec<PlanRow> {
        let today = self.today();
        let now_unix = self.now_unix();
        self.plans
            .sorted()
            .into_iter()
            .map(|p| PlanRow {
                plan: p.clone(),
                status: self.plans.status(p, &today),
                name: plans::display_name(p, self.refdata.as_deref(), now_unix),
            })
            .collect()
    }

    fn open_plans(&mut self) {
        self.plans_modal = Some(PlansModal::new(
            self.plan_rows(),
            self.plans.error().map(str::to_string),
            self.today(),
        ));
    }

    fn refresh_plans_modal(&mut self) {
        let (rows, blocked) = (self.plan_rows(), self.plans.error().map(str::to_string));
        if let Some(modal) = &mut self.plans_modal {
            modal.set_rows(rows, blocked);
        }
    }

    /// Stop using the active plan's reference and Soft keys (it was finished or deleted).
    fn deactivate_plan(&mut self) {
        self.exchange.my_ref = None;
        self.set_plan_soft_keys(None, BTreeMap::new());
        self.tell_activities_plan(None);
    }

    fn tell_activities_plan(&mut self, plan: Option<&Plan>) {
        let (role, refs) = match plan {
            Some(p) => (
                match p.activity {
                    PlanActivity::Sota => Role::SotaActivator,
                    PlanActivity::Pota => Role::PotaActivator,
                },
                p.references.clone(),
            ),
            None => (Role::General, Vec::new()),
        };
        for a in &mut self.activities {
            a.set_plan(role, &refs);
        }
        self.activation_stale = true;
    }

    /// Start the plan at `index` of the stored list: it becomes the active plan, its reference is
    /// my reference, its Soft key set loads, and its Activity comes to the foreground in the
    /// Activator Role. A missing Soft key set is flagged and never stops this.
    fn launch_plan(&mut self, index: usize) {
        let Some(plan) = self.plans.list().get(index).cloned() else {
            return;
        };
        self.launch_plan_quietly(index);
        let name = plans::activity_label(plan.activity);
        let primary = plan.primary().unwrap_or("");
        let position = self.activities.iter().position(|a| a.name() == name);
        if let Some(position) = position {
            self.set_foreground(position);
        }
        // A flag about a missing Soft key set stays on the status line.
        if self.resolved.missing_sets.is_empty() {
            self.status = Some(match position {
                Some(_) => format!("Plan {name} {primary} launched"),
                None => format!(
                    "Plan {name} {primary} is active; the {name} Activity is not available yet"
                ),
            });
        }
    }

    fn on_plans_key(&mut self, key: KeyEvent) {
        let Some(modal) = &mut self.plans_modal else {
            return;
        };
        match modal.on_key(key) {
            PlansOutcome::Stay => {}
            PlansOutcome::Close => self.plans_modal = None,
            PlansOutcome::Save(plan) => {
                let mut plan = *plan;
                plan.snapshot =
                    plans::snapshot_for(&plan, self.refdata.as_deref(), self.now_unix());
                let id = plan.id.clone();
                let was_active = self.plans.active().is_some_and(|a| a.id == id);
                match self.plans.upsert(plan) {
                    Ok(()) => {
                        if was_active {
                            self.deactivate_plan();
                            if let Some(i) = self.plans.list().iter().position(|p| p.id == id) {
                                self.launch_plan_quietly(i);
                            }
                        }
                        self.refresh_plans_modal();
                        if let Some(modal) = &mut self.plans_modal {
                            modal.show_list();
                            modal.select(&id);
                        }
                    }
                    Err(why) => self.plans_notice(why),
                }
            }
            PlansOutcome::Delete(id) => {
                let was_active = self.plans.active().is_some_and(|a| a.id == id);
                match self.plans.delete(&id) {
                    Ok(()) => {
                        if was_active {
                            self.deactivate_plan();
                        }
                        self.refresh_plans_modal();
                    }
                    Err(why) => self.plans_notice(why),
                }
            }
            PlansOutcome::ToggleDone(id, done) => {
                let was_active = self.plans.active().is_some_and(|a| a.id == id);
                match self.plans.set_done(&id, done) {
                    Ok(()) => {
                        if was_active && done {
                            self.deactivate_plan();
                        }
                        self.refresh_plans_modal();
                    }
                    Err(why) => self.plans_notice(why),
                }
            }
            PlansOutcome::Export(id) => self.open_export(&id),
            PlansOutcome::Reload => {
                let path = self.config_path.as_deref().map(plans::plans_path);
                self.plans.reload(path.as_deref());
                self.refresh_plans_modal();
                if let Some(modal) = &mut self.plans_modal {
                    modal.show_list();
                }
            }
        }
    }

    /// Open the export preview for plan `id`: read the Log once and resolve each park's cached
    /// location. Writes nothing.
    fn open_export(&mut self, id: &str) {
        let Some(plan) = self.plans.list().iter().find(|p| p.id == id).cloned() else {
            return;
        };
        let Some(operator) = self.callsign.clone() else {
            return self.plans_notice("no callsign: set `callsign` in the config file".into());
        };
        let records = match self
            .current_log()
            .and_then(|l| l.read().map_err(|e| e.to_string()))
        {
            Ok(r) => r,
            Err(why) => return self.plans_notice(format!("cannot read the Log: {why}")),
        };
        let now = self.now_unix();
        let mut locations = BTreeMap::new();
        for r in &plan.references {
            let from_snapshot = plan
                .snapshot
                .as_ref()
                .filter(|s| s.reference.eq_ignore_ascii_case(r))
                .and_then(|s| s.location.clone());
            let found = from_snapshot.or_else(|| match self.refdata.as_deref()?.lookup(r, now) {
                crate::refdata::Lookup::Found(hit) => hit.entry.location,
                crate::refdata::Lookup::NotCached => None,
            });
            if let Some(loc) = found {
                locations.insert(r.to_ascii_uppercase(), loc);
            }
        }
        self.export_modal = Some(ExportModal::new(
            plan,
            operator,
            records,
            locations,
            self.export_dir.clone(),
        ));
    }

    /// Re-apply an edited active plan's reference and Soft keys without moving the screen.
    fn launch_plan_quietly(&mut self, index: usize) {
        let Some(plan) = self.plans.list().get(index).cloned() else {
            return;
        };
        self.plans.set_active(Some(plan.id.clone()));
        self.exchange.my_ref = plan.primary().map(str::to_string);
        self.set_plan_soft_keys(plan.softkey_set.clone(), plan.softkeys.clone());
        self.tell_activities_plan(Some(&plan));
    }

    fn plans_notice(&mut self, why: String) {
        if let Some(modal) = &mut self.plans_modal {
            modal.set_notice(Some(why));
        }
    }

    /// Plans saved offline get their snapshot once the reference turns up in the cache. Cheap
    /// when nothing is pending, and never fetches.
    fn tick_plans(&mut self, now: Instant) {
        const EVERY: Duration = Duration::from_secs(5);
        if !self.plans.has_unresolved()
            || self
                .last_resolve
                .is_some_and(|at| now.duration_since(at) < EVERY)
        {
            return;
        }
        self.last_resolve = Some(now);
        if let Some(data) = self.refdata.clone()
            && self.plans.resolve_pending(&data, self.now_unix())
        {
            self.refresh_plans_modal();
        }
    }

    fn open_palette(&mut self) {
        self.palette = Some(Palette::new(self.palette_rows()));
    }

    fn open_keybindings(&mut self) {
        let activity = &self.activities[self.foreground];
        let rows = crate::keybindings::rows(
            activity.name(),
            &activity.keys(),
            activity.text_focus(&ctx!(self)),
        );
        self.keybindings = Some(KeybindingsModal::new(rows));
    }

    fn open_config(&mut self) {
        self.config = Some(ConfigDialog::open(
            self.config_path.as_deref(),
            &self.current_settings(),
            self.rig_name(),
            self.rig_addr(),
        ));
    }

    /// Do what a Chord or alias asks. The go-to palette, keybindings modal, bandplan and rig
    /// switcher are not built yet, so those say so.
    fn perform(&mut self, action: Action) -> Vec<Command> {
        match action {
            Action::GoTo => self.open_palette(),
            Action::Keybindings => self.open_keybindings(),
            Action::RigSwitcher => {
                self.open_switcher();
            }
            Action::Bandplan => self.bandplan = Some(BandplanModal::open(self.frequency())),
            Action::LogForm if self.embedded() => {
                self.status = Some("the Contact form is already on this screen".to_string());
            }
            Action::LogForm => self.open_log_form(),
            Action::SessionList => self.logging.open_list(),
            Action::Config => self.open_config(),
            Action::Quit => self.open_quit_confirm(),
            _ => {}
        }
        Vec::new()
    }

    /// The Contact form is open as a modal (an embedded form is not a modal).
    pub(crate) fn log_form_open(&self) -> bool {
        self.form_modal_open()
    }

    /// Recent Contacts for the body when no panel applies, newest first.
    pub(crate) fn recent_contacts(&self, n: usize) -> Vec<String> {
        self.logging.recent(n)
    }

    /// The Log for the settings in use now: the config's `[logbook] path`, else the default.
    /// Built fresh each time (a `Log` holds no handle), so a saved path or callsign applies at
    /// once and Contacts already logged keep what they were written with.
    fn current_log(&self) -> Result<Log, String> {
        let operator = self
            .callsign
            .as_deref()
            .and_then(|c| Callsign::new(c).ok())
            .ok_or("no callsign: set `callsign` in the config file to log")?;
        let path = self
            .log_path
            .clone()
            .or_else(|| self.default_log_path.clone())
            .ok_or("no log path: set `[logbook] path` in the config file to log")?;
        Ok(Log::new(path, operator))
    }

    /// The Locator for `MY_GRIDSQUARE`, if one is set and valid.
    fn own_grid(&self) -> Option<Grid> {
        self.locator.as_deref().and_then(|g| Grid::new(g).ok())
    }

    /// The `l` gate. Opens the form with a snapshot of the rig, or says in the status line which
    /// condition failed.
    fn open_log_form(&mut self) {
        self.open_log_form_with(None);
    }

    /// `open_log_form`, with the Call (and Their ref) an Activity chose, such as a spot.
    fn open_log_form_with(&mut self, prefill: Option<LogPrefill>) {
        match self.log_snapshot() {
            Ok(mut snapshot) => {
                self.entry = None;
                if let Some(their_ref) = prefill.as_ref().and_then(|p| p.their_ref.clone()) {
                    snapshot.defaults.their_ref = Some(their_ref);
                }
                self.logging.open_form(snapshot);
                if let Some(p) = prefill.as_ref().filter(|p| !p.call.is_empty()) {
                    self.logging.prefill_call(&p.call);
                }
                self.sync_form_exchange();
            }
            Err(why) => self.status = Some(why),
        }
    }

    /// Publish the open form's call, reference and reports as the Soft key variables
    /// `{THEIRCALL}`, `{THEIRREF}`, `{MYRST}` and `{THEIRRST}`, and put the exchange back as it
    /// was when the form closes.
    fn sync_form_exchange(&mut self) {
        match self.logging.view() {
            Some(view) => {
                if self.exchange_saved.is_none() {
                    self.exchange_saved = Some(self.exchange.clone());
                }
                self.exchange.their_call = Some(view.their_call.to_string());
                self.exchange.their_ref = Some(view.their_ref.to_string());
                self.exchange.my_rst = Some(view.rst_sent.to_string());
                self.exchange.their_rst = Some(view.rst_rcvd.to_string());
            }
            None => {
                if let Some(before) = self.exchange_saved.take() {
                    self.exchange = before;
                }
            }
        }
    }

    fn log_snapshot(&self) -> Result<Snapshot, String> {
        self.snapshot(true)
    }

    /// The rig's reading for a Contact, without touching the Log (for keeping an open form
    /// current).
    fn rig_snapshot(&self) -> Option<Snapshot> {
        self.snapshot(false).ok()
    }

    fn snapshot(&self, check_log: bool) -> Result<Snapshot, String> {
        if let Some(why) = self.activities[self.foreground].log_blocked() {
            return Err(why);
        }
        if !self.connected {
            return Err("not connected: cannot log".to_string());
        }
        let (Some(_), Some(rig_mode)) = (self.frequency(), self.rig_mode().cloned()) else {
            return Err("rig has not reported frequency and mode in the last 2 s".to_string());
        };
        // The transmit frequency under split. Its read-back on the KX3 is untested on hardware.
        let Some(hz) = self.transmit_frequency() else {
            return Err(
                "transmit frequency not known: the rig has not reported split or its transmit frequency".to_string(),
            );
        };
        if band_for_hz(hz).is_none() {
            return Err(format!("{} Hz is outside the band table", hz));
        }
        if check_log {
            self.current_log()?
                .check()
                .map_err(|e| format!("log: {e}"))?;
        }
        let (log_mode, defaults) = self.activities[self.foreground].log_hints(&rig_mode);
        Ok(Snapshot {
            hz,
            rig_mode,
            started: self.clock(),
            log_mode,
            defaults,
        })
    }

    /// The UTC time that stamps Contacts: the same clock the strip shows.
    fn clock(&self) -> DateTime<Utc> {
        self.utc.unwrap_or_else(SystemTime::now).into()
    }

    /// Open a modal; it gets every key until it closes.
    pub fn open_overlay(&mut self, overlay: Box<dyn Overlay>) {
        self.overlay = Some(overlay);
    }

    /// The focus in effect: the Activity text area only while it has focus.
    pub fn focus(&self) -> Focus {
        if self.activities[self.foreground].text_focus(&ctx!(self)) {
            Focus::Panel
        } else {
            Focus::Strip
        }
    }

    /// The logbook mode the foreground Activity maps the rig mode to.
    pub fn log_mode(&self) -> Option<LogMode> {
        if !self.activities[self.foreground].has_panel(&ctx!(self)) {
            return None;
        }
        self.activities[self.foreground]
            .log_hints(self.last_mode()?)
            .0
    }

    /// Extra defaults the foreground Activity contributes to a logged Contact.
    pub fn log_defaults(&self) -> Option<LogDefaults> {
        if !self.activities[self.foreground].has_panel(&ctx!(self)) {
            return None;
        }
        Some(
            self.activities[self.foreground]
                .log_hints(self.last_mode()?)
                .1,
        )
    }

    pub(crate) fn activity_hint(&self) -> &'static str {
        self.activities[self.foreground].hint(&ctx!(self))
    }

    /// The last mode the rig reported, however old: the panel stays put while the link is down.
    fn last_mode(&self) -> Option<&RigMode> {
        self.mode.as_ref().map(|r| &r.value.0)
    }

    /// Tell the app the terminal size, so keys are ignored while the window is too small.
    pub fn set_size(&mut self, width: u16, height: u16) {
        self.size = Some((width, height));
    }

    /// Whether the window is below the floor, so only the notice is shown.
    pub fn too_small(&self) -> bool {
        self.size
            .is_some_and(|(w, h)| matches!(layout::fit(w, h), Fit::TooSmall { .. }))
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Vec<Command> {
        let commands = self.on_key_inner(key);
        self.sync_embedded();
        commands
    }

    fn on_key_inner(&mut self, key: KeyEvent) -> Vec<Command> {
        if self.quit_confirm.is_some() {
            return self.on_quit_confirm_key(key);
        }
        let chord = keys::lookup(&key, &[Scope::Chord]).map(|e| e.action);
        // Quit is the one Chord that works over every overlay.
        if chord == Some(Action::Quit) {
            return self.perform(Action::Quit);
        }
        if self.too_small() {
            // Nothing is on screen to act on; only the quit Chord works.
            return Vec::new();
        }
        if let Some(dialog) = &mut self.config {
            return match dialog.on_key(key) {
                Outcome::Stay => Vec::new(),
                Outcome::Lookup(address) => vec![Command::Lookup(address)],
                Outcome::Close => {
                    self.config = None;
                    vec![Command::ClearGeocodeCache]
                }
                Outcome::Apply(settings) => {
                    self.config = None;
                    let mut commands = self.apply_settings(*settings);
                    commands.push(Command::ClearGeocodeCache);
                    commands
                }
            };
        }
        if self.switcher_open() {
            self.on_switcher_key(key);
            return Vec::new();
        }
        if let Some(palette) = &mut self.palette {
            match palette.on_key(key) {
                PaletteOutcome::Keep => {}
                PaletteOutcome::Close => self.palette = None,
                PaletteOutcome::Choose(target) => {
                    self.palette = None;
                    match target {
                        Target::Activity(index) => self.set_foreground(index),
                        Target::Plan(index) => self.launch_plan(index),
                        Target::ManagePlans => self.open_plans(),
                    }
                }
            }
            return Vec::new();
        }
        if let Some(modal) = &mut self.export_modal {
            if modal.on_key(key) {
                self.export_modal = None;
            }
            return Vec::new();
        }
        if self.plans_modal.is_some() {
            self.on_plans_key(key);
            return Vec::new();
        }
        if let Some(modal) = &mut self.keybindings {
            if modal.on_key(key) == OverlayAction::Close {
                self.keybindings = None;
            }
            return Vec::new();
        }
        if let Some(overlay) = &mut self.overlay {
            if overlay.on_key(key) == OverlayAction::Close {
                self.overlay = None;
            }
            return Vec::new();
        }
        if let Some(modal) = &mut self.bandplan {
            let rx = self.freq.as_ref().map(|r| r.value);
            let action = modal.on_key(key, rx);
            return self.on_bandplan_action(action);
        }
        if self.logging_modal_open() {
            // The session list holds nothing to lose, so a Chord may open over it.
            if let (true, Some(action)) = (!self.form_modal_open(), chord) {
                return self.perform(action);
            }
            // Soft keys work while the form has the keyboard; the Activity sees what is typed.
            if self.form_modal_open() && matches!(key.code, KeyCode::F(1..=6)) {
                let ctx = ctx!(self);
                return match self.activities[self.foreground].on_key(key, &ctx) {
                    Some(out) => {
                        if out.status.is_some() {
                            self.status = out.status;
                        }
                        out.commands
                    }
                    None => Vec::new(),
                };
            }
            self.feed_form(key);
            return Vec::new();
        }
        if let Some(action) = chord {
            self.entry = None;
            return self.perform(action);
        }
        if self.entry.is_some() {
            return self.on_entry_key(key);
        }
        if self.embedded()
            && let Some(commands) = self.on_embedded_key(key)
        {
            return commands;
        }
        let worked = self.worked_today();
        let ctx = ctx!(self, &[], crate::layout::Panes::MINIMAL, &worked);
        if let Some(out) = self.activities[self.foreground].on_key(key, &ctx) {
            if out.status.is_some() {
                self.status = out.status;
            }
            let mut commands = out.commands;
            match self.activities[self.foreground].take_request() {
                Some(HostRequest::OpenLog(prefill)) => self.open_log_form_with(Some(prefill)),
                Some(HostRequest::Tune(tune)) => commands.extend(self.apply_tune(tune)),
                None => {}
            }
            return commands;
        }
        self.on_strip_key(key)
    }

    /// Give a key to the Contact form (modal or embedded) and act on a save.
    fn feed_form(&mut self, key: KeyEvent) {
        let now = self.clock();
        let (log, grid) = (self.current_log(), self.own_grid());
        let before = self.logging.count();
        if let Some(status) = self.logging.on_key(key, now, log, grid.as_ref()) {
            self.status = Some(status);
        }
        if self.logging.count() != before {
            self.activation_stale = true;
        }
        if self.logging.take_reopen() {
            // Quick entry: the next Contact needs a fresh rig reading, like any other.
            match self.log_snapshot() {
                Ok(snapshot) => self.logging.open_form(snapshot),
                Err(why) => {
                    let saved = self.status.take().unwrap_or_default();
                    self.status = Some(format!("{saved}; {why}"));
                }
            }
        }
        self.sync_form_exchange();
    }

    /// A key on an Activity-embedded Contact form screen: plan picking, then the Activity's
    /// routing between the form and the rest. `None`: not taken, so the Activity's own keys and
    /// the shared strip keys get it.
    fn on_embedded_key(&mut self, key: KeyEvent) -> Option<Vec<Command>> {
        if key.modifiers.contains(KeyModifiers::ALT)
            && let KeyCode::Char(c @ '1'..='9') = key.code
        {
            let planless = self.activation.as_ref().is_none_or(|a| a.banner.is_none());
            if planless
                && let Some(choice) = self
                    .plan_choices(self.activity_name())
                    .get(usize::from(c as u8 - b'1'))
            {
                let index = choice.index;
                self.launch_plan(index);
            }
            return Some(Vec::new());
        }
        let ctx = ctx!(self);
        match self.activities[self.foreground].form_key(key, &ctx) {
            FormRoute::Pass => None,
            FormRoute::Handled => Some(Vec::new()),
            FormRoute::Form => {
                self.feed_form(key);
                Some(Vec::new())
            }
            FormRoute::Discard => {
                self.logging.discard();
                self.sync_embedded();
                Some(Vec::new())
            }
        }
    }

    /// Today's Contacts from the Log, for an Activity that marks "worked today". Derived afresh
    /// each time so there is no state to drift; empty for any other Activity.
    fn worked_today(&self) -> Vec<Worked> {
        if !self.activities[self.foreground].uses_log() {
            return Vec::new();
        }
        let Ok(log) = self.current_log() else {
            return Vec::new();
        };
        let today = self.clock().date_naive();
        // A stat per frame instead of reading and parsing the whole Log: the cache holds until the
        // file changes (a save appends, so its size grows) or the UTC date rolls over.
        let meta = std::fs::metadata(log.path()).ok();
        let key = WorkedKey {
            path: log.path().to_path_buf(),
            modified: meta.as_ref().and_then(|m| m.modified().ok()),
            len: meta.as_ref().map_or(0, |m| m.len()),
            day: today,
        };
        if let Some((cached, worked)) = &*self.worked_cache.borrow()
            && *cached == key
        {
            return worked.clone();
        }
        let Ok(records) = log.read() else {
            return Vec::new();
        };
        let worked: Vec<Worked> = records
            .iter()
            .filter(|r| r.start().is_some_and(|s| s.date_naive() == today))
            .filter_map(|r| {
                Some(Worked {
                    call: r.call()?.to_ascii_uppercase(),
                    band: r.band()?.to_string(),
                    mode: r.mode()?.to_ascii_uppercase(),
                })
            })
            .collect();
        *self.worked_cache.borrow_mut() = Some((key, worked.clone()));
        worked
    }

    /// Tune the receive frequency (and the mode when one is mapped) for an Activity. Refused
    /// while any hosted Activity is correcting the dial itself. Never touches split, and none of
    /// these commands can key the transmitter.
    fn apply_tune(&mut self, tune: TuneRequest) -> Vec<Command> {
        if let Some(why) = self.activities.iter().find_map(|a| a.tuning_blocked()) {
            self.status = Some(format!("not while {why}"));
            return Vec::new();
        }
        if !self.ready() {
            return Vec::new();
        }
        let mut commands = Vec::new();
        match tune.mode {
            Some(mode) => {
                self.status = None;
                if self.last_mode() != Some(&mode) {
                    commands.push(Command::SetMode { mode, passband: 0 });
                }
            }
            None => {
                self.status = Some(format!("mode {} not set: frequency only", tune.mode_name));
            }
        }
        commands.push(Command::SetFreq(tune.hz));
        commands
    }

    /// The plain keys and aliases of the shared table. Only reached when no text-entering area
    /// has focus.
    fn on_strip_key(&mut self, key: KeyEvent) -> Vec<Command> {
        if self.focus() == Focus::Panel {
            return Vec::new();
        }
        let Some(entry) = keys::lookup(&key, &[Scope::Plain, Scope::Alias]) else {
            return Vec::new();
        };
        match entry.action {
            Action::Quit => {
                if self.activity_name() == "General" {
                    self.open_quit_confirm();
                } else {
                    self.status = entry.blocked.map(String::from);
                }
                Vec::new()
            }
            Action::LogForm | Action::SessionList | Action::Config => self.perform(entry.action),
            Action::FreqEntry => {
                self.start_entry(Entry::Freq(String::new()));
                Vec::new()
            }
            Action::FreqDigit(c) => {
                self.start_entry(Entry::Freq(c.to_string()));
                Vec::new()
            }
            Action::SplitFreqEntry => {
                if self.require(Feature::Split) && self.require_split_on() {
                    self.start_entry(Entry::SplitFreq(String::new()));
                }
                Vec::new()
            }
            Action::TuneUp => self.tune(true),
            Action::TuneDown => self.tune(false),
            Action::StepUp => {
                self.step = (self.step + 1).min(STEPS.len() - 1);
                Vec::new()
            }
            Action::StepDown => {
                self.step = self.step.saturating_sub(1);
                Vec::new()
            }
            Action::ModeNext => self.cycle_mode(true),
            Action::ModePrev => self.cycle_mode(false),
            Action::ToggleVfo => self.toggle_vfo(),
            Action::ToggleSplit => self.toggle_split(),
            _ => Vec::new(),
        }
    }

    /// Whether a control can be used now; if not, say why in the status line.
    fn ready(&mut self) -> bool {
        if !self.connected {
            self.status = Some("not connected".to_string());
        }
        self.connected
    }

    /// `ready`, and the feature is not known to be unsupported.
    fn require(&mut self, feature: Feature) -> bool {
        if !self.ready() {
            return false;
        }
        if self.is_unsupported(feature) {
            self.status = Some(format!("{} not supported", feature.label()));
            return false;
        }
        true
    }

    fn require_split_on(&mut self) -> bool {
        match self.split() {
            Some((true, _)) => true,
            Some((false, _)) => {
                self.status = Some("split is off".to_string());
                false
            }
            None => {
                self.status = Some("split state not known".to_string());
                false
            }
        }
    }

    fn start_entry(&mut self, entry: Entry) {
        if self.ready() {
            self.entry = Some(entry);
        }
    }

    fn on_entry_key(&mut self, key: KeyEvent) -> Vec<Command> {
        let Some(entry) = self.entry.as_mut() else {
            return Vec::new();
        };
        match key.code {
            KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => entry.text_mut().push(c),
            KeyCode::Backspace => {
                entry.text_mut().pop();
            }
            KeyCode::Esc => self.entry = None,
            KeyCode::Enter => {
                let entry = self.entry.take().expect("checked above");
                return match parse_entry(entry.text()) {
                    Some(hz) if matches!(entry, Entry::Freq(_)) => vec![Command::SetFreq(hz)],
                    Some(hz) => vec![Command::SetSplitFreq(hz)],
                    None => {
                        self.status = Some(format!("not a frequency: {}", entry.text()));
                        Vec::new()
                    }
                };
            }
            _ => {}
        }
        Vec::new()
    }

    fn tune(&mut self, up: bool) -> Vec<Command> {
        if !self.ready() {
            return Vec::new();
        }
        let Some(hz) = self.frequency() else {
            self.status = Some("no current frequency to tune from".to_string());
            return Vec::new();
        };
        let step = self.tuning_step();
        let target = if up {
            hz.saturating_add(step)
        } else {
            hz.saturating_sub(step)
        };
        vec![Command::SetFreq(target)]
    }

    fn cycle_mode(&mut self, forward: bool) -> Vec<Command> {
        if !self.ready() {
            return Vec::new();
        }
        let Some(current) = self.rig_mode() else {
            self.status = Some("no current mode".to_string());
            return Vec::new();
        };
        let len = MODE_CYCLE.len();
        let next = match MODE_CYCLE.iter().position(|m| m == current) {
            Some(i) if forward => (i + 1) % len,
            Some(i) => (i + len - 1) % len,
            // A mode outside the cycle (a data mode, say): start from USB.
            None => 1,
        };
        vec![Command::SetMode {
            mode: MODE_CYCLE[next].clone(),
            passband: 0,
        }]
    }

    fn toggle_vfo(&mut self) -> Vec<Command> {
        if !self.require(Feature::Vfo) {
            return Vec::new();
        }
        let Some(current) = self.vfo() else {
            self.status = Some("VFO not known yet".to_string());
            return Vec::new();
        };
        vec![Command::SetVfo(other_vfo(current).to_string())]
    }

    fn toggle_split(&mut self) -> Vec<Command> {
        if !self.require(Feature::Split) {
            return Vec::new();
        }
        let Some((on, _)) = self.split() else {
            self.status = Some("split state not known yet".to_string());
            return Vec::new();
        };
        let current = self.vfo().unwrap_or("VFOA");
        let tx_vfo = if on {
            // Off: the verified call is `S 0 <receive VFO>`.
            current.to_string()
        } else {
            other_vfo(current).to_string()
        };
        vec![Command::SetSplit {
            enabled: !on,
            tx_vfo,
        }]
    }

    pub fn on_rig(&mut self, event: RigEvent) {
        match event {
            RigEvent::Connected => {
                self.connected = true;
                self.last_poll = None;
                self.last_smeter = None;
                self.unsupported.clear();
                self.status = None;
                self.poll_failed = false;
            }
            RigEvent::Disconnected(why) => {
                self.connected = false;
                self.entry = None;
                self.status = Some(why);
            }
            RigEvent::Freq(hz) => self.freq = self.stamp(hz),
            RigEvent::Mode { mode, passband } => {
                self.mode = self.stamp((mode, passband));
                let ctx = ctx!(self);
                self.activities[self.foreground].on_mode(&ctx);
            }
            RigEvent::Vfo(vfo) => self.vfo = self.stamp(vfo),
            RigEvent::Split { enabled, tx_vfo } => self.split = self.stamp((enabled, tx_vfo)),
            RigEvent::SplitFreq(hz) => self.split_freq = self.stamp(hz),
            RigEvent::Ptt(on) => self.ptt = self.stamp(on),
            RigEvent::Strength(db) => self.strength = self.stamp(db),
            RigEvent::Unsupported(feature) => {
                self.unsupported.insert(feature);
            }
            RigEvent::SetFailed(why) | RigEvent::SendFailed(why) => self.status = Some(why),
            RigEvent::PanelReply { request, reply } => {
                match self.activities[self.foreground].on_reply(&request, reply) {
                    Ok(()) => {
                        if self.poll_failed {
                            self.status = None;
                            self.poll_failed = false;
                        }
                    }
                    Err(why) => {
                        self.status = Some(why);
                        self.poll_failed = true;
                    }
                }
            }
        }
    }

    /// Rig work due at `now`: a reconnect attempt while disconnected; while connected the state
    /// poll, the S-meter poll and the active panel's requests (the CW panel's `TB;` every tick).
    pub fn tick(&mut self, now: Instant) -> Vec<Command> {
        self.now = now;
        self.tick_plans(now);
        self.sync_embedded();
        if !self.connected {
            let due = self
                .last_connect
                .is_none_or(|at| now.duration_since(at) >= RECONNECT_EVERY);
            let mut commands = self.tick_foreground(now);
            if due {
                self.last_connect = Some(now);
                commands.insert(0, Command::Connect);
            }
            commands.extend(self.tick_inactive(now));
            return commands;
        }
        let mut commands = self.tick_inactive(now);
        if self
            .last_poll
            .is_none_or(|at| now.duration_since(at) >= STATE_POLL_EVERY)
        {
            self.last_poll = Some(now);
            commands.push(Command::PollState);
        }
        if self
            .last_smeter
            .is_none_or(|at| now.duration_since(at) >= SMETER_POLL_EVERY)
        {
            self.last_smeter = Some(now);
            commands.push(Command::PollSMeter);
        }
        commands.extend(self.tick_foreground(now));
        commands
    }

    /// Only the foreground Activity is ticked.
    fn tick_foreground(&mut self, now: Instant) -> Vec<Command> {
        let ctx = ctx!(self);
        let mut commands = self.activities[self.foreground].on_tick(now, &ctx);
        // Doppler correction is the one thing an Activity may do to the dial by itself, and only
        // on a rig with satellite mode. The foreground Activity is the only one ticked, so this
        // also means no Doppler command is ever issued from the background.
        if !self.capabilities().has(Capability::SatMode) {
            commands.retain(|c| !matches!(c, Command::Doppler(_)));
        }
        commands
    }

    pub(crate) fn render_body(&self, frame: &mut Frame, area: Rect, panes: Panes) {
        // Room inside the border, less the heading line.
        let worked = self.worked_today();
        let rows = usize::from(area.height.saturating_sub(3));
        let (recent, choices) = if self.embedded() {
            let planless = self.activation.as_ref().is_none_or(|a| a.banner.is_none());
            let choices = if planless {
                self.plan_choices(self.activity_name())
            } else {
                Vec::new()
            };
            (self.logging.recent_activator(rows), choices)
        } else {
            (self.recent_contacts(rows), Vec::new())
        };
        let ctx = ActivityCtx {
            plans: &choices,
            ..ctx!(self, &recent, panes, &worked)
        };
        self.activities[self.foreground].render(frame, area, &ctx);
    }

    pub(crate) fn render_overlay(&self, frame: &mut Frame, area: Rect) {
        if let Some(overlay) = &self.overlay {
            overlay.render(frame, area);
        }
        if let Some(dialog) = &self.config {
            dialog.render(frame, area);
        }
        self.render_switcher(frame, area);
        if let Some(modal) = &self.bandplan {
            let view = BandView {
                rx: self.frequency(),
                tx: match self.split() {
                    Some((true, _)) => self.transmit_frequency(),
                    _ => None,
                },
                class: self.license_class,
                today: self.clock().date_naive(),
            };
            modal.render(frame, area, &view);
        }
        if let Some(palette) = &self.palette {
            palette.render(frame, area);
        }
        if let Some(modal) = &self.plans_modal {
            modal.render(frame, area);
        }
        if let Some(modal) = &self.export_modal {
            modal.render(frame, area);
        }
        if let Some(modal) = &self.keybindings {
            modal.render(frame, area);
        }
        self.logging
            .render(frame, area, self.own_grid().is_some(), self.embedded());
        if let Some(lines) = &self.quit_confirm {
            let rows = lines.len() as u16;
            let inner = Modal::new("Quit rig-tui?", 60, rows + 5).render(frame, area);
            let mut text: Vec<Line> = lines
                .iter()
                .map(|l| Line::styled(l.clone(), red()))
                .collect();
            if !lines.is_empty() {
                text.push(Line::raw(""));
            }
            text.push(Line::raw("Quit rig-tui? (default: cancel)"));
            text.push(Line::styled(
                "y or Ctrl-C quit    n, Esc or Enter cancel",
                dim(),
            ));
            frame.render_widget(Paragraph::new(text), inner);
        }
    }
}

fn other_vfo(vfo: &str) -> &'static str {
    if vfo == "VFOA" { "VFOB" } else { "VFOA" }
}

/// A typed frequency in Hz: kilohertz as a whole number, or megahertz when it has a dot.
fn parse_entry(text: &str) -> Option<u64> {
    if text.is_empty() {
        return None;
    }
    match text.split_once('.') {
        None => text.parse::<u64>().ok()?.checked_mul(1_000),
        Some((mhz, frac)) => {
            if frac.contains('.') || frac.len() > 6 || (mhz.is_empty() && frac.is_empty()) {
                return None;
            }
            let whole: u64 = if mhz.is_empty() { 0 } else { mhz.parse().ok()? };
            let mut digits = frac.to_string();
            while digits.len() < 6 {
                digits.push('0');
            }
            let fraction: u64 = digits.parse().ok()?;
            whole.checked_mul(1_000_000)?.checked_add(fraction)
        }
    }
}
