//! Rig profiles, the static capability table, per-rig state and the Ctrl-R switcher.
//!
//! Both rigs stay connected. The Active rig's readings live in `App`'s own fields; when the
//! operator switches, `App` swaps them with the [`RigState`] parked in the slot, so each rig
//! keeps its connection, last readings and tuning step. Capabilities are a static table keyed
//! by the profile's kind, not probed from `rigctld` (ADR 0004): hamlib's claims and reality
//! differ. A row not verified on hardware says so.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use hamtools_config::{RigKind, RigProfile};
use hamtools_tui::modal::Modal;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::Reading;
use crate::rig::{Feature, RigEvent, RigMode};
use crate::style::{dim, red, yellow};

/// How often the inactive rig is asked for health, frequency and mode. A starting value,
/// untested on hardware.
pub const LIGHT_POLL_EVERY: Duration = Duration::from_secs(5);

/// Something a rig can do that an Activity may need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// Send CW through `rigctld` (`b`).
    Cw,
    /// Split operation with a separate transmit frequency.
    Split,
    /// The FT-847's satellite mode (separate uplink and downlink VFOs).
    SatMode,
}

impl Capability {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cw => "CW",
            Self::Split => "split",
            Self::SatMode => "sat mode",
        }
    }
}

/// One row cell: whether the rig does it, and whether that was seen on the real radio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Support {
    pub supported: bool,
    pub verified: bool,
}

const fn yes(verified: bool) -> Support {
    Support {
        supported: true,
        verified,
    }
}

const fn no(verified: bool) -> Support {
    Support {
        supported: false,
        verified,
    }
}

/// What a kind of rig can do. Verified means seen on the user's radio per CLAUDE.md's
/// "Hardware status"; everything else is a datasheet or hamlib claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub cw: Support,
    pub split: Support,
    pub sat_mode: Support,
    /// ADIF band names the rig covers.
    pub bands: Vec<&'static str>,
    pub bands_verified: bool,
}

/// The capability table.
pub fn table(kind: RigKind) -> Capabilities {
    match kind {
        // CW send (2026-10-26) and split (2026-11-04) are verified on the KX3. It has no
        // satellite mode, but that is a datasheet fact, not something seen through rigctld.
        // Only 40 m was ever tuned, so coverage is not verified.
        RigKind::Kx3 => Capabilities {
            cw: yes(true),
            split: yes(true),
            sat_mode: no(false),
            bands: [
                "160m", "80m", "60m", "40m", "30m", "20m", "17m", "15m", "12m", "10m", "6m",
            ]
            .to_vec(),
            bands_verified: false,
        },
        // Split is verified on the FT-847 (2026-11-04, CLAUDE.md: `s` reads `1 Sub`, `i`/`x`
        // return the transmit side, tracking set and read back both frequencies). SAT mode
        // engaging and releasing is verified too (2026-11-05, CLAUDE.md): split on turns the SAT
        // indicator on, `S 0 VFOA` turns it off, seen on the radio's display. Hamlib has no
        // read-back for it, and `Z` while tracking does not hold (#81). CW is not known to work
        // through hamlib's ft847 backend, so it stays off and unverified.
        RigKind::Ft847 => Capabilities {
            cw: no(false),
            split: yes(true),
            sat_mode: yes(true),
            bands: [
                "160m", "80m", "40m", "30m", "20m", "17m", "15m", "12m", "10m", "6m", "2m", "70cm",
            ]
            .to_vec(),
            bands_verified: false,
        },
        // A rig of unknown make: nothing is assumed, and the profile's overrides say more.
        RigKind::Generic => Capabilities {
            cw: no(false),
            split: no(false),
            sat_mode: no(false),
            bands: Vec::new(),
            bands_verified: false,
        },
    }
}

