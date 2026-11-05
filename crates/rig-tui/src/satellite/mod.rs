//! The Satellite Activity: `sat-tui`'s pass list, transmitter picker and live/tracking screens,
//! hosted inside `rig-tui`. The shared strip, Contact form, session list and rig connection are
//! the app's. All of it is untested on hardware (the FT-847 in particular).
//!
//! The Activity does no I/O. It plans Doppler commands ([`Command::Doppler`]) while it is the
//! foreground Activity and tracking; the App performs them on the Active rig only when that rig
//! has the `sat_mode` capability. Leaving the Activity stops tracking and does not restore the
//! radio.

mod state;
mod ui;

use std::time::{Duration as StdDuration, Instant};

use chrono::{DateTime, Utc};
use hamtools_config::SatelliteSettings;
use orbit::Observer;
use orbit::parse_tles;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use sat_tui::cache::{Cache, Dataset, tles_stale};
use sat_tui::satnogs::{RigMode as SatMode, Transmitter, parse_transmitters};

use crate::activity::{Activity, ActivityCtx, ActivityOutput, HostEvent};
use crate::keys::{Action, Key, KeyEntry, Scope};
use crate::panel::{LogDefaults, LogMode, Role};
use crate::rig::{Command, RigMode};
use crate::rigs::Capability;

pub use state::{
    Choice, ContactPrefill, Countdown, Link, Live, Request, Row, SatState, TrackingView,
};
pub use ui::{Footer, format_countdown, format_duration, render, short_name};

/// Rebuild the pass list now and then so finished passes drop off and the horizon moves on.
const REBUILD_EVERY: StdDuration = StdDuration::from_secs(30 * 60);
/// Tracking is evaluated once a second, as `sat-tui` did.
const STEP_EVERY: StdDuration = StdDuration::from_secs(1);

/// What the Activity is built from: the satellites and transmitters in the cache, and when each
/// was downloaded.
#[derive(Clone, Default)]
pub struct SatelliteData {
    pub sats: Vec<orbit::Satellite>,
    pub transmitters: Vec<Transmitter>,
    pub tle_fetched: Option<DateTime<Utc>>,
    pub transmitters_fetched: Option<DateTime<Utc>>,
}

/// Read whatever the cache holds (nothing is fine: the pass list is just empty). This reads
/// files, so `main` calls it; the Activity never does.
pub fn load_data(cache: &Cache) -> SatelliteData {
    let sats = cache
        .load(Dataset::Tles)
        .ok()
        .flatten()
        .and_then(|c| parse_tles(&c.body).ok())
        .unwrap_or_default();
    let transmitters = cache
        .load(Dataset::Transmitters)
        .ok()
        .flatten()
        .and_then(|c| parse_transmitters(&c.body).ok())
        .unwrap_or_default();
    SatelliteData {
        sats,
        transmitters,
        tle_fetched: cache.state(Dataset::Tles).fetched_at,
        transmitters_fetched: cache.state(Dataset::Transmitters).fetched_at,
    }
}

pub struct Satellite {
    state: SatState,
    data: SatelliteData,
    observer: Observer,
    settings: SatelliteSettings,
    /// Data that arrived while tracking; applied when tracking stops.
    pending_data: Option<SatelliteData>,
    /// New data is in `data`; rebuild the pass list on the next tick (it needs the clock).
    rebuild_due: bool,
    built_at: Instant,
    downloading: bool,
    /// Ask for a stale-data download on the next tick (on every entry to the foreground).
    want_refresh: bool,
    was_connected: bool,
    last_step: Option<Instant>,
}

impl Satellite {
    pub fn new(
        data: SatelliteData,
        observer: Observer,
        settings: SatelliteSettings,
        now: DateTime<Utc>,
    ) -> Self {
        let state = Self::build(&data, observer, &settings, now);
        Self {
            state,
            data,
            observer,
            settings,
            pending_data: None,
            rebuild_due: false,
            built_at: Instant::now(),
            downloading: false,
            want_refresh: true,
            was_connected: false,
            last_step: None,
        }
    }

    fn build(
        data: &SatelliteData,
        observer: Observer,
        settings: &SatelliteSettings,
        now: DateTime<Utc>,
    ) -> SatState {
        SatState::new(
            data.sats.clone(),
            data.transmitters.clone(),
            observer,
            settings,
            now,
        )
    }

