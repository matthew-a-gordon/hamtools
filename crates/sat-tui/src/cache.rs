//! On-disk cache for TLEs and SatNOGS data, plus the rules for when to refresh them.
//!
//! The field use case has no internet, so everything shown comes from here and carries its age.

use std::io;
use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dataset {
    /// CelesTrak amateur-radio TLEs.
    Tles,
    /// SatNOGS DB transmitters.
    Transmitters,
}

impl Dataset {
    fn stem(self) -> &'static str {
        match self {
            Dataset::Tles => "tles.txt",
            Dataset::Transmitters => "transmitters.json",
        }
    }
}

/// Cached text and when it was downloaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cached {
    pub body: String,
    pub fetched_at: DateTime<Utc>,
}

/// What the cache knows about a dataset, for deciding whether to fetch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    /// When the cached data was successfully downloaded.
    pub fetched_at: Option<DateTime<Utc>>,
    /// When we last tried, successful or not.
    pub last_attempt: Option<DateTime<Utc>>,
}

pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, dataset: Dataset, suffix: &str) -> PathBuf {
        self.dir.join(format!("{}{suffix}", dataset.stem()))
    }

    /// Write `contents` to `path` without ever leaving a half-written file behind.
    fn write_atomic(path: &PathBuf, contents: &str) -> io::Result<()> {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, contents)?;
        std::fs::rename(&tmp, path)
    }

    fn read_time(&self, dataset: Dataset, suffix: &str) -> Option<DateTime<Utc>> {
        let text = std::fs::read_to_string(self.path(dataset, suffix)).ok()?;
        DateTime::parse_from_rfc3339(text.trim())
            .ok()
            .map(|t| t.with_timezone(&Utc))
    }

    /// Replace the cached data; this also counts as an attempt at `now`.
    pub fn store(&self, dataset: Dataset, body: &str, now: DateTime<Utc>) -> io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        Self::write_atomic(&self.path(dataset, ""), body)?;
        Self::write_atomic(&self.path(dataset, ".fetched"), &now.to_rfc3339())?;
        self.record_attempt(dataset, now)
    }

    /// Remember that we tried to download at `now`, whatever the outcome.
    pub fn record_attempt(&self, dataset: Dataset, now: DateTime<Utc>) -> io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        Self::write_atomic(&self.path(dataset, ".attempt"), &now.to_rfc3339())
    }

    /// The cached data, or `None` if there is none or its age is unknown.
    pub fn load(&self, dataset: Dataset) -> io::Result<Option<Cached>> {
        let body = match std::fs::read_to_string(self.path(dataset, "")) {
            Ok(body) => body,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        Ok(self
            .read_time(dataset, ".fetched")
            .map(|fetched_at| Cached { body, fetched_at }))
    }

    pub fn state(&self, dataset: Dataset) -> State {
        State {
            fetched_at: self.read_time(dataset, ".fetched"),
            last_attempt: self.read_time(dataset, ".attempt"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Cached data is recent enough; do nothing.
    Fresh,
    Fetch,
    /// Wanted, but CelesTrak must not be asked again yet.
    TooSoon {
        retry_in: Duration,
    },
}

/// Data older than this is refreshed automatically.
const MAX_AGE: i64 = 24;
/// CelesTrak asks clients not to download a group more often than this.
const CELESTRAK_MIN_INTERVAL: i64 = 2;
/// TLEs older than this are too inaccurate to trust for pointing.
const STALE_DAYS: i64 = 7;

/// Whether to download `dataset` now. `manual` means the operator asked, which skips the
/// "is it old enough" test but never CelesTrak's minimum interval between requests.
pub fn decide(dataset: Dataset, state: &State, now: DateTime<Utc>, manual: bool) -> Decision {
    let wanted = manual
        || state
            .fetched_at
            .is_none_or(|fetched| now - fetched > Duration::hours(MAX_AGE));
    if !wanted {
        return Decision::Fresh;
    }
    if dataset == Dataset::Tles
        && let Some(attempt) = state.last_attempt
    {
        let since = now - attempt;
        let min = Duration::hours(CELESTRAK_MIN_INTERVAL);
        if since < min {
            return Decision::TooSoon {
                retry_in: min - since,
            };
        }
    }
    Decision::Fetch
}

/// Whether TLEs fetched at `fetched_at` are old enough to warn about.
pub fn tles_stale(fetched_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now - fetched_at > Duration::days(STALE_DAYS)
}

/// An age in the largest whole unit: `<1m`, `5m`, `47h`, `2d`.
pub fn format_age(age: Duration) -> String {
    if age < Duration::minutes(1) {
        "<1m".to_string()
    } else if age < Duration::hours(1) {
        format!("{}m", age.num_minutes())
    } else if age < Duration::hours(48) {
        format!("{}h", age.num_hours())
    } else {
        format!("{}d", age.num_days())
    }
}
