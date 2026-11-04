//! The one shared client through which every reference-data request passes (ADR 0003).
//!
//! There is no unthrottled method: the only way to reach the [`Transport`] is the worker thread's
//! private `fetch`, which applies, in order, the per-feed interval, any backoff, and the global
//! minimum gap. Network work runs on that thread; callers read the latest [`Snapshot`] from memory
//! (or the disk cache) and are never blocked by a slow or failing fetch.
//!
//! Limits (config may only slow them, see [`Limits::slowed`]): spots 1 per 60 s, scheduled
//! activations (reserved) 1 per 5 min, the SOTA summit list 1 per day, 1 s between any two
//! requests, exponential backoff 60 s to 1 h on 429/5xx and on transport errors (honouring
//! `Retry-After`), timeouts 5 s connect and 15 s total. Conditional requests (`If-None-Match`,
//! `If-Modified-Since`) are sent whenever a cached copy carries validators.
//!
//! The cache is one JSON file per feed in the XDG cache directory, each with a `fetched_at`.
//! Time and HTTP are behind [`Clock`] and [`Transport`] so tests need no network and no waiting.
//!
//! Not yet routed through here: the config dialog's Nominatim lookup ([`crate::geocode`]), which
//! keeps its own 1 s spacing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const PROJECT_URL: &str = "https://github.com/matthew-a-gordon/hamtools";
pub const SPOTS_EVERY: Duration = Duration::from_secs(60);
pub const ACTIVATIONS_EVERY: Duration = Duration::from_secs(5 * 60);
pub const DAILY_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
pub const MIN_GAP: Duration = Duration::from_secs(1);
pub const BACKOFF_START: Duration = Duration::from_secs(60);
pub const BACKOFF_MAX: Duration = Duration::from_secs(60 * 60);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const TOTAL_TIMEOUT: Duration = Duration::from_secs(15);
/// Reference data older than this is shown in a warning color (still usable).
pub const STALE_AFTER_SECS: u64 = 30 * 24 * 60 * 60;
const MAX_BODY_BYTES: u64 = 64 * 1024 * 1024;

/// Time source: a monotonic reading, wall-clock seconds for `fetched_at`, and sleeping.
pub trait Clock: Send + Sync {
    fn now(&self) -> Duration;
    fn unix(&self) -> u64;
    fn sleep(&self, d: Duration);
}

/// The real clock.
pub struct SystemClock {
    start: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }
    fn unix(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
    fn sleep(&self, d: Duration) {
        thread::sleep(d);
    }
}

/// One GET, as handed to a [`Transport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: String,
    pub user_agent: String,
    pub if_none_match: Option<String>,
    pub if_modified_since: Option<String>,
    pub connect_timeout: Duration,
    pub total_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub retry_after: Option<Duration>,
}

/// The HTTP layer. Implementations must not retry or sleep; the client owns all pacing.
pub trait Transport: Send + Sync {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, String>;
}

/// How often a feed may be requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    Spots,
    Activations,
    Daily,
    /// Lists and details fetched on demand: only the global gap and backoff apply.
    OnDemand,
}

/// One cacheable source. `key` names its cache file (letters, digits, `-` and `_` only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub key: String,
    pub url: String,
    pub pace: Pace,
}

impl Feed {
    pub fn new(key: &str, url: &str, pace: Pace) -> Self {
        let key = key
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        Self {
            key,
            url: url.to_string(),
            pace,
        }
    }
}

/// The request-rate constants. Only [`Limits::slowed`] changes them, and only upward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    spots: Duration,
    activations: Duration,
    gap: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            spots: SPOTS_EVERY,
            activations: ACTIVATIONS_EVERY,
            gap: MIN_GAP,
        }
    }
}

