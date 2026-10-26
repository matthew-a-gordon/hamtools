//! SatNOGS DB transmitter records: parsing, filtering, and planning what to tune the rig to.

use std::fmt;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Two-way FM (or similar) repeater: one uplink, one downlink.
    Transceiver,
    /// Linear transponder with passbands on both bands.
    Transponder,
    /// Downlink only: beacon, telemetry.
    Transmitter,
    Other,
}

/// One band of a transmitter: a single frequency, or a passband when `high_hz` is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Band {
    pub low_hz: u64,
    pub high_hz: Option<u64>,
    /// SatNOGS mode name, e.g. `FM`, `USB`, `CW`, `BPSK`.
    pub mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transmitter {
    pub uuid: String,
    pub norad_id: u32,
    pub description: String,
    pub kind: Kind,
    pub alive: bool,
    pub active: bool,
    /// Whether the transponder inverts sidebands (uplink low maps to downlink high).
    pub invert: bool,
    pub uplink: Option<Band>,
    pub downlink: Band,
}

/// Modes the FT-847 can demodulate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigMode {
    Fm,
    Usb,
    Lsb,
    Cw,
    Am,
}

impl RigMode {
    /// The mode name rigctld expects.
    pub fn name(self) -> &'static str {
        match self {
            Self::Fm => "FM",
            Self::Usb => "USB",
            Self::Lsb => "LSB",
            Self::Cw => "CW",
            Self::Am => "AM",
        }
    }

    /// Map a SatNOGS mode name; `None` if the radio cannot demodulate it.
    fn from_satnogs(name: &str) -> Option<Self> {
        match name {
            "FM" | "FMN" | "AFSK" => Some(Self::Fm),
            "USB" => Some(Self::Usb),
            "LSB" => Some(Self::Lsb),
            "CW" => Some(Self::Cw),
            "AM" => Some(Self::Am),
            _ => None,
        }
    }
}

/// What to tune the rig to, before Doppler correction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub downlink_hz: u64,
    /// `None` for receive-only transmitters.
    pub uplink_hz: Option<u64>,
    pub rx_mode: RigMode,
    pub tx_mode: Option<RigMode>,
}

#[derive(Debug)]
pub struct ParseError(String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bad SatNOGS data: {}", self.0)
    }
}

impl std::error::Error for ParseError {}

#[derive(Deserialize)]
struct Raw {
    uuid: String,
    description: String,
    norad_cat_id: Option<u32>,
    #[serde(rename = "type")]
    kind: Option<String>,
    alive: Option<bool>,
    status: Option<String>,
    invert: Option<bool>,
    mode: Option<String>,
    uplink_mode: Option<String>,
    uplink_low: Option<u64>,
    uplink_high: Option<u64>,
    downlink_low: Option<u64>,
    downlink_high: Option<u64>,
}

/// Parse a SatNOGS `/api/transmitters/` response. Records with no downlink frequency or no
/// NORAD id are of no use to a tracker and are skipped.
pub fn parse_transmitters(json: &str) -> Result<Vec<Transmitter>, ParseError> {
    let raw: Vec<Raw> = serde_json::from_str(json).map_err(|e| ParseError(e.to_string()))?;
    Ok(raw
        .into_iter()
        .filter_map(|r| {
            Some(Transmitter {
                uuid: r.uuid,
                norad_id: r.norad_cat_id?,
                description: r.description,
                kind: match r.kind.as_deref() {
                    Some("Transceiver") => Kind::Transceiver,
                    Some("Transponder") => Kind::Transponder,
                    Some("Transmitter") => Kind::Transmitter,
                    _ => Kind::Other,
                },
                alive: r.alive.unwrap_or(false),
                active: r.status.as_deref() == Some("active"),
                invert: r.invert.unwrap_or(false),
                uplink: r.uplink_low.map(|low_hz| Band {
                    low_hz,
                    high_hz: r.uplink_high,
                    mode: r.uplink_mode,
                }),
                downlink: Band {
                    low_hz: r.downlink_low?,
                    high_hz: r.downlink_high,
                    mode: r.mode,
                },
            })
        })
        .collect())
}

impl Transmitter {
    /// Worth offering to the operator: SatNOGS says it is alive and active.
    pub fn is_usable(&self) -> bool {
        self.alive && self.active
    }

    /// Frequencies and modes to tune, with `nudge_hz` applied along the downlink passband
    /// (clamped to its edges; ignored for single-frequency transmitters). `None` when the radio
    /// cannot demodulate the downlink, or the uplink needs a mode we do not know.
    pub fn plan(&self, nudge_hz: i64) -> Option<Plan> {
        let rx_mode = RigMode::from_satnogs(self.downlink.mode.as_deref()?)?;
        let (down_low, down_high) = (self.downlink.low_hz, self.downlink.high_hz);

        let downlink_hz = match down_high {
            Some(high) => {
                let centre = down_low + (high - down_low) / 2;
                centre.saturating_add_signed(nudge_hz).clamp(down_low, high)
            }
            None => down_low,
        };

        let (uplink_hz, tx_mode) = match &self.uplink {
            None => (None, None),
            Some(up) => {
                let tx_mode = RigMode::from_satnogs(up.mode.as_deref()?)?;
                let offset = downlink_hz - down_low;
                let hz = match (up.high_hz, down_high) {
                    (Some(up_high), Some(_)) if self.invert => up_high.saturating_sub(offset),
                    (Some(up_high), Some(_)) => (up.low_hz + offset).min(up_high),
                    _ => up.low_hz,
                };
                (Some(hz.max(up.low_hz)), Some(tx_mode))
            }
        };

        Some(Plan {
            downlink_hz,
            uplink_hz,
            rx_mode,
            tx_mode,
        })
    }
}
