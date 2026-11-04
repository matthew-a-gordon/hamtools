//! Terminal UI state: the pass list, which pass and transmitter are selected, and tracking.
//! No I/O; the event loop feeds it keys and the clock.

use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use orbit::{Observer, Pass, PropagationError, Satellite};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::logging::FormEvent;
pub use crate::logging::{ContactForm, Field, LoggedContact, Logging};
use crate::satnogs::{Plan, Transmitter};
use crate::tracker::{RigCommand, STEP_HZ, Tracker, doppler_range_rate};

/// Tunables that shape the pass list.
#[derive(Debug, Clone)]
pub struct Settings {
    /// NORAD ids to show; empty means every satellite with a usable transmitter.
    pub watch: Vec<u32>,
    pub horizon_hours: u32,
    pub min_elevation_deg: f64,
    /// A pass shorter than this many seconds is flagged as short.
    pub short_pass_secs: i64,
}

/// What the event loop must do in response to a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Quit,
    /// Download fresh TLEs and transmitter data.
    Refresh,
}

/// One line of the pass list.
#[derive(Debug, Clone)]
pub struct Row {
    pub name: String,
    pub norad_id: u32,
    pub pass: Pass,
    /// Shorter than the configured threshold: easy to miss, worth a warning.
    pub short: bool,
    sat_index: usize,
}

/// One entry of the transmitter picker.
#[derive(Debug, Clone)]
pub struct Choice {
    pub description: String,
    /// Whether the FT-847 can demodulate it.
    pub supported: bool,
    /// What the rig would be tuned to at the passband centre; `None` when unsupported.
    pub plan: Option<Plan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Passes,
    Transmitters,
}

struct Entry {
    sat: Satellite,
    name: String,
    /// Alive and active transmitters, in SatNOGS order.
    transmitters: Vec<Transmitter>,
}

pub struct App {
    entries: Vec<Entry>,
    rows: Vec<Row>,
    focus: Focus,
    row: usize,
    transmitter: usize,
    show_unsupported: bool,
    observer: Observer,
    min_elevation_deg: f64,
    status: Option<String>,
    tracking: Option<Tracking>,
    nudge_hz: i64,
    /// Commands queued by keys, sent on the next tick.
    pending: Vec<RigCommand>,
    /// `None` when there is no callsign: logging is off.
    logging: Option<Logging>,
    rig_live: bool,
    /// The latest time the event loop has told us, for stamping Contacts.
    clock: DateTime<Utc>,
    form: Option<ContactForm>,
    session: Vec<LoggedContact>,
    /// `Some(n)` while the session list is open, scrolled `n` lines back from the newest.
    session_scroll: Option<usize>,
}

impl App {
    pub fn new(
        sats: Vec<Satellite>,
        transmitters: Vec<Transmitter>,
        observer: Observer,
        settings: Settings,
        now: DateTime<Utc>,
    ) -> Self {
        let watch: HashSet<u32> = settings.watch.iter().copied().collect();
        let until = now + Duration::hours(i64::from(settings.horizon_hours));
        let mut entries = Vec::new();
        let mut rows = Vec::new();
        for sat in sats {
            let norad_id = sat.norad_id() as u32;
            if !watch.is_empty() && !watch.contains(&norad_id) {
                continue;
            }
            let usable: Vec<Transmitter> = transmitters
                .iter()
                .filter(|t| t.norad_id == norad_id && t.is_usable())
                .cloned()
                .collect();
            if !usable.iter().any(|t| t.plan(0).is_some()) {
                continue;
            }
            let Ok(passes) = sat.passes(&observer, now, until, settings.min_elevation_deg) else {
                continue;
            };
            let sat_index = entries.len();
            let short_below = Duration::seconds(settings.short_pass_secs);
            rows.extend(passes.into_iter().map(|pass| Row {
                name: sat.name().to_string(),
                norad_id,
                short: pass.los - pass.aos < short_below,
                pass,
                sat_index,
            }));
            entries.push(Entry {
                name: sat.name().to_string(),
                sat,
                transmitters: usable,
            });
        }
        rows.sort_by_key(|row| row.pass.aos);
        Self {
            entries,
            rows,
            focus: Focus::Passes,
            row: 0,
            transmitter: 0,
            show_unsupported: false,
            observer,
            min_elevation_deg: settings.min_elevation_deg,
            status: None,
            tracking: None,
            nudge_hz: 0,
            pending: Vec::new(),
            logging: None,
            rig_live: false,
            clock: now,
            form: None,
            session: Vec::new(),
            session_scroll: None,
        }
    }