impl Limits {
    /// Apply configured intervals; a value below the built-in floor is ignored.
    pub fn slowed(
        spots: Option<Duration>,
        activations: Option<Duration>,
        gap: Option<Duration>,
    ) -> Self {
        let d = Self::default();
        Self {
            spots: spots.map_or(d.spots, |v| v.max(d.spots)),
            activations: activations.map_or(d.activations, |v| v.max(d.activations)),
            gap: gap.map_or(d.gap, |v| v.max(d.gap)),
        }
    }

    fn interval(&self, pace: Pace) -> Duration {
        match pace {
            Pace::Spots => self.spots,
            Pace::Activations => self.activations,
            Pace::Daily => DAILY_EVERY,
            Pace::OnDemand => Duration::ZERO,
        }
    }
}

/// The latest data for a feed, as cached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub body: Arc<String>,
    /// Seconds since the Unix epoch of the last successful fetch or revalidation.
    pub fetched_at: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

impl Snapshot {
    pub fn age_secs(&self, now_unix: u64) -> u64 {
        now_unix.saturating_sub(self.fetched_at)
    }
    pub fn is_stale(&self, now_unix: u64) -> bool {
        self.age_secs(now_unix) > STALE_AFTER_SECS
    }
}

/// What the offline indicator needs for one feed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FeedStatus {
    pub last_success: Option<u64>,
    pub last_error: Option<String>,
    pub consecutive_failures: u32,
    /// How long until another request for this feed would be allowed ("next in Ns").
    pub next_in: Duration,
}

/// The reply to [`ReferenceClient::request`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Submit {
    Queued,
    /// Refused by a limit or backoff; try again after this long.
    TooSoon(Duration),
}

#[derive(Default)]
struct FeedState {
    last_start: Option<Duration>,
    blocked_until: Option<Duration>,
    failures: u32,
    last_success: Option<u64>,
    last_error: Option<String>,
}

#[derive(Default)]
struct State {
    last_start_any: Option<Duration>,
    feeds: HashMap<String, FeedState>,
    snapshots: HashMap<String, Snapshot>,
}

struct Inner {
    transport: Arc<dyn Transport>,
    clock: Arc<dyn Clock>,
    limits: Limits,
    user_agent: String,
    cache_dir: PathBuf,
    state: Mutex<State>,
}

enum Job {
    Fetch(Feed),
    Barrier(Sender<()>),
}

/// The handle callers hold; share it behind an `Arc`.
pub struct ReferenceClient {
    inner: Arc<Inner>,
    tx: Sender<Job>,
}

/// `hamtools/<version> (+<project url>; <callsign>)`, the callsign only when set.
pub fn user_agent(callsign: Option<&str>) -> String {
    let call: String = callsign
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '/' || *c == '-')
        .collect();
    let version = env!("CARGO_PKG_VERSION");
    if call.is_empty() {
        format!("hamtools/{version} (+{PROJECT_URL})")
    } else {
        format!("hamtools/{version} (+{PROJECT_URL}; {call})")
    }
}

/// `$XDG_CACHE_HOME/hamtools`, else `~/.cache/hamtools`.
pub fn default_cache_dir() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".cache"),
    };
    Some(base.join("hamtools"))
}

