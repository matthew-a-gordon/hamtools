//! Rig profiles on `App`: the Active rig, switching, the light poll of the inactive rig and the
//! rig-related settings. A child of `app` so it can reach `App`'s private readings.

use std::mem;
use std::time::Instant;

use hamtools_config::{Callsign, Grid, Settings};

use super::{App, Reading};
use crate::rig::{Command, RigEvent};
use crate::rigs::{
    Capabilities, Capability, LIGHT_POLL_EVERY, RigState, Switcher, SwitcherOutcome, SwitcherRow,
};

use ratatui::crossterm::event::KeyEvent;

impl App {
    /// The Active rig's profile name; the stand-in rig is named after its address.
    pub(crate) fn rig_name(&self) -> &str {
        &self.rigs[self.active].name
    }

    /// The Active rig's `rigctld` address.
    pub fn rig_addr(&self) -> &str {
        &self.rigs[self.active].addr
    }

    /// Every rig's `rigctld` address, in profile order. `main` keeps one connection per entry.
    pub fn rig_addrs(&self) -> Vec<String> {
        self.rigs.iter().map(|r| r.addr.clone()).collect()
    }

    /// The index (into [`App::rig_addrs`]) of the Active rig. Ordinary commands go to it.
    pub fn active_rig(&self) -> usize {
        self.active
    }

    /// What the Active rig can do, from the capability table and the profile's overrides.
    pub fn capabilities(&self) -> Capabilities {
        self.rigs[self.active].caps.clone()
    }

    /// Whether a rig-switch is open on screen.
    pub(crate) fn switcher_open(&self) -> bool {
        self.switcher.is_some()
    }

    /// Why the Activity at `index` cannot run on the Active rig, or `None` when it can.
    pub fn activity_unavailable(&self, index: usize) -> Option<String> {
        let activity = self.activities.get(index)?;
        let caps = self.capabilities();
        activity
            .requires()
            .iter()
            .find(|c| !caps.has(**c))
            .map(|c| {
                format!(
                    "{} needs {}, which {} lacks",
                    activity.name(),
                    c.label(),
                    self.rig_name()
                )
            })
    }

    /// Whether the Active rig is, or may be, transmitting: PTT on, or a CW send not yet drained.
    /// A rig that is not connected cannot be transmitting as far as the app can tell, so a dead
    /// radio never blocks a switch.
    pub fn transmitting(&self) -> bool {
        self.connected && (self.ptt() == Some(true) || self.activities[self.foreground].sending())
    }

    /// What the app is using now, as config settings.
    pub(super) fn current_settings(&self) -> Settings {
        let rigs = if self.implicit_rig {
            Vec::new()
        } else {
            self.rigs
                .iter()
                .map(|slot| hamtools_config::RigProfile {
                    name: slot.name.clone(),
                    addr: slot.addr.clone(),
                    kind: slot.kind,
                    cw: None,
                    split: None,
                    sat_mode: None,
                    bands: None,
                })
                .collect()
        };
        Settings {
            callsign: self.callsign.as_deref().and_then(|c| Callsign::new(c).ok()),
            locator: self.locator.as_deref().and_then(|g| Grid::new(g).ok()),
            log_path: self.log_path.clone(),
            export_dir: self.export_dir.clone(),
            license_class: self.license_class,
            rigs,
            rig_default: self.rig_default.clone(),
            macros: self.soft_keys.clone(),
            softkey_sets: self.softkey_sets.clone(),
            // Read at start-up by `main`; the dialog neither shows nor writes it.
            satellite: hamtools_config::SatelliteSettings::default(),
            reference: hamtools_config::ReferenceSettings::default(),
        }
    }

    /// Use saved settings now, with no restart. Profiles can be edited (address) but not added
    /// or removed here. Returns what the rig side must do.
    pub(super) fn apply_settings(&mut self, settings: Settings) -> Vec<Command> {
        self.callsign = settings.callsign.map(|c| c.as_str().to_string());
        self.locator = settings.locator.map(|g| g.as_str().to_string());
        self.log_path = settings.log_path;
        self.license_class = settings.license_class;
        self.soft_keys = settings.macros;
        self.softkey_sets = settings.softkey_sets;
        self.refresh_soft_keys();
        self.rig_default = settings.rig_default;
        let mut commands = Vec::new();
        for index in 0..self.rigs.len() {
            let Some(profile) = settings
                .rigs
                .iter()
                .find(|p| p.name == self.rigs[index].name)
            else {
                continue;
            };
            self.rigs[index].caps = Capabilities::for_profile(profile);
            self.rigs[index].kind = profile.kind;
            if profile.addr == self.rigs[index].addr {
                continue;
            }
            self.rigs[index].addr = profile.addr.clone();
            if index == self.active {
                self.connected = false;
                self.last_connect = None;
            } else {
                self.rigs[index].state.connected = false;
                self.rigs[index].state.last_light = None;
            }
            commands.push(Command::SetRigAddr {
                rig: index,
                addr: profile.addr.clone(),
            });
        }
        commands
    }

