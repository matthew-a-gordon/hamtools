//! What a POTA Activator takes from the shared spot feed: whether the operator is spotted, and
//! the other activators nearby (park-to-park candidates).
//!
//! It reads the same feed the Hunter does ([`crate::pota::spots_feed`]) through the same
//! [`ReferenceClient`], so the 60 s floor and every other limit stay in the client. It only asks
//! while its Activity is ticked, which is only while that Activity is in the foreground.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::geo::distance_km;
use crate::pota::{SPOTS_KEY, spots_feed};
use crate::reference::{ReferenceClient, SPOTS_EVERY, Submit};
use crate::spots::{Spot, parse_spots};
use crate::ui::format_hz;

const SNAPSHOT_EVERY: Duration = Duration::from_millis(500);

pub struct SpotWatch {
    client: Arc<ReferenceClient>,
    online: bool,
    /// The operator's call, uppercase.
    call: Option<String>,
    spots: Vec<Spot>,
    parsed_at: Option<u64>,
    next_try: Option<Instant>,
    last_check: Option<Instant>,
}

impl SpotWatch {
    pub fn new(client: Arc<ReferenceClient>, online: bool, call: Option<&str>) -> Self {
        Self {
            client,
            online,
            call: call.map(|c| c.trim().to_ascii_uppercase()),
            spots: Vec::new(),
            parsed_at: None,
            next_try: None,
            last_check: None,
        }
    }

    /// Ask for spots when due and pick up the latest snapshot. Called from the Activity's tick.
    pub fn tick(&mut self, now: Instant) {
        if self.online && self.next_try.is_none_or(|t| now >= t) {
            let wait = match self.client.request(&spots_feed()) {
                Submit::Queued => SPOTS_EVERY,
                Submit::TooSoon(wait) => wait.max(Duration::from_secs(1)),
            };
            self.next_try = Some(now + wait);
        }
        if self
            .last_check
            .is_none_or(|t| now.duration_since(t) >= SNAPSHOT_EVERY)
        {
            self.last_check = Some(now);
            self.refresh_snapshot();
        }
    }

    fn refresh_snapshot(&mut self) {
        let Some(snap) = self.client.snapshot(SPOTS_KEY) else {
            return;
        };
        if self.parsed_at == Some(snap.fetched_at) {
            return;
        }
        self.parsed_at = Some(snap.fetched_at);
        if let Ok(spots) = parse_spots(&snap.body, snap.fetched_at) {
            self.spots = spots;
        }
    }

    fn live(&self, now: i64) -> impl Iterator<Item = &Spot> {
        self.spots.iter().filter(move |s| s.expires_at > now)
    }

    fn is_me(&self, spot: &Spot) -> bool {
        self.call.as_deref().is_some_and(|me| spot.call == me)
    }

    /// The "you are spotted" line, only when the operator's call is in the current spots: call,
    /// frequency, mode, spotter and age of the newest such spot.
    pub fn spotted_line(&self, now: i64) -> Option<String> {
        let spot = self
            .live(now)
            .filter(|s| self.is_me(s))
            .min_by_key(|s| s.age_secs(now))?;
        let spotter = if spot.spotter.is_empty() {
            "?"
        } else {
            &spot.spotter
        };
        Some(format!(
            "spotted: {} {} {} by {} {} ago",
            spot.call,
            format_hz(spot.hz),
            spot.mode,
            spotter,
            age_text(spot.age_secs(now)),
        ))
    }

    /// Other activators' live spots as display lines, nearest first when `from` is known, else
    /// newest first.
    pub fn nearby(&self, now: i64, from: Option<(f64, f64)>) -> Vec<String> {
        let mut rows: Vec<(Option<f64>, u64, &Spot)> = self
            .live(now)
            .filter(|s| !self.is_me(s))
            .map(|s| {
                let d = from.zip(s.position).map(|(a, b)| distance_km(a, b));
                (d, s.age_secs(now), s)
            })
            .collect();
        rows.sort_by(|a, b| {
            a.0.unwrap_or(f64::MAX)
                .total_cmp(&b.0.unwrap_or(f64::MAX))
                .then(a.1.cmp(&b.1))
                .then(a.2.call.cmp(&b.2.call))
        });
        rows.into_iter()
            .map(|(d, _, s)| {
                format!(
                    "{:<9} {:>11} {:<4} {:<8}{}",
                    s.call,
                    format_hz(s.hz),
                    s.mode,
                    s.reference,
                    d.map_or(String::new(), |d| format!(" {d:.0}km")),
                )
            })
            .collect()
    }
}

fn age_text(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m", secs / 60)
    }
}
