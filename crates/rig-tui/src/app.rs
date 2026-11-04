//! The pure application state: key events and rig events in, commands out. No I/O; ticks carry
//! the time (and `set_utc` the wall clock), so every behaviour is testable without a rig or a
//! terminal.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant, SystemTime};

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use crate::cw::CwPanel;
use crate::panel::{LogDefaults, LogMode, ModePanel, PanelCommand, PanelCtx};
use crate::rig::{Command, Feature, RigEvent, RigMode};

/// How long to wait between attempts to reach `rigctld`. A starting value, untested on hardware.
pub const RECONNECT_EVERY: Duration = Duration::from_secs(3);
/// How often frequency, mode, VFO, split and PTT are read (about twice a second). Untested on
/// hardware.
pub const STATE_POLL_EVERY: Duration = Duration::from_millis(500);
/// How often the S-meter is read (about four times a second). Untested on hardware.
pub const SMETER_POLL_EVERY: Duration = Duration::from_millis(250);
/// A reading older than this shows as a dash. A starting value, untested on hardware.
pub const STALE_AFTER: Duration = Duration::from_secs(2);

/// Tuning steps in Hz, smallest first.
pub(crate) const STEPS: [u64; 6] = [1, 10, 100, 1_000, 10_000, 100_000];
const DEFAULT_STEP: usize = 3;

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
struct Reading<T> {
    value: T,
    at: Instant,
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

pub struct App {
    panels: Vec<Box<dyn ModePanel>>,
    overlay: Option<Box<dyn Overlay>>,
    focus: Focus,
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
    status: Option<String>,
    /// The status came from a failing poll, so the next good poll clears it.
    poll_failed: bool,
    callsign: Option<String>,
    soft_keys: BTreeMap<u8, String>,
    last_connect: Option<Instant>,
    last_poll: Option<Instant>,
    last_smeter: Option<Instant>,
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
            panels: vec![Box::new(CwPanel::new())],
            overlay: None,
            focus: Focus::Strip,
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
            status: None,
            poll_failed: false,
            callsign: None,
            soft_keys: BTreeMap::new(),
            last_connect: None,
            last_poll: None,
            last_smeter: None,
        }
    }

    /// An app that expands `{CALL}` in sent text to `callsign`.
    pub fn with_callsign(callsign: Option<String>) -> Self {
        Self {
            callsign,
            ..Self::new()
        }
    }

    /// Bind F1..F6 to canned messages (text is uppercase; `{CALL}` expands to the callsign).
    pub fn with_soft_keys(mut self, soft_keys: BTreeMap<u8, String>) -> Self {
        self.soft_keys = soft_keys;
        self
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
    }

    /// Open a modal; it gets every key until it closes.
    pub fn open_overlay(&mut self, overlay: Box<dyn Overlay>) {
        self.overlay = Some(overlay);
    }

    /// The focus in effect: the panel's input line only while a panel that takes text is active.
    pub fn focus(&self) -> Focus {
        match self.active_panel() {
            Some(i) if self.focus == Focus::Panel && self.panels[i].captures_text() => Focus::Panel,
            _ => Focus::Strip,
        }
    }

    /// The logbook mode the active panel maps the rig's mode to.
    pub fn log_mode(&self) -> Option<LogMode> {
        let panel = &self.panels[self.active_panel()?];
        panel.logbook_mode(self.last_mode()?)
    }

    /// Extra defaults the active panel contributes to a logged Contact.
    pub fn log_defaults(&self) -> Option<LogDefaults> {
        let panel = &self.panels[self.active_panel()?];
        Some(panel.log_defaults())
    }

    pub(crate) fn panel_active(&self) -> bool {
        self.active_panel().is_some()
    }

    /// The last mode the rig reported, however old: the panel stays put while the link is down.
    fn last_mode(&self) -> Option<&RigMode> {
        self.mode.as_ref().map(|r| &r.value.0)
    }

    fn active_panel(&self) -> Option<usize> {
        let mode = self.last_mode()?;
        self.panels.iter().position(|p| p.applies(mode))
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Vec<Command> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return vec![Command::Quit];
        }
        if let Some(overlay) = &mut self.overlay {
            if overlay.on_key(key) == OverlayAction::Close {
                self.overlay = None;
            }
            return Vec::new();
        }
        if self.entry.is_some() {
            return self.on_entry_key(key);
        }
        let Some(index) = self.active_panel() else {
            return self.on_strip_key(key, false);
        };
        let panel_key = matches!(key.code, KeyCode::F(1..=6)) || self.focus() == Focus::Panel;
        if matches!(key.code, KeyCode::Esc | KeyCode::Tab) && self.focus() == Focus::Panel {
            self.focus = Focus::Strip;
            return Vec::new();
        }
        if panel_key {
            return self.on_panel_key(index, key);
        }
        let can_type = self.panels[index].captures_text();
        self.on_strip_key(key, can_type)
    }

    fn on_strip_key(&mut self, key: KeyEvent, panel_takes_text: bool) -> Vec<Command> {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Vec::new();
        }
        match key.code {
            KeyCode::Char('q') => vec![Command::Quit],
            KeyCode::Tab if panel_takes_text => {
                self.focus = Focus::Panel;
                Vec::new()
            }
            KeyCode::Char('f') => {
                self.start_entry(Entry::Freq(String::new()));
                Vec::new()
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                self.start_entry(Entry::Freq(c.to_string()));
                Vec::new()
            }
            KeyCode::Char('S') => {
                if self.require(Feature::Split) && self.require_split_on() {
                    self.start_entry(Entry::SplitFreq(String::new()));
                }
                Vec::new()
            }
            KeyCode::Right => self.tune(true),
            KeyCode::Left => self.tune(false),
            KeyCode::Up => {
                self.step = (self.step + 1).min(STEPS.len() - 1);
                Vec::new()
            }
            KeyCode::Down => {
                self.step = self.step.saturating_sub(1);
                Vec::new()
            }
            KeyCode::Char('m') => self.cycle_mode(true),
            KeyCode::Char('M') => self.cycle_mode(false),
            KeyCode::Char('v') => self.toggle_vfo(),
            KeyCode::Char('s') => self.toggle_split(),
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

    fn on_panel_key(&mut self, index: usize, key: KeyEvent) -> Vec<Command> {
        let ctx = PanelCtx {
            connected: self.connected,
            callsign: self.callsign.as_deref(),
            soft_keys: &self.soft_keys,
            focused: self.focus == Focus::Panel,
        };
        let commands = self.panels[index].handle_key(key, &ctx);
        let mut out = Vec::new();
        for command in commands {
            match command {
                PanelCommand::Send(text) => out.push(Command::Send(text)),
                PanelCommand::Status(text) => self.status = Some(text),
            }
        }
        out
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
                if self.active_panel().is_none() {
                    self.focus = Focus::Strip;
                }
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
                let Some(index) = self.active_panel() else {
                    return;
                };
                match self.panels[index].on_reply(&request, reply) {
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
        if !self.connected {
            let due = self
                .last_connect
                .is_none_or(|at| now.duration_since(at) >= RECONNECT_EVERY);
            if !due {
                return Vec::new();
            }
            self.last_connect = Some(now);
            return vec![Command::Connect];
        }
        let mut commands = Vec::new();
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
        if let Some(index) = self.active_panel() {
            commands.extend(
                self.panels[index]
                    .due_requests()
                    .into_iter()
                    .map(Command::Panel),
            );
        }
        commands
    }

    pub(crate) fn render_body(&self, frame: &mut Frame, area: Rect) {
        match self.active_panel() {
            Some(index) => {
                let ctx = PanelCtx {
                    connected: self.connected,
                    callsign: self.callsign.as_deref(),
                    soft_keys: &self.soft_keys,
                    focused: self.focus() == Focus::Panel,
                };
                self.panels[index].render(frame, area, &ctx);
            }
            None => crate::ui::render_no_panel(self, frame, area),
        }
    }

    pub(crate) fn render_overlay(&self, frame: &mut Frame, area: Rect) {
        if let Some(overlay) = &self.overlay {
            overlay.render(frame, area);
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