impl ReferenceClient {
    /// Start the client and its network thread. The thread ends when the client is dropped.
    pub fn start(
        transport: Arc<dyn Transport>,
        clock: Arc<dyn Clock>,
        limits: Limits,
        user_agent: String,
        cache_dir: PathBuf,
    ) -> Self {
        let inner = Arc::new(Inner {
            transport,
            clock,
            limits,
            user_agent,
            cache_dir,
            state: Mutex::new(State::default()),
        });
        let (tx, rx) = mpsc::channel::<Job>();
        let worker = Arc::clone(&inner);
        thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                match job {
                    Job::Fetch(feed) => worker.fetch(&feed),
                    Job::Barrier(done) => {
                        let _ = done.send(());
                    }
                }
            }
        });
        Self { inner, tx }
    }

    /// Ask for a fetch. Returns at once; the result shows up in [`snapshot`](Self::snapshot).
    pub fn request(&self, feed: &Feed) -> Submit {
        let wait = self.inner.ready_in(feed);
        if !wait.is_zero() {
            return Submit::TooSoon(wait);
        }
        let _ = self.tx.send(Job::Fetch(feed.clone()));
        Submit::Queued
    }

    /// Block until every request queued so far has been handled (tests, shutdown).
    pub fn sync(&self) {
        let (done_tx, done_rx) = mpsc::channel();
        if self.tx.send(Job::Barrier(done_tx)).is_ok() {
            let _ = done_rx.recv();
        }
    }

    /// The latest data for `key`: memory first, then the disk cache. Never touches the network.
    pub fn snapshot(&self, key: &str) -> Option<Snapshot> {
        if let Some(s) = self.inner.lock().snapshots.get(key) {
            return Some(s.clone());
        }
        let loaded = read_cache(&self.inner.cache_dir, key)?;
        Some(
            self.inner
                .lock()
                .snapshots
                .entry(key.to_string())
                .or_insert(loaded)
                .clone(),
        )
    }

    pub fn status(&self, feed: &Feed) -> FeedStatus {
        let next_in = self.inner.ready_in(feed);
        let st = self.inner.lock();
        let f = st.feeds.get(&feed.key);
        FeedStatus {
            last_success: f.and_then(|f| f.last_success),
            last_error: f.and_then(|f| f.last_error.clone()),
            consecutive_failures: f.map_or(0, |f| f.failures),
            next_in,
        }
    }
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("reference client state")
    }

    /// Time until a request for `feed` may start: its interval and backoff. The 1 s gap is not
    /// counted here; the worker sleeps through it.
    fn ready_in(&self, feed: &Feed) -> Duration {
        let now = self.clock.now();
        let st = self.lock();
        let Some(f) = st.feeds.get(&feed.key) else {
            return Duration::ZERO;
        };
        let by_interval = f.last_start.map_or(Duration::ZERO, |t| {
            (t + self.limits.interval(feed.pace)).saturating_sub(now)
        });
        let by_backoff = f
            .blocked_until
            .map_or(Duration::ZERO, |t| t.saturating_sub(now));
        by_interval.max(by_backoff)
    }

    /// The only place a request is made. Runs on the worker thread.
    fn fetch(&self, feed: &Feed) {
        // Reserve our slot under the lock: interval, backoff, then the global gap.
        if !self.ready_in(feed).is_zero() {
            return;
        }
        let (gap_wait, validators) = {
            let now = self.clock.now();
            let mut st = self.lock();
            let gap_wait = st.last_start_any.map_or(Duration::ZERO, |t| {
                (t + self.limits.gap).saturating_sub(now)
            });
            let slot = now + gap_wait;
            st.last_start_any = Some(slot);
            st.feeds.entry(feed.key.clone()).or_default().last_start = Some(slot);
            let v = st
                .snapshots
                .get(&feed.key)
                .map(|s| (s.etag.clone(), s.last_modified.clone()));
            (gap_wait, v)
        };
        let validators = validators
            .or_else(|| read_cache(&self.cache_dir, &feed.key).map(|s| (s.etag, s.last_modified)));
        if !gap_wait.is_zero() {
            self.clock.sleep(gap_wait);
        }
        let (etag, modified) = validators.unwrap_or((None, None));
        let req = HttpRequest {
            url: feed.url.clone(),
            user_agent: self.user_agent.clone(),
            if_none_match: etag,
            if_modified_since: modified,
            connect_timeout: CONNECT_TIMEOUT,
            total_timeout: TOTAL_TIMEOUT,
        };
        let result = self.transport.get(&req);
        self.finish(feed, result);
    }

    fn finish(&self, feed: &Feed, result: Result<HttpResponse, String>) {
        let unix = self.clock.unix();
        let now = self.clock.now();
        let mut retry_after = None;
        // Backoff applies to 429, 5xx and transport errors, not to other statuses.
        let mut backoff = false;
        let outcome: Result<Snapshot, String> = match result {
            Err(e) => {
                backoff = true;
                Err(e)
            }
            Ok(r) if r.status == 200 => String::from_utf8(r.body)
                .map(|b| Snapshot {
                    body: Arc::new(b),
                    fetched_at: unix,
                    etag: r.etag,
                    last_modified: r.last_modified,
                })
                .map_err(|_| "reply is not UTF-8 text".to_string()),
            Ok(r) if r.status == 304 => {
                let have = self.lock().snapshots.get(&feed.key).cloned();
                let have = have.or_else(|| read_cache(&self.cache_dir, &feed.key));
                have.map(|s| Snapshot {
                    fetched_at: unix,
                    ..s
                })
                .ok_or_else(|| "HTTP 304 with nothing cached".to_string())
            }
            Ok(r) => {
                backoff = r.status == 429 || (500..600).contains(&r.status);
                retry_after = r.retry_after;
                Err(format!("HTTP {}", r.status))
            }
        };
        let mut st = self.lock();
        let f = st.feeds.entry(feed.key.clone()).or_default();
        match outcome {
            Ok(snap) => {
                f.failures = 0;
                f.blocked_until = None;
                f.last_error = None;
                f.last_success = Some(unix);
                st.snapshots.insert(feed.key.clone(), snap.clone());
                drop(st);
                write_cache(&self.cache_dir, &feed.key, &snap);
            }
            Err(e) => {
                f.last_error = Some(e);
                if backoff {
                    f.failures += 1;
                    let mut delay = BACKOFF_START;
                    for _ in 1..f.failures {
                        delay = (delay * 2).min(BACKOFF_MAX);
                    }
                    if let Some(ra) = retry_after {
                        delay = delay.max(ra.min(BACKOFF_MAX));
                    }
                    f.blocked_until = Some(now + delay);
                }
            }
        }
    }
}