    fn rebuild(&mut self, now: DateTime<Utc>) {
        self.state = Self::build(&self.data, self.observer, &self.settings, now);
        self.built_at = Instant::now();
    }

    /// The screens' state, for tests and the palette.
    pub fn state(&self) -> &SatState {
        &self.state
    }

    /// The satellite name written as `SAT_NAME`: the config's map, else the name as shown.
    fn sat_name(&self, shown: &str) -> String {
        self.settings
            .names
            .get(shown)
            .cloned()
            .unwrap_or_else(|| shown.to_string())
    }

    fn footer(&self, now: DateTime<Utc>) -> Footer {
        Footer {
            now,
            tle_age: self.data.tle_fetched.map(|t| now - t),
            transmitter_age: self.data.transmitters_fetched.map(|t| now - t),
            tles_stale: self.data.tle_fetched.is_some_and(|t| tles_stale(t, now)),
            downloading: self.downloading,
        }
    }

    fn out(&mut self, commands: Vec<Command>) -> ActivityOutput {
        ActivityOutput {
            commands,
            status: self.state.take_status(),
        }
    }
}

const fn key(c: char, label: &'static str) -> KeyEntry {
    KeyEntry {
        scope: Scope::Plain,
        key: Key::plain(KeyCode::Char(c)),
        action: Action::Activity("satellite"),
        label,
        blocked: None,
    }
}

/// The ADIF mode of the uplink, for the Contact form.
fn log_mode(mode: SatMode) -> LogMode {
    let (adif, submode) = match mode {
        SatMode::Fm => ("FM", None),
        SatMode::Usb => ("SSB", Some("USB")),
        SatMode::Lsb => ("SSB", Some("LSB")),
        SatMode::Cw => ("CW", None),
        SatMode::Am => ("AM", None),
    };
    LogMode {
        mode: adif.to_string(),
        submode: submode.map(String::from),
        default_rst: None,
    }
}