    pub fn rows(&self) -> Vec<Row> {
        self.rows.clone()
    }

    pub fn selected_row(&self) -> Option<usize> {
        (!self.rows.is_empty()).then_some(self.row)
    }

    pub fn selected_transmitter(&self) -> Option<usize> {
        (!self.transmitter_choices().is_empty()).then_some(self.transmitter)
    }

    /// Transmitters of the selected pass's satellite worth offering.
    pub fn transmitter_choices(&self) -> Vec<Choice> {
        self.choice_items()
            .into_iter()
            .map(|(t, plan)| Choice {
                description: t.description.clone(),
                supported: plan.is_some(),
                plan,
            })
            .collect()
    }

    /// The picker's contents: each shown transmitter with its centre plan, if the radio has one.
    fn choice_items(&self) -> Vec<(&Transmitter, Option<Plan>)> {
        let Some(row) = self.rows.get(self.row) else {
            return Vec::new();
        };
        self.entries[row.sat_index]
            .transmitters
            .iter()
            .map(|t| (t, t.plan(0)))
            .filter(|(_, plan)| plan.is_some() || self.show_unsupported)
            .collect()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Command> {
        if self.form.is_some() {
            self.handle_form_key(key);
            return None;
        }
        if self.session_scroll.is_some() {
            match key.code {
                KeyCode::Char('q') => return Some(Command::Quit),
                _ => self.handle_session_key(key),
            }
            return None;
        }
        match key.code {
            KeyCode::Char('l') => self.open_form(),
            KeyCode::Char('L') => self.session_scroll = Some(0),
            KeyCode::Char('q') | KeyCode::Esc => return Some(Command::Quit),
            KeyCode::Char('r') => return Some(Command::Refresh),
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Passes => Focus::Transmitters,
                    Focus::Transmitters => Focus::Passes,
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
            KeyCode::Char('t') => self.toggle_tracking(),
            KeyCode::Char('s') => self.pending.push(RigCommand::SatMode(false)),
            KeyCode::Char(c @ ('[' | ']' | '{' | '}')) if self.tracking.is_some() => {
                self.nudge_hz += match c {
                    ']' => 1_000,
                    '[' => -1_000,
                    '}' => 100,
                    _ => -100,
                };
            }
            KeyCode::Char('m') => {
                self.show_unsupported = !self.show_unsupported;
                let last = self.transmitter_choices().len().saturating_sub(1);
                self.transmitter = self.transmitter.min(last);
            }
            _ => {}
        }
        None
    }

    fn move_cursor(&mut self, delta: isize) {
        match self.focus {
            Focus::Passes => {
                let last = self.rows.len().saturating_sub(1);
                let moved = self.row.saturating_add_signed(delta).min(last);
                if moved != self.row {
                    self.row = moved;
                    self.transmitter = 0;
                }
            }
            Focus::Transmitters => {
                let last = self.transmitter_choices().len().saturating_sub(1);
                self.transmitter = self.transmitter.saturating_add_signed(delta).min(last);
            }
        }
    }
}

struct Tracking {
    tracker: Tracker,
    transmitter: Transmitter,
    satellite: String,
    /// Index into `entries`, to find this satellite's passes.
    sat_index: usize,
}

/// What is being tracked, for the display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackingView {
    pub satellite: String,
    pub transmitter: String,
    /// Offset from the passband centre in Hz.
    pub nudge_hz: i64,
}