fn cache_path(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{key}.json"))
}

fn read_cache(dir: &Path, key: &str) -> Option<Snapshot> {
    let text = std::fs::read_to_string(cache_path(dir, key)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    Some(Snapshot {
        body: Arc::new(s("body")?),
        fetched_at: v.get("fetched_at")?.as_u64()?,
        etag: s("etag"),
        last_modified: s("last_modified"),
    })
}

/// Best effort: a cache that cannot be written only costs a refetch.
fn write_cache(dir: &Path, key: &str, snap: &Snapshot) {
    let v = serde_json::json!({
        "fetched_at": snap.fetched_at,
        "etag": snap.etag,
        "last_modified": snap.last_modified,
        "body": snap.body.as_str(),
    });
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let path = cache_path(dir, key);
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, v.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// The real HTTP layer: blocking `ureq`, bodies capped.
pub struct UreqTransport;

impl Transport for UreqTransport {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(req.connect_timeout))
            .timeout_global(Some(req.total_timeout))
            .http_status_as_error(false)
            .build()
            .into();
        let mut r = agent.get(&req.url).header("User-Agent", &req.user_agent);
        if let Some(v) = &req.if_none_match {
            r = r.header("If-None-Match", v);
        }
        if let Some(v) = &req.if_modified_since {
            r = r.header("If-Modified-Since", v);
        }
        let mut resp = r.call().map_err(|e| e.to_string())?;
        let header = |name: &str| {
            resp.headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let etag = header("etag");
        let last_modified = header("last-modified");
        let retry_after = header("retry-after")
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .with_config()
            .limit(MAX_BODY_BYTES)
            .read_to_vec()
            .map_err(|e| e.to_string())?;
        Ok(HttpResponse {
            status,
            body,
            etag,
            last_modified,
            retry_after,
        })
    }
}
