//! The Satellite screens' state: the pass list, which pass and transmitter are selected, and
//! tracking. No I/O; the Activity feeds it keys and the clock.

use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use hamtools_config::SatelliteSettings;
use orbit::{Observer, Pass, PropagationError, Satellite, downlink_hz, uplink_hz};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use sat_tui::satnogs::{Plan, RigMode, Transmitter};
use sat_tui::tracker::{RigCommand, STEP_HZ, Tracker, doppler_range_rate};

/// What the Activity must do beyond updating the screen in response to a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
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

pub struct SatState {
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
    /// What tracking was doing when it was stopped by leaving, so `t` on return picks it up again.
    resume: Option<Resume>,
    /// Commands queued by keys, sent on the next tick.
    pending: Vec<RigCommand>,
}

impl SatState {
    pub fn new(
        sats: Vec<Satellite>,
        transmitters: Vec<Transmitter>,
        observer: Observer,
        settings: &SatelliteSettings,
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
            resume: None,
            pending: Vec::new(),
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

    /// The plain keys the Satellite screens own. Up/Down and `j`/`k` move the cursor (the
    /// screens are unchanged from `sat-tui`); `Z` is sat mode off, `u` shows the transmitters
    /// the radio cannot demodulate (it was `m`, which is the shared mode key).
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Request> {
        match key.code {
            KeyCode::Char('r') => return Some(Request::Refresh),
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Passes => Focus::Transmitters,
                    Focus::Transmitters => Focus::Passes,
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
            KeyCode::Char('t') => self.toggle_tracking(),
            KeyCode::Char('Z') => self.pending.push(RigCommand::SatMode(false)),
            KeyCode::Char(c @ ('[' | ']' | '{' | '}')) => {
                if let Some(tracking) = &mut self.tracking {
                    tracking.nudge_hz += match c {
                        ']' => 1_000,
                        '[' => -1_000,
                        '}' => 100,
                        _ => -100,
                    };
                }
            }
            KeyCode::Char('u') => {
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

pub(super) struct Resume {
    sat_index: usize,
    transmitter: String,
    nudge_hz: i64,
}

struct Tracking {
    tracker: Tracker,
    transmitter: Transmitter,
    satellite: String,
    /// Index into `entries`, to find this satellite's passes.
    sat_index: usize,
    /// Offset from the passband centre in Hz, set by the operator.
    nudge_hz: i64,
}

impl Tracking {
    /// What the radio should be on now, nudge included.
    fn plan(&self) -> Option<Plan> {
        self.transmitter.plan(self.nudge_hz)
    }

    /// The receive frequency last sent to the radio.
    fn sent_rx_hz(&self) -> Option<u64> {
        self.tracker.sent_rx_hz()
    }

    /// How far the operator has turned the dial from what was last sent, if by a step or more.
    /// Only transponders with a passband have anything to nudge.
    fn dial_turn(&self, rx_hz: u64) -> Option<i64> {
        self.transmitter.downlink.high_hz?;
        let turned = rx_hz as i64 - self.sent_rx_hz()? as i64;
        (turned.unsigned_abs() as f64 >= STEP_HZ).then_some(turned)
    }

    /// What is on the air now, or why there is nothing to log.
    fn prefill(&self) -> Result<ContactPrefill, &'static str> {
        let (Some(rx_hz), Some(tx_hz), Some(plan)) =
            (self.sent_rx_hz(), self.tracker.sent_tx_hz(), self.plan())
        else {
            return Err("no frequencies sent to the radio yet: wait a tick");
        };
        let Some(tx_mode) = plan.tx_mode else {
            return Err("this transmitter has no uplink: nothing to log");
        };
        Ok(ContactPrefill {
            satellite: self.satellite.clone(),
            transmitter: self.transmitter.description.clone(),
            rx: Link {
                hz: rx_hz,
                mode: plan.rx_mode,
            },
            tx: Link {
                hz: tx_hz,
                mode: tx_mode,
            },
        })
    }
}

/// One side of the radio link: a frequency and the mode it is worked in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub hz: u64,
    pub mode: RigMode,
}

/// What a satellite Contact is prefilled with: the satellite, and the frequencies and modes last
/// sent to the radio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactPrefill {
    pub satellite: String,
    pub transmitter: String,
    pub rx: Link,
    pub tx: Link,
}

/// What is being tracked, for the display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackingView {
    pub satellite: String,
    pub transmitter: String,
    /// Offset from the passband centre in Hz.
    pub nudge_hz: i64,
}

impl SatState {
    pub(super) fn take_resume(&mut self) -> Option<Resume> {
        self.resume.take()
    }