impl App {
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn set_status(&mut self, status: Option<String>) {
        self.status = status;
    }

    /// Call after the rig connection is restored: tracking resends everything on the next tick.
    pub fn resync(&mut self) {
        if let Some(tracking) = &mut self.tracking {
            tracking.tracker.reset();
        }
    }

    /// Fold a turn of the radio's dial into the nudge. `rx_hz` is the receive frequency read back
    /// from the rig; anything but a step away from what tracking last sent means the operator
    /// moved the dial (the FT-847 moves its transmit VFO with it), so tracking carries on from
    /// there instead of overwriting it. Only transponders with a passband have anything to nudge.
    pub fn follow_dial(&mut self, rx_hz: u64) {
        let Some(tracking) = &self.tracking else {
            return;
        };
        let Some(sent) = tracking.tracker.sent_rx_hz() else {
            return;
        };
        if tracking.transmitter.downlink.high_hz.is_none() {
            return;
        }
        let turned = rx_hz as i64 - sent as i64;
        if turned.unsigned_abs() as f64 >= STEP_HZ {
            self.nudge_hz += turned;
        }
    }

    pub fn tracking(&self) -> Option<TrackingView> {
        self.tracking.as_ref().map(|t| TrackingView {
            satellite: t.satellite.clone(),
            transmitter: t.transmitter.description.clone(),
            nudge_hz: self.nudge_hz,
        })
    }

    /// The transmitter the cursor is on, if the radio can use it.
    fn chosen_transmitter(&self) -> Option<(&Entry, &Transmitter)> {
        let entry = &self.entries[self.rows.get(self.row)?.sat_index];
        let choices = self.choice_items();
        let (transmitter, plan) = choices.get(self.transmitter)?;
        plan.as_ref()?;
        Some((entry, *transmitter))
    }

    fn toggle_tracking(&mut self) {
        if self.tracking.take().is_some() {
            self.status = Some("tracking stopped".to_string());
            return;
        }
        let Some((entry, transmitter)) = self.chosen_transmitter() else {
            self.status = Some("pick a supported transmitter first".to_string());
            return;
        };
        let sat_index = self.rows[self.row].sat_index;
        let tracking = Tracking {
            tracker: Tracker::new(entry.sat.clone(), self.observer, self.min_elevation_deg),
            transmitter: transmitter.clone(),
            satellite: entry.name.clone(),
            sat_index,
        };
        self.status = Some(format!("tracking {}", tracking.transmitter.description));
        self.nudge_hz = 0;
        self.tracking = Some(tracking);
    }

    /// Advance to `now`: queued commands first, then whatever tracking needs. The caller applies
    /// the returned commands to the rig.
    pub fn tick(&mut self, now: DateTime<Utc>) -> Result<Vec<RigCommand>, PropagationError> {
        self.clock = now;
        let mut commands = std::mem::take(&mut self.pending);
        if let Some(tracking) = &mut self.tracking
            && let Some(plan) = tracking.transmitter.plan(self.nudge_hz)
        {
            commands.extend(tracking.tracker.step(&plan, now)?);
        }
        Ok(commands)
    }
}

/// Where the selected satellite is right now, and what the radio would be tuned to for the
/// transmitter in focus (the tracked one, else the one under the cursor).
#[derive(Debug, Clone)]
pub struct Live {
    pub azimuth_deg: f64,
    pub elevation_deg: f64,
    pub range_km: f64,
    pub range_rate_km_s: f64,
    /// Whether rx_hz and tx_hz carry Doppler correction: only from AOS (the minimum elevation).
    pub doppler_corrected: bool,
    /// On the radio's 10 Hz grid; `None` with no usable transmitter in focus.
    pub rx_hz: Option<u64>,
    pub tx_hz: Option<u64>,
    pub plan: Option<Plan>,
}

