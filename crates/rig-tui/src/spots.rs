//! POTA spots as data: parsing the feed, the mode groups, the mode a rig is asked for, and the
//! distance from the Observer. Pure; no clock, no I/O.
//!
//! The spot shape is taken from a third-party client (no official documentation was found), so
//! every field is read defensively: a spot missing its activator, frequency or reference is
//! skipped, and anything else missing is just blank.

use std::collections::BTreeMap;

use chrono::NaiveDateTime;

use crate::geo::grid_center;
use crate::rig::RigMode;

/// How long a spot lives when the feed gives no expiry.
const DEFAULT_LIFE_SECS: i64 = 15 * 60;

#[derive(Debug, Clone, PartialEq)]
pub struct Spot {
    /// The activator's call, uppercase.
    pub call: String,
    pub hz: u64,
    /// As the feed names it (`CW`, `SSB`, `FT8`), uppercase.
    pub mode: String,
    pub reference: String,
    pub park_name: String,
    pub location: String,
    pub spotter: String,
    pub comment: String,
    /// Unix seconds the spot was made, if the feed said.
    pub spotted_at: Option<i64>,
    /// Unix seconds after which the spot is dropped.
    pub expires_at: i64,
    /// Latitude and longitude of the park, if the feed gave a position or a grid.
    pub position: Option<(f64, f64)>,
}

impl Spot {
    /// Identity for keeping a selection across refreshes.
    pub fn key(&self) -> String {
        format!("{}|{}|{}", self.call, self.reference, self.hz)
    }

    /// Seconds since the spot was made at `now` (unix), or since the fetch when unknown.
    pub fn age_secs(&self, now: i64) -> u64 {
        let at = self
            .spotted_at
            .unwrap_or(self.expires_at - DEFAULT_LIFE_SECS);
        now.saturating_sub(at).max(0) as u64
    }
}

/// Parse the activator spot list fetched at `fetched_at` (unix seconds). Repeats of one activator
/// on one frequency in one park keep only the newest spot.
pub fn parse_spots(body: &str, fetched_at: u64) -> Result<Vec<Spot>, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("spots are not JSON: {e}"))?;
    let items = value.as_array().ok_or("spots are not a list")?;
    let mut newest: BTreeMap<String, Spot> = BTreeMap::new();
    for item in items {
        let Some(spot) = parse_one(item, fetched_at as i64) else {
            continue;
        };
        match newest.get(&spot.key()) {
            Some(have) if have.spotted_at >= spot.spotted_at => {}
            _ => {
                newest.insert(spot.key(), spot);
            }
        }
    }
    Ok(newest.into_values().collect())
}

fn parse_one(item: &serde_json::Value, fetched_at: i64) -> Option<Spot> {
    let text = |k: &str| {
        item.get(k)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };
    let number = |k: &str| {
        let v = item.get(k)?;
        v.as_f64()
            .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
    };
    if item
        .get("invalid")
        .is_some_and(|v| v.as_bool() == Some(true) || v.as_f64().is_some_and(|n| n != 0.0))
    {
        return None;
    }
    let call = text("activator").to_ascii_uppercase();
    let reference = text("reference").to_ascii_uppercase();
    let khz = number("frequency").filter(|k| *k > 0.0)?;
    if call.is_empty() || reference.is_empty() {
        return None;
    }
    let spotted_at = NaiveDateTime::parse_from_str(&text("spotTime"), "%Y-%m-%dT%H:%M:%S")
        .ok()
        .map(|t| t.and_utc().timestamp());
    // `expire` is seconds from the fetch; without it, a short life from the spot time.
    let expires_at = match number("expire") {
        Some(secs) => fetched_at + secs as i64,
        None => spotted_at.unwrap_or(fetched_at) + DEFAULT_LIFE_SECS,
    };
    let position = number("latitude")
        .zip(number("longitude"))
        .or_else(|| {
            let grid = text("grid6");
            grid_center(&grid)
        })
        .or_else(|| grid_center(&text("grid4")));
    Some(Spot {
        call,
        hz: (khz * 1000.0).round() as u64,
        mode: text("mode").to_ascii_uppercase(),
        reference,
        park_name: text("parkName"),
        location: text("locationDesc"),
        spotter: text("spotter"),
        comment: text("comments"),
        spotted_at,
        expires_at,
        position,
    })
}

/// What the `o` filter groups modes by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeGroup {
    Cw,
    Phone,
    Digital,
}

impl ModeGroup {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cw => "CW",
            Self::Phone => "phone",
            Self::Digital => "digital",
        }
    }
}

