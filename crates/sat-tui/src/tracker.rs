//! Turns "this satellite, this transmitter, now" into the rig commands that keep the radio tuned.
//!
//! Pure planning: it emits [`RigCommand`]s and remembers what it last asked for, so each step
//! only sends what changed. Applying commands to a real rig is somebody else's job. It never
//! produces a command that keys the transmitter.

use chrono::{DateTime, Utc};
use orbit::{Observer, PropagationError, Satellite, downlink_hz, uplink_hz};

use crate::satnogs::{Plan, RigMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigCommand {
    /// Switch the FT-847's satellite mode on or off.
    SatMode(bool),
    RxMode(RigMode),
    TxMode(RigMode),
    RxFreq(u64),
    TxFreq(u64),
}

/// The FT-847 tunes in 10 Hz steps.
pub const STEP_HZ: f64 = 10.0;

fn to_radio_step(hz: f64) -> u64 {
    ((hz / STEP_HZ).round() * STEP_HZ) as u64
}

/// The range rate to correct Doppler for: the real one from AOS (the minimum elevation), none
/// before it, where the radio sits on the plan's own frequencies and the operator tunes by hand.
pub fn doppler_range_rate(elevation_deg: f64, min_elevation_deg: f64, range_rate_km_s: f64) -> f64 {
    if elevation_deg >= min_elevation_deg {
        range_rate_km_s
    } else {
        0.0
    }
}

/// What has been sent to the radio so far.
#[derive(Default)]
struct Sent {
    sat_mode: bool,
    rx_mode: Option<RigMode>,
    tx_mode: Option<RigMode>,
    rx_hz: Option<u64>,
    tx_hz: Option<u64>,
}

pub struct Tracker {
    sat: Satellite,
    observer: Observer,
    min_elevation_deg: f64,
    sent: Sent,
}

impl Tracker {
    pub fn new(sat: Satellite, observer: Observer, min_elevation_deg: f64) -> Self {
        Self {
            sat,
            observer,
            min_elevation_deg,
            sent: Sent::default(),
        }
    }

    /// Commands needed at `now` to follow `plan`. Before AOS the radio sits on the plan's
    /// own frequencies; from AOS they are Doppler-corrected (see [`doppler_range_rate`]).
    pub fn step(
        &mut self,
        plan: &Plan,
        now: DateTime<Utc>,
    ) -> Result<Vec<RigCommand>, PropagationError> {
        let look = self.sat.look_angle(&self.observer, now)?;
        let range_rate_km_s = doppler_range_rate(
            look.elevation_deg,
            self.min_elevation_deg,
            look.range_rate_km_s,
        );

        let mut commands = Vec::new();
        if plan.uplink_hz.is_some() && !self.sent.sat_mode {
            commands.push(RigCommand::SatMode(true));
            self.sent.sat_mode = true;
        }
        if self.sent.rx_mode != Some(plan.rx_mode) {
            commands.push(RigCommand::RxMode(plan.rx_mode));
            self.sent.rx_mode = Some(plan.rx_mode);
        }
        if let Some(mode) = plan.tx_mode
            && self.sent.tx_mode != Some(mode)
        {
            commands.push(RigCommand::TxMode(mode));
            self.sent.tx_mode = Some(mode);
        }

        let rx = to_radio_step(downlink_hz(plan.downlink_hz as f64, range_rate_km_s));
        if self.sent.rx_hz != Some(rx) {
            commands.push(RigCommand::RxFreq(rx));
            self.sent.rx_hz = Some(rx);
        }
        if let Some(nominal) = plan.uplink_hz {
            let tx = to_radio_step(uplink_hz(nominal as f64, range_rate_km_s));
            if self.sent.tx_hz != Some(tx) {
                commands.push(RigCommand::TxFreq(tx));
                self.sent.tx_hz = Some(tx);
            }
        }

        Ok(commands)
    }
}

impl Tracker {
    /// The receive frequency last sent to the radio, if any.
    pub fn sent_rx_hz(&self) -> Option<u64> {
        self.sent.rx_hz
    }

    /// The transmit frequency last sent to the radio, if any.
    pub fn sent_tx_hz(&self) -> Option<u64> {
        self.sent.tx_hz
    }

    /// Forget what was sent, so the next step sets the radio up from scratch. For use after the
    /// rig connection has been lost and restored.
    pub fn reset(&mut self) {
        self.sent = Sent::default();
    }
}