    pub(super) fn put_resume(&mut self, resume: Option<Resume>) {
        self.resume = resume;
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// The message the last key left for the status line, if any (reading it clears it).
    pub fn take_status(&mut self) -> Option<String> {
        self.status.take()
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
        if let Some(tracking) = &mut self.tracking
            && let Some(turned) = tracking.dial_turn(rx_hz)
        {
            tracking.nudge_hz += turned;
        }
    }

    pub fn tracking(&self) -> Option<TrackingView> {
        self.tracking.as_ref().map(|t| TrackingView {
            satellite: t.satellite.clone(),
            transmitter: t.transmitter.description.clone(),
            nudge_hz: t.nudge_hz,
        })
    }

    /// Stop tracking (if it is running) and drop commands not yet sent. Does not touch the radio.
    pub fn stop_tracking(&mut self) -> bool {
        self.pending.clear();
        let stopped = self.tracking.take();
        if let Some(t) = &stopped {
            self.resume = Some(Resume {
                sat_index: t.sat_index,
                transmitter: t.transmitter.description.clone(),
                nudge_hz: t.nudge_hz,
            });
        }
        stopped.is_some()
    }

    /// A line saying what `t` will pick up again, after tracking was stopped by leaving.
    pub fn resume_hint(&self) -> Option<String> {
        let r = self.resume.as_ref()?;
        let name = &self.entries.get(r.sat_index)?.name;
        Some(format!("t resumes {name} {}", r.transmitter))
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
        // The same transmitter on the same satellite carries on from the operator's nudge.
        let resumed_nudge = self
            .resume
            .as_ref()
            .filter(|r| r.sat_index == sat_index && r.transmitter == transmitter.description)
            .map_or(0, |r| r.nudge_hz);
        let tracking = Tracking {
            tracker: Tracker::new(entry.sat.clone(), self.observer, self.min_elevation_deg),
            transmitter: transmitter.clone(),
            satellite: entry.name.clone(),
            sat_index,
            nudge_hz: resumed_nudge,
        };
        self.status = Some(format!("tracking {}", tracking.transmitter.description));
        self.resume = None;
        self.tracking = Some(tracking);
    }

    /// Advance to `now`: queued commands first, then whatever tracking needs. The caller applies
    /// the returned commands to the rig.
    pub fn tick(&mut self, now: DateTime<Utc>) -> Result<Vec<RigCommand>, PropagationError> {
        let mut commands = std::mem::take(&mut self.pending);
        if let Some(tracking) = &mut self.tracking
            && let Some(plan) = tracking.plan()
        {
            commands.extend(tracking.tracker.step(&plan, now)?);
        }
        Ok(commands)
    }

    /// Whether keys have queued commands that wait for a rig.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// What a Contact logged now is prefilled with, or why nothing can be logged.
    pub fn contact_prefill(&self) -> Result<ContactPrefill, String> {
        let Some(tracking) = &self.tracking else {
            return Err("start tracking (t) before logging a Contact".to_string());
        };
        tracking.prefill().map_err(String::from)
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

impl SatState {
    /// `None` when there are no passes or the TLE cannot be propagated to `now`.
    pub fn live(&self, now: DateTime<Utc>) -> Option<Live> {
        let row = self.rows.get(self.row)?;
        let entry = &self.entries[row.sat_index];
        let look = entry.sat.look_angle(&self.observer, now).ok()?;
        let plan = match &self.tracking {
            Some(t) => t.plan(),
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
                .map(|p| step(downlink_hz(p.downlink_hz as f64, corrected_rate))),
            tx_hz: plan
                .as_ref()
                .and_then(|p| p.uplink_hz)
                .map(|hz| step(uplink_hz(hz as f64, corrected_rate))),
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

    /// The next pass starting after `now`, for the palette's one-line state.
    pub fn next_pass(&self, now: DateTime<Utc>) -> Option<&Row> {
        self.rows
            .iter()
            .filter(|row| row.pass.los >= now)
            .min_by_key(|row| row.pass.aos)
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

impl SatState {
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