impl App {
    /// `None` when there are no passes or the TLE cannot be propagated to `now`.
    pub fn live(&self, now: DateTime<Utc>) -> Option<Live> {
        let row = self.rows.get(self.row)?;
        let entry = &self.entries[row.sat_index];
        let look = entry.sat.look_angle(&self.observer, now).ok()?;
        let plan = match &self.tracking {
            Some(t) => t.transmitter.plan(self.nudge_hz),
            None => self.chosen_transmitter().and_then(|(_, t)| t.plan(0)),
        };
        let step = |hz: f64| ((hz / 10.0).round() * 10.0) as u64;
        let doppler_corrected = look.elevation_deg >= self.min_elevation_deg;
        let corrected_rate = doppler_range_rate(
            look.elevation_deg,
            self.min_elevation_deg,
            look.range_rate_km_s,
        );
        Some(Live {
            doppler_corrected,
            azimuth_deg: look.azimuth_deg,
            elevation_deg: look.elevation_deg,
            range_km: look.range_km,
            range_rate_km_s: look.range_rate_km_s,
            rx_hz: plan
                .as_ref()
                .map(|p| step(orbit::downlink_hz(p.downlink_hz as f64, corrected_rate))),
            tx_hz: plan
                .as_ref()
                .and_then(|p| p.uplink_hz)
                .map(|hz| step(orbit::uplink_hz(hz as f64, corrected_rate))),
            plan,
        })
    }

    /// Whether the cursor is in the transmitter picker rather than the pass list.
    pub fn picker_focused(&self) -> bool {
        self.focus == Focus::Transmitters
    }

    /// The pass under the cursor.
    pub fn selected_pass(&self) -> Option<&Row> {
        self.rows.get(self.row)
    }
}

const NO_LOGGING: &str = "logging is off: set callsign in the shared config (config.toml)";

impl App {
    /// `None` turns logging off (no callsign).
    pub fn set_logging(&mut self, logging: Option<Logging>) {
        self.logging = logging;
    }

    pub fn logging_enabled(&self) -> bool {
        self.logging.is_some()
    }

    /// Whether the event loop currently has a working connection to the rig.
    pub fn set_rig_live(&mut self, live: bool) {
        self.rig_live = live;
    }

    /// The time the Contact stamps use: tell the app on every turn of the event loop.
    pub fn set_clock(&mut self, now: DateTime<Utc>) {
        self.clock = now;
    }

    pub fn form(&self) -> Option<&ContactForm> {
        self.form.as_ref()
    }

    /// This session's Contacts, oldest first.
    pub fn session(&self) -> &[LoggedContact] {
        &self.session
    }

    /// Hand the session list over, so it can survive the app being rebuilt.
    pub fn take_session(&mut self) -> Vec<LoggedContact> {
        std::mem::take(&mut self.session)
    }

    pub fn restore_session(&mut self, session: Vec<LoggedContact>) {
        self.session = session;
    }

    pub fn session_open(&self) -> bool {
        self.session_scroll.is_some()
    }

    /// Lines the session list is scrolled back from the newest Contact.
    pub fn session_scroll(&self) -> usize {
        self.session_scroll.unwrap_or(0)
    }

    /// The receive frequency the tracker last sent to the radio.
    pub fn radio_now_rx_hz(&self) -> Option<u64> {
        self.tracking.as_ref()?.tracker.sent_rx_hz()
    }

    /// The operator's own locator for the Contact: the config override, else the observer's.
    pub fn own_grid(&self) -> String {
        self.logging
            .as_ref()
            .and_then(|l| l.grid_override.clone())
            .unwrap_or_else(|| logbook::locator(self.observer.lat_deg, self.observer.lon_deg))
    }