    /// Exchange the Active rig's readings with `other` (the parked state of another rig).
    fn swap_rig_state(&mut self, other: &mut RigState) {
        mem::swap(&mut self.connected, &mut other.connected);
        mem::swap(&mut self.freq, &mut other.freq);
        mem::swap(&mut self.mode, &mut other.mode);
        mem::swap(&mut self.vfo, &mut other.vfo);
        mem::swap(&mut self.split, &mut other.split);
        mem::swap(&mut self.split_freq, &mut other.split_freq);
        mem::swap(&mut self.ptt, &mut other.ptt);
        mem::swap(&mut self.strength, &mut other.strength);
        mem::swap(&mut self.unsupported, &mut other.unsupported);
        mem::swap(&mut self.step, &mut other.step);
        mem::swap(&mut self.last_connect, &mut other.last_connect);
        mem::swap(&mut self.last_poll, &mut other.last_poll);
        mem::swap(&mut self.last_smeter, &mut other.last_smeter);
    }

    /// Make rig `to` the Active rig, keeping each rig's own state. Refused while transmitting.
    /// An Activity the new rig cannot run drops to General, with a status line saying so.
    pub fn switch_rig(&mut self, to: usize) {
        if to >= self.rigs.len() || to == self.active {
            return;
        }
        if self.transmitting() {
            self.status = Some(format!(
                "cannot switch to {} while {} is transmitting",
                self.rigs[to].name,
                self.rig_name()
            ));
            return;
        }
        let from = self.active;
        let mut parked = mem::replace(&mut self.rigs[to].state, RigState::new(0));
        self.swap_rig_state(&mut parked);
        parked.last_light = Some(self.now);
        self.rigs[from].state = parked;
        self.active = to;
        self.entry = None;
        self.poll_failed = false;
        for activity in &mut self.activities {
            activity.switch_rig(from, to);
        }
        self.status = Some(format!("rig: {}", self.rig_name()));
        if let Some(why) = self.activity_unavailable(self.foreground) {
            let stopped = self.activities[self.foreground].on_leave();
            self.doppler_stopped |= stopped.is_some();
            self.foreground = 0;
            self.status = Some(match stopped {
                Some(note) => format!("{why}; back to General. {note}"),
                None => format!("{why}; back to General"),
            });
        }
    }

    /// An event from the light poll of rig `rig`. The Active rig's are ordinary rig events.
    pub fn on_rig_at(&mut self, rig: usize, event: RigEvent) {
        if rig == self.active {
            self.on_rig(event);
        } else if let Some(slot) = self.rigs.get_mut(rig) {
            slot.state.apply(event, self.now);
        }
    }

    /// The inactive rigs due for a light poll (health, frequency and mode), about every 5 s.
    pub(super) fn tick_inactive(&mut self, now: Instant) -> Vec<Command> {
        let active = self.active;
        self.rigs
            .iter_mut()
            .enumerate()
            .filter(|(i, slot)| {
                *i != active
                    && slot
                        .state
                        .last_light
                        .is_none_or(|at| now.duration_since(at) >= LIGHT_POLL_EVERY)
            })
            .map(|(i, slot)| {
                slot.state.last_light = Some(now);
                Command::LightPoll(i)
            })
            .collect()
    }

    pub(super) fn open_switcher(&mut self) {
        self.switcher = Some(Switcher::open(self.active, self.rigs.len()));
    }

    /// A key while the switcher is open.
    pub(super) fn on_switcher_key(&mut self, key: KeyEvent) {
        let Some(switcher) = &mut self.switcher else {
            return;
        };
        match switcher.on_key(key) {
            SwitcherOutcome::Stay => {}
            SwitcherOutcome::Close => self.switcher = None,
            SwitcherOutcome::Select(index) => {
                self.switcher = None;
                self.switch_rig(index);
            }
        }
    }

    pub(super) fn render_switcher(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let Some(switcher) = &self.switcher else {
            return;
        };
        let rows: Vec<SwitcherRow> = self
            .rigs
            .iter()
            .enumerate()
            .map(|(i, slot)| {
                let active = i == self.active;
                let (connected, freq, mode) = if active {
                    (self.connected, &self.freq, &self.mode)
                } else {
                    (slot.state.connected, &slot.state.freq, &slot.state.mode)
                };
                SwitcherRow {
                    name: slot.name.clone(),
                    kind: slot.kind,
                    addr: slot.addr.clone(),
                    connected,
                    active,
                    frequency: freq.as_ref().map(|r: &Reading<u64>| r.value),
                    mode: mode.as_ref().map(|r| r.value.0.name().to_string()),
                    caps: slot.caps.clone(),
                }
            })
            .collect();
        switcher.render(frame, area, &rows);
    }

    /// The capabilities an Activity needs, checked against the Active rig (for tests and the
    /// palette).
    pub fn rig_has(&self, capability: Capability) -> bool {
        self.capabilities().has(capability)
    }
}
