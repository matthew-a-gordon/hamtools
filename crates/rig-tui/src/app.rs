//! The pure application state: key events and rig events in, commands out. No I/O, no clock
//! (ticks carry the time), so every behaviour is testable without a rig or a terminal.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hamtools_config::{Callsign, Grid, Settings};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use crate::config_dialog::{ConfigDialog, Outcome};
use crate::cw::CwPanel;
use crate::geocode::GeocodeResult;
use crate::panel::{LogDefaults, LogMode, ModePanel, PanelCommand, PanelCtx};
use crate::rig::{Command, RigEvent, RigMode};

/// How long to wait between attempts to reach `rigctld`. A starting value, untested on hardware.
pub const RECONNECT_EVERY: Duration = Duration::from_secs(3);
/// How often frequency and mode are read (about twice a second). Untested on hardware.
pub const STATE_POLL_EVERY: Duration = Duration::from_millis(500);

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

pub struct App {
    panels: Vec<Box<dyn ModePanel>>,
    overlay: Option<Box<dyn Overlay>>,
    focus: Focus,
    connected: bool,
    freq: Option<u64>,
    mode: Option<RigMode>,
    passband: Option<i64>,
    status: Option<String>,
    /// The status came from a failing poll, so the next good poll clears it.
    poll_failed: bool,
    callsign: Option<String>,
    soft_keys: BTreeMap<u8, String>,
    locator: Option<String>,
    log_path: Option<PathBuf>,
    rig_addr: String,
    config_path: Option<PathBuf>,
    config: Option<ConfigDialog>,
    last_connect: Option<Instant>,
    last_poll: Option<Instant>,
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
            freq: None,
            mode: None,
            passband: None,
            status: None,
            poll_failed: false,
            callsign: None,
            soft_keys: BTreeMap::new(),
            locator: None,
            log_path: None,
            rig_addr: DEFAULT_RIG_ADDR.to_string(),
            config_path: None,
            config: None,
            last_connect: None,
            last_poll: None,
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

    /// The config file the dialog edits (`None`: no location, so the dialog is read-only).
    pub fn with_config_path(mut self, path: Option<PathBuf>) -> Self {
        self.config_path = path;
        self
    }

    /// The Locator in use (6 characters, uppercase), if any.
    pub fn with_locator(mut self, locator: Option<String>) -> Self {
        self.locator = locator;
        self
    }

    pub fn with_log_path(mut self, log_path: Option<PathBuf>) -> Self {
        self.log_path = log_path;
        self
    }

    /// The `rigctld` address in use, shown in the config dialog when the file sets none.
    pub fn with_rig_addr(mut self, addr: impl Into<String>) -> Self {
        self.rig_addr = addr.into();
        self
    }

    pub fn locator(&self) -> Option<&str> {
        self.locator.as_deref()
    }

    /// The Log path in use; `None` means the default.
    pub fn log_path(&self) -> Option<&Path> {
        self.log_path.as_deref()
    }

    pub fn rig_addr(&self) -> &str {
        &self.rig_addr
    }

    /// What the app is using now, as config settings.
    fn current_settings(&self) -> Settings {
        Settings {
            callsign: self.callsign.as_deref().and_then(|c| Callsign::new(c).ok()),
            locator: self.locator.as_deref().and_then(|g| Grid::new(g).ok()),
            log_path: self.log_path.clone(),
            rig_addr: Some(self.rig_addr.clone()),
            macros: self.soft_keys.clone(),
        }
    }

    /// Use saved settings now, with no restart. Returns what the rig side must do.
    fn apply_settings(&mut self, settings: Settings) -> Vec<Command> {
        self.callsign = settings.callsign.map(|c| c.as_str().to_string());
        self.locator = settings.locator.map(|g| g.as_str().to_string());
        self.log_path = settings.log_path;
        self.soft_keys = settings.macros;
        match settings.rig_addr {
            Some(addr) if addr != self.rig_addr => {
                self.rig_addr = addr.clone();
                self.connected = false;
                self.last_connect = None;
                vec![Command::SetRigAddr(addr)]
            }
            _ => Vec::new(),
        }
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

    pub fn frequency(&self) -> Option<u64> {
        self.freq
    }

    pub fn rig_mode(&self) -> Option<&RigMode> {
        self.mode.as_ref()
    }

    pub fn passband(&self) -> Option<i64> {
        self.passband
    }

    pub fn callsign(&self) -> Option<&str> {
        self.callsign.as_deref()
    }

    pub fn soft_keys(&self) -> &BTreeMap<u8, String> {
        &self.soft_keys
    }

    pub fn has_overlay(&self) -> bool {
        self.overlay.is_some() || self.config.is_some()
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
        panel.logbook_mode(self.mode.as_ref()?)
    }

    /// Extra defaults the active panel contributes to a logged Contact.
    pub fn log_defaults(&self) -> Option<LogDefaults> {
        let panel = &self.panels[self.active_panel()?];
        Some(panel.log_defaults())
    }

    pub(crate) fn panel_active(&self) -> bool {
        self.active_panel().is_some()
    }

    fn active_panel(&self) -> Option<usize> {
        let mode = self.mode.as_ref()?;
        self.panels.iter().position(|p| p.applies(mode))
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Vec<Command> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return vec![Command::Quit];
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
                    let mut commands = self.apply_settings(settings);
                    commands.push(Command::ClearGeocodeCache);
                    commands
                }
            };
        }
        if let Some(overlay) = &mut self.overlay {
            if overlay.on_key(key) == OverlayAction::Close {
                self.overlay = None;
            }
            return Vec::new();
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
        match key.code {
            KeyCode::Char('q') => vec![Command::Quit],
            KeyCode::Char('c') => {
                self.config = Some(ConfigDialog::open(
                    self.config_path.as_deref(),
                    &self.current_settings(),
                    &self.rig_addr,
                ));
                Vec::new()
            }
            KeyCode::Tab if panel_takes_text => {
                self.focus = Focus::Panel;
                Vec::new()
            }
            _ => Vec::new(),
        }
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
                self.status = None;
                self.poll_failed = false;
            }
            RigEvent::Disconnected(why) => {
                self.connected = false;
                self.status = Some(why);
            }
            RigEvent::Freq(hz) => self.freq = Some(hz),
            RigEvent::Mode { mode, passband } => {
                self.mode = Some(mode);
                self.passband = Some(passband);
                if self.active_panel().is_none() {
                    self.focus = Focus::Strip;
                }
            }
            RigEvent::SendFailed(why) => self.status = Some(why),
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
    /// poll and the active panel's requests (the CW panel's `TB;` every tick).
    pub fn tick(&mut self, now: Instant) -> Vec<Command> {
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
        if let Some(dialog) = &self.config {
            dialog.render(frame, area);
        }
    }
}