    fn open_form(&mut self) {
        let Some(logging) = &self.logging else {
            self.status = Some(NO_LOGGING.to_string());
            return;
        };
        let Some(tracking) = &self.tracking else {
            self.status = Some("start tracking (t) before logging a Contact".to_string());
            return;
        };
        if !self.rig_live {
            self.status = Some("no rig connected: cannot log a Contact".to_string());
            return;
        }
        let (Some(rx_hz), Some(tx_hz), Some(plan)) = (
            tracking.tracker.sent_rx_hz(),
            tracking.tracker.sent_tx_hz(),
            tracking.transmitter.plan(self.nudge_hz),
        ) else {
            self.status = Some("no frequencies sent to the radio yet: wait a tick".to_string());
            return;
        };
        let Some(tx_mode) = plan.tx_mode else {
            self.status = Some("this transmitter has no uplink: nothing to log".to_string());
            return;
        };
        if let Err(e) = logging.log.check() {
            self.status = Some(format!("log: {e}"));
            return;
        }
        self.form = Some(ContactForm::new(
            tracking.satellite.clone(),
            tracking.transmitter.description.clone(),
            self.clock,
            (rx_hz, plan.rx_mode),
            (tx_hz, tx_mode),
        ));
    }

    fn handle_form_key(&mut self, key: KeyEvent) {
        let Some(form) = &mut self.form else {
            return;
        };
        match form.handle_key(key) {
            FormEvent::Edited => {}
            FormEvent::Cancel => self.form = None,
            FormEvent::Save => self.save_form(),
        }
    }

    fn save_form(&mut self) {
        let own_grid = self.own_grid();
        let (Some(form), Some(logging)) = (&mut self.form, &self.logging) else {
            return;
        };
        let sat_name = logging.sat_name(&form.satellite).to_string();
        let contact = match form.contact(self.clock, own_grid, &sat_name) {
            Ok(contact) => contact,
            Err(message) => {
                form.error = Some(message);
                return;
            }
        };
        if let Err(e) = logging.log.append(&contact) {
            form.error = None;
            self.status = Some(format!("not logged: {e}"));
            return;
        }
        let entry = LoggedContact {
            time_on: contact.start(),
            call: contact.call().to_string(),
            grid: contact.grid().unwrap_or_default().to_string(),
            rst_sent: contact.rst_sent().unwrap_or_default().to_string(),
            rst_rcvd: contact.rst_rcvd().unwrap_or_default().to_string(),
            satellite: sat_name,
        };
        self.status = Some(format!("logged {}", entry.call));
        self.session.push(entry);
        self.form = None;
    }

    fn handle_session_key(&mut self, key: KeyEvent) {
        let Some(scroll) = self.session_scroll else {
            return;
        };
        let oldest = self.session.len().saturating_sub(1);
        match key.code {
            KeyCode::Char('L') | KeyCode::Esc => self.session_scroll = None,
            KeyCode::Up | KeyCode::Char('k') => {
                self.session_scroll = Some((scroll + 1).min(oldest))
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.session_scroll = Some(scroll.saturating_sub(1));
            }
            _ => {}
        }
    }
}

impl Row {
    /// The pass duration: AOS to LOS, however far through it we are.
    pub fn duration(&self) -> Duration {
        self.pass.los - self.pass.aos
    }
}

/// Time until the tracked satellite's current or next pass starts and ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Countdown {
    /// The pass has not started: time to AOS and to LOS.
    Before { aos_in: Duration, los_in: Duration },
    /// The pass is under way: time to LOS.
    During { los_in: Duration },
    /// The pass list holds no pass for this satellite from `now` on.
    NoFurtherPass,
}

impl App {
    /// Countdown for the tracked satellite's current pass, or its next one if none is under way.
    /// Independent of where the cursor is; `None` when nothing is tracked.
    pub fn tracked_countdown(&self, now: DateTime<Utc>) -> Option<Countdown> {
        let sat_index = self.tracking.as_ref()?.sat_index;
        let next = self
            .rows
            .iter()
            .filter(|row| row.sat_index == sat_index && row.pass.los >= now)
            .min_by_key(|row| row.pass.aos);
        Some(match next {
            None => Countdown::NoFurtherPass,
            Some(row) if row.pass.aos > now => Countdown::Before {
                aos_in: row.pass.aos - now,
                los_in: row.pass.los - now,
            },
            Some(row) => Countdown::During {
                los_in: row.pass.los - now,
            },
        })
    }
}