/// The group of a spot or ADIF mode name: CW; SSB, its sidebands, AM and FM as phone; anything
/// else (FT8, RTTY, PSK31, ...) as digital.
pub fn mode_group(mode: &str) -> ModeGroup {
    match mode.to_ascii_uppercase().as_str() {
        "CW" => ModeGroup::Cw,
        "SSB" | "USB" | "LSB" | "AM" | "FM" | "PHONE" => ModeGroup::Phone,
        _ => ModeGroup::Digital,
    }
}

/// The rig mode to ask for, or `None` when the spot's mode is not one the rig reports (data
/// modes, which need a rig-specific data mode the app does not guess). Single sideband follows
/// the usual convention: lower below 10 MHz, upper above, and upper on 60 m.
pub fn rig_mode_for(mode: &str, hz: u64) -> Option<RigMode> {
    match mode.to_ascii_uppercase().as_str() {
        "CW" => Some(RigMode::Cw),
        "AM" => Some(RigMode::Am),
        "FM" => Some(RigMode::Fm),
        "RTTY" => Some(RigMode::Rtty),
        "USB" => Some(RigMode::Usb),
        "LSB" => Some(RigMode::Lsb),
        "SSB" => Some(
            if hz >= 10_000_000 || (5_300_000..=5_410_000).contains(&hz) {
                RigMode::Usb
            } else {
                RigMode::Lsb
            },
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"[
      {"spotId":1,"activator":"k1abc","frequency":"14062.5","mode":"cw","reference":"US-0001",
       "parkName":"Alpha SP","locationDesc":"US-IN","spotter":"W9XYZ","comments":"QRP",
       "spotTime":"2026-11-04T12:00:00","expire":300,"latitude":41.7,"longitude":-86.2},
      {"spotId":2,"activator":"K1ABC","frequency":"14062.5","mode":"CW","reference":"US-0001",
       "spotTime":"2026-11-04T12:03:00","expire":300,"grid6":"EN71dq"},
      {"spotId":3,"activator":"N2DEF","frequency":7200,"mode":"SSB","reference":"US-0002",
       "spotTime":"2026-11-04T11:00:00","grid4":"EN71"},
      {"spotId":4,"activator":"","frequency":"7000","mode":"CW","reference":"US-0003"},
      {"spotId":5,"activator":"X1BAD","frequency":"abc","mode":"CW","reference":"US-0004"},
      {"spotId":6,"activator":"W3OLD","frequency":"7030","mode":"CW","reference":"US-0005","invalid":true}
    ]"#;

    #[test]
    fn parses_and_keeps_the_newest_of_repeated_spots() {
        let spots = parse_spots(BODY, 1_000).unwrap();
        let calls: Vec<_> = spots.iter().map(|s| s.call.as_str()).collect();
        assert_eq!(calls, ["K1ABC", "N2DEF"]);
        let k = &spots[0];
        assert_eq!(k.hz, 14_062_500);
        assert_eq!(k.mode, "CW");
        // The 12:03 repeat wins (2026-11-04T12:03:00Z = 1_793_793_780).
        assert_eq!(k.spotted_at, Some(1_793_793_780));
        assert_eq!(k.expires_at, 1_000 + 300);
    }

    #[test]
    fn without_an_expiry_a_spot_lives_fifteen_minutes() {
        let spots = parse_spots(BODY, 1_000).unwrap();
        let n = &spots[1];
        assert_eq!(n.expires_at, n.spotted_at.unwrap() + 15 * 60);
    }

    #[test]
    fn rejects_text_that_is_not_a_list() {
        assert!(parse_spots("{}", 0).is_err());
        assert!(parse_spots("<html>", 0).is_err());
        assert_eq!(parse_spots("[]", 0).unwrap(), Vec::new());
    }

    #[test]
    fn a_grid_position_is_used_when_the_feed_has_no_latitude() {
        let spots = parse_spots(BODY, 0).unwrap();
        assert!(spots[1].position.is_some(), "grid4 gives a position");
    }

    #[test]
    fn modes_group_and_map_to_the_rig() {
        assert_eq!(mode_group("cw"), ModeGroup::Cw);
        assert_eq!(mode_group("SSB"), ModeGroup::Phone);
        assert_eq!(mode_group("FT8"), ModeGroup::Digital);
        assert_eq!(rig_mode_for("CW", 7_030_000), Some(RigMode::Cw));
        assert_eq!(rig_mode_for("SSB", 7_200_000), Some(RigMode::Lsb));
        assert_eq!(rig_mode_for("SSB", 14_250_000), Some(RigMode::Usb));
        assert_eq!(rig_mode_for("SSB", 5_357_000), Some(RigMode::Usb));
        assert_eq!(rig_mode_for("FT8", 14_074_000), None);
    }
}