impl Capabilities {
    /// The kind's row with the profile's overrides applied (cw, split, sat_mode and bands). An override is the operator's word,
    /// not a hardware check, so it is never verified.
    pub fn for_profile(profile: &RigProfile) -> Self {
        let mut caps = table(profile.kind);
        let over = |cell: &mut Support, value: Option<bool>| {
            if let Some(supported) = value {
                *cell = Support {
                    supported,
                    verified: false,
                };
            }
        };
        over(&mut caps.cw, profile.cw);
        over(&mut caps.split, profile.split);
        over(&mut caps.sat_mode, profile.sat_mode);
        if let Some(bands) = &profile.bands {
            // The config only lets ADIF band names through, so every name finds its table entry.
            caps.bands = bands
                .iter()
                .filter_map(|name| logbook::BANDS.iter().find(|b| b.name == name))
                .map(|b| b.name)
                .collect();
            // The operator's word about what is installed, not a hardware check.
            caps.bands_verified = false;
        }
        caps
    }

    fn cell(&self, capability: Capability) -> Support {
        match capability {
            Capability::Cw => self.cw,
            Capability::SatMode => self.sat_mode,
            Capability::Split => self.split,
        }
    }

    pub fn has(&self, capability: Capability) -> bool {
        self.cell(capability).supported
    }

    pub fn verified(&self, capability: Capability) -> bool {
        self.cell(capability).verified
    }

    /// One line per row, each unverified one labelled "untested on hardware".
    pub fn rows(&self) -> Vec<String> {
        let note = |verified: bool| {
            if verified {
                ""
            } else {
                ", untested on hardware"
            }
        };
        let mut rows: Vec<String> = [Capability::Cw, Capability::Split, Capability::SatMode]
            .into_iter()
            .map(|c| {
                let cell = self.cell(c);
                format!(
                    "{} {}{}",
                    c.label(),
                    if cell.supported { "yes" } else { "no" },
                    note(cell.verified)
                )
            })
            .collect();
        let (first, last) = (self.bands.first(), self.bands.last());
        match (first, last) {
            (Some(first), Some(last)) => {
                rows.push(format!("bands {first}-{last}{}", note(self.bands_verified)))
            }
            _ => rows.push(format!("bands unknown{}", note(self.bands_verified))),
        }
        rows
    }
}

/// What the app keeps per rig besides the profile: the readings and timers that belong to one
/// radio. `App` holds the Active rig's in its own fields and swaps on a switch.
#[derive(Debug, Clone)]
pub(crate) struct RigState {
    pub connected: bool,
    pub freq: Option<Reading<u64>>,
    pub mode: Option<Reading<(RigMode, i64)>>,
    pub vfo: Option<Reading<String>>,
    pub split: Option<Reading<(bool, String)>>,
    pub split_freq: Option<Reading<u64>>,
    pub ptt: Option<Reading<bool>>,
    pub strength: Option<Reading<i32>>,
    pub unsupported: BTreeSet<Feature>,
    pub step: usize,
    pub last_connect: Option<Instant>,
    pub last_poll: Option<Instant>,
    pub last_smeter: Option<Instant>,
    /// When the inactive rig was last asked for a light poll.
    pub last_light: Option<Instant>,
}

impl RigState {
    pub fn new(step: usize) -> Self {
        Self {
            connected: false,
            freq: None,
            mode: None,
            vfo: None,
            split: None,
            split_freq: None,
            ptt: None,
            strength: None,
            unsupported: BTreeSet::new(),
            step,
            last_connect: None,
            last_poll: None,
            last_smeter: None,
            last_light: None,
        }
    }

