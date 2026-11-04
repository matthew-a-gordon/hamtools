//! Address lookup for the config dialog: a typed address in, up to five places with the 6-character
//! Locator each gives out. Nominatim (OpenStreetMap) only.
//!
//! Privacy: the typed address goes to the service and nowhere else. Only the Locator is kept; the
//! name is shown to the operator, and the latitude and longitude are dropped as soon as the
//! Locator is computed. The cache lives in memory for one dialog session ([`Geocoder::clear_cache`]).
//!
//! Blocking; `main` runs it on a short-lived thread (see [`Geocoder::lookup_in_background`]) and
//! hands the result to `App`, the same shape as sat-tui's fetcher.
//!
//! Dependency justification: `ureq` is used for this one HTTPS GET to Nominatim and nothing else.
//! It is blocking, so there is no async runtime or hyper/reqwest stack to carry for it. Only its
//! `rustls` feature is enabled (https needs TLS); gzip is off because the JSON is small.

use std::collections::HashMap;
use std::fmt;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use logbook::Grid;

const BASE_URL: &str = "https://nominatim.openstreetmap.org";
/// Names the app, its version and where to find it; no personal contact.
pub const USER_AGENT: &str = concat!(
    "rig-tui/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/matthew-a-gordon/hamtools)"
);
/// Nominatim's usage policy allows at most one request per second.
pub const MIN_INTERVAL: Duration = Duration::from_secs(1);
const MAX_CANDIDATES: usize = 5;
const MAX_BODY_BYTES: u64 = 1024 * 1024;

/// One place the address could be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub name: String,
    pub locator: Grid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeocodeError {
    /// The service could not be reached.
    Offline(String),
    /// HTTP 429.
    RateLimited,
    /// Any other HTTP error status.
    Server(u16),
    /// The service answered with something that is not a list of places.
    BadReply(String),
}

impl fmt::Display for GeocodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GeocodeError::Offline(why) => write!(f, "no network: {why}"),
            GeocodeError::RateLimited => f.write_str("rate limited by OpenStreetMap; try later"),
            GeocodeError::Server(code) => write!(f, "OpenStreetMap answered HTTP {code}"),
            GeocodeError::BadReply(why) => write!(f, "unreadable answer: {why}"),
        }
    }
}

impl std::error::Error for GeocodeError {}

pub type GeocodeResult = Result<Vec<Candidate>, GeocodeError>;

#[derive(Default)]
struct State {
    /// The earliest moment the next request may start.
    next_slot: Option<Instant>,
    cache: HashMap<String, Vec<Candidate>>,
}

/// A handle on the lookup service; clones share the spacing and the cache.
#[derive(Clone)]
pub struct Geocoder {
    base_url: String,
    min_interval: Duration,
    agent: ureq::Agent,
    state: Arc<Mutex<State>>,
}

impl Default for Geocoder {
    fn default() -> Self {
        Self::with_base_url(BASE_URL)
    }
}

impl Geocoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Point at another server (for tests).
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(20)))
            .build()
            .into();
        Self {
            base_url: base_url.into(),
            min_interval: MIN_INTERVAL,
            agent,
            state: Arc::default(),
        }
    }

    pub fn with_min_interval(mut self, min_interval: Duration) -> Self {
        self.min_interval = min_interval;
        self
    }

    /// Forget every cached answer.
    pub fn clear_cache(&self) {
        self.state.lock().expect("geocoder state").cache.clear();
    }

    /// Look the address up, waiting first if the last request was less than the minimum interval
    /// ago. An address seen before in this session is answered from the cache.
    pub fn lookup(&self, address: &str) -> GeocodeResult {
        let key = address.split_whitespace().collect::<Vec<_>>().join(" ");
        let key = key.to_lowercase();
        let wait = {
            let mut state = self.state.lock().expect("geocoder state");
            if let Some(hit) = state.cache.get(&key) {
                return Ok(hit.clone());
            }
            let now = Instant::now();
            let start = state.next_slot.map_or(now, |slot| slot.max(now));
            state.next_slot = Some(start + self.min_interval);
            start - now
        };
        thread::sleep(wait);
        let answer = self.request(address.trim());
        // Space from when this request finished, so two never arrive closer than the minimum.
        self.state.lock().expect("geocoder state").next_slot =
            Some(Instant::now() + self.min_interval);
        let found = answer?;
        self.state
            .lock()
            .expect("geocoder state")
            .cache
            .insert(key, found.clone());
        Ok(found)
    }

    /// Run [`Geocoder::lookup`] on a short-lived thread and send the result on `tx`.
    pub fn lookup_in_background(&self, address: String, tx: Sender<GeocodeResult>) {
        let geocoder = self.clone();
        thread::spawn(move || {
            let _ = tx.send(geocoder.lookup(&address));
        });
    }

    fn request(&self, address: &str) -> GeocodeResult {
        let url = format!("{}/search", self.base_url);
        let mut response = self
            .agent
            .get(&url)
            .query("q", address)
            .query("format", "jsonv2")
            .query("limit", MAX_CANDIDATES.to_string())
            .call()
            .map_err(|e| match e {
                ureq::Error::StatusCode(429) => GeocodeError::RateLimited,
                ureq::Error::StatusCode(code) => GeocodeError::Server(code),
                other => GeocodeError::Offline(other.to_string()),
            })?;
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_BODY_BYTES)
            .read_to_string()
            .map_err(|e| GeocodeError::Offline(e.to_string()))?;
        parse(&body)
    }
}

fn parse(body: &str) -> GeocodeResult {
    let bad = |why: &str| GeocodeError::BadReply(why.to_string());
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| GeocodeError::BadReply(e.to_string()))?;
    let places = value.as_array().ok_or_else(|| bad("not a list"))?;
    let mut found = Vec::new();
    for place in places.iter().take(MAX_CANDIDATES) {
        let number = |key: &str| {
            place
                .get(key)
                .and_then(serde_json::Value::as_str)
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|n| n.is_finite())
        };
        let (Some(lat), Some(lon)) = (number("lat"), number("lon")) else {
            return Err(bad("a place without a position"));
        };
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return Err(bad("a position out of range"));
        }
        let name = place
            .get("display_name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("(unnamed)")
            .to_string();
        found.push(Candidate {
            name,
            locator: Grid::at(lat, lon),
        });
    }
    Ok(found)
}