impl Activity for Satellite {
    fn name(&self) -> &'static str {
        "Satellite"
    }

    fn requires(&self) -> &'static [Capability] {
        &[Capability::SatMode]
    }

    fn on_key(&mut self, key: KeyEvent, _ctx: &ActivityCtx) -> Option<ActivityOutput> {
        // Only the keys the screens use; everything else falls through to the shared keys
        // (frequency, mode, VFO, split, Chords and the `l`/`L`/`c` aliases).
        let ours = matches!(
            key.code,
            KeyCode::Tab
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Char('r' | 'j' | 'k' | 't' | 'Z' | 'u' | '[' | ']' | '{' | '}')
        );
        if !ours
            || key
                .modifiers
                .contains(ratatui::crossterm::event::KeyModifiers::CONTROL)
        {
            return None;
        }
        let commands = match self.state.handle_key(key) {
            Some(Request::Refresh) => vec![Command::SatelliteRefresh { force: true }],
            None => Vec::new(),
        };
        Some(self.out(commands))
    }

    fn keys(&self) -> Vec<KeyEntry> {
        vec![
            key('j', "next pass or transmitter"),
            key('k', "previous pass or transmitter"),
            key('t', "track the transmitter"),
            key('Z', "sat mode off"),
            key('r', "refresh satellite data"),
            key('u', "show unsupported transmitters"),
            key('[', "nudge down 1 kHz"),
            key(']', "nudge up 1 kHz"),
            key('{', "nudge down 100 Hz"),
            key('}', "nudge up 100 Hz"),
        ]
    }

    fn on_tick(&mut self, now: Instant, ctx: &ActivityCtx) -> Vec<Command> {
        let mut commands = Vec::new();
        if std::mem::take(&mut self.want_refresh) {
            commands.push(Command::SatelliteRefresh { force: false });
        }
        if self.state.tracking().is_none() {
            if let Some(data) = self.pending_data.take() {
                self.data = data;
                self.rebuild_due = true;
            }
            if self.rebuild_due || self.built_at.elapsed() > REBUILD_EVERY {
                self.rebuild_due = false;
                self.rebuild(ctx.utc);
            }
        }
        // A restored connection means the radio may have lost what tracking sent.
        if ctx.connected && !self.was_connected {
            self.state.resync();
        }
        self.was_connected = ctx.connected;
        if !ctx.connected {
            return commands;
        }
        if self.state.tracking().is_some() || self.state.has_pending() {
            if self
                .last_step
                .is_some_and(|at| now.duration_since(at) < STEP_EVERY)
            {
                return commands;
            }
            self.last_step = Some(now);
            if let Some(rx) = ctx.freq {
                self.state.follow_dial(rx);
            }
            // A TLE that will not propagate yields nothing this second; tracking keeps trying.
            if let Ok(planned) = self.state.tick(ctx.utc) {
                commands.extend(planned.into_iter().map(Command::Doppler));
            }
        }
        commands
    }

    fn on_reply(&mut self, _request: &str, _reply: Result<String, String>) -> Result<(), String> {
        Ok(())
    }

    fn on_mode(&mut self, _ctx: &ActivityCtx) {}

    fn text_focus(&self, _ctx: &ActivityCtx) -> bool {
        false
    }

    fn has_panel(&self, _ctx: &ActivityCtx) -> bool {
        false
    }

    fn log_blocked(&self) -> Option<String> {
        self.state.contact_prefill().err()
    }

    fn log_hints(&self, _mode: &RigMode) -> (Option<LogMode>, LogDefaults) {
        let Ok(prefill) = self.state.contact_prefill() else {
            return (
                None,
                LogDefaults {
                    role: Role::Satellite,
                    ..LogDefaults::default()
                },
            );
        };
        let name = self.sat_name(&prefill.satellite);
        let extras =
            logbook::satellite::extras(&name, prefill.tx.hz, prefill.rx.hz).unwrap_or_default();
        // One report for both directions: 599 only when CW is worked both ways.
        let rst = if prefill.tx.mode == SatMode::Cw && prefill.rx.mode == SatMode::Cw {
            "599"
        } else {
            "59"
        };
        let mode = log_mode(prefill.tx.mode);
        (
            Some(mode.clone()),
            LogDefaults {
                mode: Some(mode),
                rst: Some(rst.to_string()),
                extra_adif: extras,
                role: Role::Satellite,
                ..LogDefaults::default()
            },
        )
    }

    fn hint(&self, _ctx: &ActivityCtx) -> &'static str {
        "j/k move  Tab switch  t track  [ ] { } nudge  Z SAT off  u unsupported  r refresh\nf freq  m/M mode  s split  l log  L list  c config  Ctrl-G go to  Ctrl-C quit"
    }

    fn render(&self, frame: &mut Frame, area: Rect, ctx: &ActivityCtx) {
        ui::render(&self.state, &self.footer(ctx.utc), area, frame);
    }

    fn tuning_blocked(&self) -> Option<&'static str> {
        self.state.tracking().map(|_| "Doppler tracking")
    }

    fn state_line(&self, ctx: &ActivityCtx) -> String {
        if let Some(tracking) = self.state.tracking() {
            return format!("tracking {}", tracking.satellite);
        }
        match self.state.next_pass(ctx.utc) {
            Some(row) if row.pass.aos > ctx.utc => format!(
                "next pass {} in {}",
                short_name(&row.name),
                format_countdown(row.pass.aos - ctx.utc)
            ),
            Some(row) => format!("{} is up", short_name(&row.name)),
            None => "no passes listed".to_string(),
        }
    }

    fn quit_warnings(&self) -> Vec<String> {
        if self.state.tracking().is_some() {
            vec!["Doppler tracking is running; it will stop.".to_string()]
        } else {
            Vec::new()
        }
    }

    fn on_leave(&mut self) -> Option<String> {
        self.last_step = None;
        self.want_refresh = true;
        self.state.stop_tracking().then(|| {
            "Doppler tracking stopped: left Satellite (the radio keeps its settings)".to_string()
        })
    }

    fn on_event(&mut self, event: &HostEvent) -> Option<String> {
        match event {
            HostEvent::SatelliteDownloading(on) => {
                self.downloading = *on;
                None
            }
            HostEvent::SatelliteData(data) => {
                if self.state.tracking().is_some() {
                    // Rebuilding would drop the tracked pass; wait until tracking stops.
                    self.pending_data = Some((**data).clone());
                } else {
                    self.data = (**data).clone();
                    self.pending_data = None;
                    self.rebuild_due = true;
                }
                None
            }
        }
    }
}