    /// Take in what a light poll of this (inactive) rig reported.
    pub fn apply(&mut self, event: RigEvent, now: Instant) {
        fn stamp<T>(value: T, at: Instant) -> Option<Reading<T>> {
            Some(Reading { value, at })
        }
        match event {
            RigEvent::Connected => {
                self.connected = true;
                self.unsupported.clear();
            }
            RigEvent::Disconnected(_) => self.connected = false,
            RigEvent::Freq(hz) => self.freq = stamp(hz, now),
            RigEvent::Mode { mode, passband } => self.mode = stamp((mode, passband), now),
            RigEvent::Vfo(vfo) => self.vfo = stamp(vfo, now),
            RigEvent::Split { enabled, tx_vfo } => self.split = stamp((enabled, tx_vfo), now),
            RigEvent::SplitFreq(hz) => self.split_freq = stamp(hz, now),
            RigEvent::Ptt(on) => self.ptt = stamp(on, now),
            RigEvent::Strength(db) => self.strength = stamp(db, now),
            RigEvent::Unsupported(feature) => {
                self.unsupported.insert(feature);
            }
            // Nothing is sent to or panelled on an inactive rig.
            RigEvent::SetFailed(_) | RigEvent::SendFailed(_) | RigEvent::PanelReply { .. } => {}
        }
    }
}

/// One configured rig.
pub(crate) struct RigSlot {
    pub name: String,
    pub addr: String,
    pub kind: RigKind,
    pub caps: Capabilities,
    /// The parked readings while this rig is not Active.
    pub state: RigState,
}

impl RigSlot {
    pub fn from_profile(profile: &RigProfile, step: usize) -> Self {
        Self {
            name: profile.name.clone(),
            addr: profile.addr.clone(),
            kind: profile.kind,
            caps: Capabilities::for_profile(profile),
            state: RigState::new(step),
        }
    }
}

/// One row of the switcher, as `App` describes it.
pub(crate) struct SwitcherRow {
    pub name: String,
    pub kind: RigKind,
    pub addr: String,
    pub connected: bool,
    pub active: bool,
    pub frequency: Option<u64>,
    pub mode: Option<String>,
    pub caps: Capabilities,
}

/// What a key did in the switcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SwitcherOutcome {
    Stay,
    Close,
    Select(usize),
}

/// The Ctrl-R overlay: both rigs, their connection states and capability rows.
pub(crate) struct Switcher {
    cursor: usize,
    count: usize,
}

impl Switcher {
    pub fn open(active: usize, count: usize) -> Self {
        Self {
            cursor: active,
            count,
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> SwitcherOutcome {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => SwitcherOutcome::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = self.cursor.saturating_sub(1);
                SwitcherOutcome::Stay
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.cursor = (self.cursor + 1).min(self.count.saturating_sub(1));
                SwitcherOutcome::Stay
            }
            KeyCode::Enter => SwitcherOutcome::Select(self.cursor),
            _ => SwitcherOutcome::Stay,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, rows: &[SwitcherRow]) {
        let mut lines: Vec<Line> = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let marker = if i == self.cursor { ">" } else { " " };
            let state = if row.connected {
                Span::raw("connected")
            } else {
                Span::styled("disconnected", red())
            };
            let reading = match (row.frequency, &row.mode) {
                (Some(hz), Some(mode)) => format!("  {} {mode}", crate::ui::format_hz(hz)),
                (Some(hz), None) => format!("  {}", crate::ui::format_hz(hz)),
                _ => String::new(),
            };
            // An unreachable rig is greyed out, but can still be chosen.
            let style = if i == self.cursor {
                crate::style::selected()
            } else if !row.connected {
                dim()
            } else {
                Style::default()
            };
            let active = if row.active { " (active)" } else { "" };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(
                        "{marker} {}{active}  {}  {}  ",
                        row.name,
                        row.kind.as_str(),
                        row.addr
                    ),
                    style,
                ),
                state,
                Span::raw(reading),
            ]));
            lines.push(Line::from(Span::styled(
                format!("    {}", row.caps.rows().join("; ")),
                if !row.connected || (row.caps.bands_verified && row.caps.cw.verified) {
                    dim()
                } else {
                    yellow()
                },
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Up/Down choose  Enter switch  Esc close",
            dim(),
        )));
        let height = u16::try_from(lines.len())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        let inner = Modal::new("Rigs", 78, height).render(frame, area);
        frame.render_widget(
            Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }),
            inner,
        );
    }
}
