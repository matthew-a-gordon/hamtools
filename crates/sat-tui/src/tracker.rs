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
const STEP_HZ: f64 = 10.0;

fn to_radio_step(hz: f64) -> u64 {
    ((hz / STEP_HZ).round() * STEP_HZ) as u64
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
    sent: Sent,
    last_plan: Option<Plan>,
}

impl Tracker {
    pub fn new(sat: Satellite, observer: Observer) -> Self {
        Self {
            sat,
            observer,
            sent: Sent::default(),
            last_plan: None,
        }
    }

    /// Commands needed at `now` to follow `plan`, Doppler-corrected. The first step sets up the
    /// radio whatever the elevation. After that, frequencies are only updated while the satellite
    /// is above the horizon, though a change of plan (another transmitter, a nudge) is always
    /// applied.
    pub fn step(
        &mut self,
        plan: &Plan,
        now: DateTime<Utc>,
    ) -> Result<Vec<RigCommand>, PropagationError> {
        let look = self.sat.look_angle(&self.observer, now)?;
        let plan_changed = self.last_plan.as_ref() != Some(plan);
        if !plan_changed && look.elevation_deg < 0.0 {
            return Ok(Vec::new());
        }

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

        let rx = to_radio_step(downlink_hz(plan.downlink_hz as f64, look.range_rate_km_s));
        if self.sent.rx_hz != Some(rx) {
            commands.push(RigCommand::RxFreq(rx));
            self.sent.rx_hz = Some(rx);
        }
        if let Some(nominal) = plan.uplink_hz {
            let tx = to_radio_step(uplink_hz(nominal as f64, look.range_rate_km_s));
            if self.sent.tx_hz != Some(tx) {
                commands.push(RigCommand::TxFreq(tx));
                self.sent.tx_hz = Some(tx);
            }
        }

        self.last_plan = Some(plan.clone());
        Ok(commands)
    }
}
