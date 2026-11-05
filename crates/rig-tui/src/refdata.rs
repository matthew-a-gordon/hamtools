//! Reference data: POTA parks and SOTA summits, read offline from the cache.
//!
//! Everything is fetched through the one [`ReferenceClient`] (ADR 0003) and only on an explicit
//! request: [`ReferenceData::refresh_summits`], [`refresh_park_list`](ReferenceData::refresh_park_list),
//! [`refresh_park`](ReferenceData::refresh_park) (the "refresh reference data" action and saving a
//! plan), or the first lookup of an unknown reference ([`ReferenceData::lookup_or_fetch`]).
//! [`lookup`](ReferenceData::lookup) and [`search`](ReferenceData::search) read the cache only and
//! never fetch. Nothing is requested at construction.
//!
//! The only URLs are the public POTA JSON endpoints and the SOTA summit-list download
//! (a data file, not the SOTA API). There is no code path to the SOTA API: it needs the owner's
//! approval of this software first.
//!
//! The POTA JSON has no official documentation (UNVERIFIED as a stable contract: the shapes are
//! those observed live on 2026-11-04, see `docs/research/missing-data.md`), so it is parsed
//! defensively (numbers may be strings, optional fields may be absent, `/park/<ref>` may answer
//! `200` with `null`, `locationDesc` may list up to 16 comma-separated states). The summit list
//! download is a `302` to another host, followed by the client; it now has 17 columns and only
//! the first 14 are checked. The summit list's two header lines are checked on
//! parse and the load fails closed with a clear message if they differ.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::reference::{Feed, Pace, ReferenceClient, Snapshot};

pub const SUMMIT_LIST_URL: &str = "https://www.sotadata.org.uk/summitslist.csv";
const POTA_BASE: &str = "https://api.pota.app";
const SUMMITS_KEY: &str = "sota-summits";
const PARK_LIST_PREFIX: &str = "pota-parks-";
const PARK_PREFIX: &str = "pota-park-";
/// Park detail is not refetched within this long.
pub const PARK_DETAIL_FRESH: Duration = Duration::from_secs(24 * 60 * 60);

/// The first line of the summit list and the start of its column header, as of 2026-11.
const SUMMIT_TITLE: &str = "SOTA Summits List";
const SUMMIT_HEADER: &str = "SummitCode,AssociationName,RegionName,SummitName,AltM,AltFt,GridRef1,GridRef2,Longitude,Latitude,Points,BonusPoints,ValidFrom,ValidTo";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pota,
    Sota,
}

/// One park or summit.
#[derive(Debug, Clone, PartialEq)]
pub struct RefEntry {
    pub kind: Kind,
    pub reference: String,
    pub name: String,
    /// POTA location (`US-IN`) or SOTA association/region.
    pub location: Option<String>,
    pub grid: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub alt_m: Option<u32>,
    pub points: Option<u32>,
    /// SOTA only: the last day the summit is valid, as `yyyymmdd`.
    pub valid_to: Option<u32>,
}

/// A result with the age of the cached data it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub entry: RefEntry,
    pub age_secs: u64,
    /// Older than 30 days: usable, but shown in a warning color.
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    Found(Hit),
    /// Not in any cache (shown dim as "not cached").
    NotCached,
}

fn text_field(v: &serde_json::Value, key: &str) -> Option<String> {
    match v.get(key)? {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn num_field(v: &serde_json::Value, key: &str) -> Option<f64> {
    match v.get(key)? {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn park_from_json(v: &serde_json::Value) -> Option<RefEntry> {
    let reference = text_field(v, "reference")?.to_ascii_uppercase();
    let name = text_field(v, "name")?;
    let grid = text_field(v, "grid6")
        .or_else(|| text_field(v, "grid"))
        .or_else(|| text_field(v, "grid4"));
    Some(RefEntry {
        kind: Kind::Pota,
        reference,
        name,
        location: text_field(v, "locationDesc"),
        grid,
        lat: num_field(v, "latitude"),
        lon: num_field(v, "longitude"),
        alt_m: None,
        points: None,
        valid_to: None,
    })
}

/// A POTA park list (`/location/parks/<location>`): a JSON array. Entries missing a reference or
/// name are skipped.
pub fn parse_park_list(json: &str) -> Result<Vec<RefEntry>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("park list: {e}"))?;
    let arr = v.as_array().ok_or("park list: expected a JSON array")?;
    Ok(arr.iter().filter_map(park_from_json).collect())
}

/// POTA park detail (`/park/<ref>`): one object (a one-element array is accepted too). A body of
/// `null` (HTTP 200 for a reference POTA does not know, e.g. `K-0001`) is `Ok(None)`: unknown, not
/// an error.
pub fn parse_park_detail(json: &str) -> Result<Option<RefEntry>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("park: {e}"))?;
    if v.is_null() {
        return Ok(None);
    }
    let obj = match &v {
        serde_json::Value::Array(a) if a.len() == 1 => &a[0],
        other => other,
    };
    if !obj.is_object() {
        return Err("park: expected a JSON object".into());
    }
    park_from_json(obj)
        .map(Some)
        .ok_or_else(|| "park: no reference or name".to_string())
}

/// Split one CSV line, honouring double-quoted fields and `""` escapes.
fn csv_fields(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// `dd/mm/yyyy` to `yyyymmdd`.
fn date_key(s: &str) -> Option<u32> {
    let mut it = s.trim().split('/');
    let d: u32 = it.next()?.parse().ok()?;
    let m: u32 = it.next()?.parse().ok()?;
    let y: u32 = it.next()?.parse().ok()?;
    Some(y * 10_000 + m * 100 + d)
}

/// The SOTA summit list. The first line must be the `SOTA Summits List (...)` title and the
/// second the known column header; anything else is an error and nothing is loaded.
pub fn parse_summit_list(text: &str) -> Result<Vec<RefEntry>, String> {
    let mut lines = text.lines();
    let title = lines.next().unwrap_or("");
    let header = lines.next().unwrap_or("").trim_start_matches('\u{feff}');
    if !title
        .trim_start_matches('\u{feff}')
        .starts_with(SUMMIT_TITLE)
    {
        return Err(format!(
            "SOTA summit list: unexpected header line {:?}; refusing to load it",
            title.chars().take(60).collect::<String>()
        ));
    }
    if !header.starts_with(SUMMIT_HEADER) {
        return Err(
            "SOTA summit list: the column header has changed; refusing to load it".to_string(),
        );
    }
    let mut out = vec![];
    for line in lines {
        let f = csv_fields(line);
        if f.len() < 14 || f[0].is_empty() {
            continue;
        }
        let num = |i: usize| f[i].trim().parse::<f64>().ok();
        out.push(RefEntry {
            kind: Kind::Sota,
            reference: f[0].trim().to_ascii_uppercase(),
            name: f[3].trim().to_string(),
            location: Some(format!("{}, {}", f[1].trim(), f[2].trim())),
            grid: None,
            lat: num(9),
            lon: num(8),
            alt_m: f[4].trim().parse().ok(),
            points: f[10].trim().parse().ok(),
            valid_to: date_key(&f[13]),
        });
    }
    Ok(out)
}

type Parsed = Result<Arc<Vec<RefEntry>>, String>;

/// Reference data over the shared client and its cache directory.
pub struct ReferenceData {
    client: Arc<ReferenceClient>,
    cache_dir: PathBuf,
    /// Parsed snapshots by feed key, valid while `fetched_at` is unchanged.
    parsed: Mutex<HashMap<String, (u64, Parsed)>>,
    /// `--offline`: no refresh or first lookup asks the client for anything.
    offline: bool,
}

fn clean(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>()
}

fn hit(entry: &RefEntry, snap: &Snapshot, now_unix: u64) -> Hit {
    Hit {
        entry: entry.clone(),
        age_secs: snap.age_secs(now_unix),
        stale: snap.is_stale(now_unix),
    }
}

fn today_key(now_unix: u64) -> u32 {
    use chrono::Datelike;
    chrono::DateTime::from_timestamp(now_unix as i64, 0)
        .map_or(0, |d| d.year() as u32 * 10_000 + d.month() * 100 + d.day())
}

impl ReferenceData {
    /// Makes no request.
    pub fn new(client: Arc<ReferenceClient>, cache_dir: PathBuf) -> Self {
        Self {
            client,
            cache_dir,
            parsed: Mutex::new(HashMap::new()),
            offline: false,
        }
    }

    /// With `offline` set every refresh and first lookup is a no-op: the cache is all there is.
    pub fn with_offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    fn summits_feed() -> Feed {
        Feed::new(SUMMITS_KEY, SUMMIT_LIST_URL, Pace::Daily)
    }

    fn park_list_feed(location: &str) -> Feed {
        let loc = clean(location).to_ascii_uppercase();
        Feed::new(
            &format!("{PARK_LIST_PREFIX}{loc}"),
            &format!("{POTA_BASE}/location/parks/{loc}"),
            Pace::OnDemand,
        )
    }

    fn park_feed(reference: &str) -> Feed {
        let r = clean(reference).to_ascii_uppercase();
        Feed::new(
            &format!("{PARK_PREFIX}{r}"),
            &format!("{POTA_BASE}/park/{r}"),
            Pace::OnDemand,
        )
    }

    /// Explicit refresh of the SOTA summit list (at most daily, conditional requests).
    pub fn refresh_summits(&self) {
        if self.offline {
            return;
        }
        self.client.request(&Self::summits_feed());
    }

    /// Explicit refresh of one POTA location's park list, e.g. `US-IN`.
    pub fn refresh_park_list(&self, location: &str) {
        if self.offline {
            return;
        }
        self.client.request(&Self::park_list_feed(location));
    }

    /// Fetch one park's detail (plan save, explicit refresh), unless cached within 24 hours.
    pub fn refresh_park(&self, reference: &str, now_unix: u64) {
        if self.offline {
            return;
        }
        let feed = Self::park_feed(reference);
        if let Some(s) = self.client.snapshot(&feed.key)
            && s.age_secs(now_unix) < PARK_DETAIL_FRESH.as_secs()
        {
            return;
        }
        self.client.request(&feed);
    }

    fn parse_snapshot(
        &self,
        key: &str,
        parser: fn(&str) -> Result<Vec<RefEntry>, String>,
    ) -> Option<(Snapshot, Parsed)> {
        let snap = self.client.snapshot(key)?;
        let mut cache = self.parsed.lock().expect("parsed cache");
        if let Some((at, p)) = cache.get(key)
            && *at == snap.fetched_at
        {
            return Some((snap, p.clone()));
        }
        let p: Parsed = parser(&snap.body).map(Arc::new);
        cache.insert(key.to_string(), (snap.fetched_at, p.clone()));
        Some((snap, p))
    }

    fn cached_keys(&self, prefix: &str, exclude: Option<&str>) -> Vec<String> {
        let mut keys: Vec<String> = std::fs::read_dir(&self.cache_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let key = name.strip_suffix(".json")?;
                (key.starts_with(prefix) && exclude.is_none_or(|x| !key.starts_with(x)))
                    .then(|| key.to_string())
            })
            .collect();
        keys.sort();
        keys
    }

    /// Visit every cached entry with the snapshot it came from, retired summits excluded. Nothing
    /// is cloned: callers copy only what they keep.
    fn scan(&self, now_unix: u64, mut visit: impl FnMut(&RefEntry, &Snapshot)) {
        let today = today_key(now_unix);
        let mut push = |entries: &[RefEntry], snap: &Snapshot| {
            for e in entries {
                if e.valid_to.is_none_or(|v| v >= today) {
                    visit(e, snap);
                }
            }
        };
        for key in self.cached_keys(PARK_PREFIX, Some(PARK_LIST_PREFIX)) {
            if let Some((snap, Ok(p))) =
                self.parse_snapshot(&key, |t| parse_park_detail(t).map(|e| e.into_iter().collect()))
            {
                push(&p, &snap);
            }
        }
        for key in self.cached_keys(PARK_LIST_PREFIX, None) {
            if let Some((snap, Ok(p))) = self.parse_snapshot(&key, parse_park_list) {
                push(&p, &snap);
            }
        }
        if let Some((snap, Ok(p))) = self.parse_snapshot(SUMMITS_KEY, parse_summit_list) {
            push(&p, &snap);
        }
    }

    /// Why the cached summit list could not be used (a failed header check), if so.
    pub fn summit_list_error(&self) -> Option<String> {
        match self.parse_snapshot(SUMMITS_KEY, parse_summit_list)? {
            (_, Err(e)) => Some(e),
            _ => None,
        }
    }

    /// Age of the cached summit list, if any.
    pub fn summit_list_age(&self, now_unix: u64) -> Option<u64> {
        self.client
            .snapshot(SUMMITS_KEY)
            .map(|s| s.age_secs(now_unix))
    }

    /// Cache only. A reference that is in no cache is [`Lookup::NotCached`]; nothing is fetched.
    pub fn lookup(&self, reference: &str, now_unix: u64) -> Lookup {
        let want = reference.trim().to_ascii_uppercase();
        let mut found = None;
        self.scan(now_unix, |e, snap| {
            if found.is_none() && e.reference == want {
                found = Some(hit(e, snap, now_unix));
            }
        });
        found.map_or(Lookup::NotCached, Lookup::Found)
    }

    /// The first lookup of a reference: like [`lookup`](Self::lookup), but when nothing at all is
    /// cached for it, asks the client for the one source that could resolve it (a park's detail,
    /// or the summit list when none is cached yet). The result shows up in a later lookup.
    pub fn lookup_or_fetch(&self, reference: &str, now_unix: u64) -> Lookup {
        let found = self.lookup(reference, now_unix);
        if matches!(found, Lookup::Found(_)) {
            return found;
        }
        let r = reference.trim();
        if r.contains('/') {
            if self.client.snapshot(SUMMITS_KEY).is_none() {
                self.refresh_summits();
            }
        } else if !clean(r).is_empty() {
            self.refresh_park(r, now_unix);
        }
        found
    }

    /// Offline search by reference or any word of the name: every word of `query` must appear in
    /// the reference or the name (case-insensitive). Reference matches rank first.
    pub fn search(&self, query: &str, now_unix: u64, limit: usize) -> Vec<Hit> {
        let words: Vec<String> = query
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .collect();
        if words.is_empty() {
            return vec![];
        }
        let mut seen = std::collections::HashSet::new();
        let mut hits: Vec<(u8, Hit)> = vec![];
        self.scan(now_unix, |e, snap| {
            let r = e.reference.to_ascii_lowercase();
            let n = e.name.to_ascii_lowercase();
            if !words.iter().all(|w| r.contains(w) || n.contains(w))
                || !seen.insert(e.reference.clone())
            {
                return;
            }
            let rank = if words.len() == 1 && r == words[0] {
                0
            } else if words.iter().all(|w| r.starts_with(w)) {
                1
            } else if words.iter().all(|w| r.contains(w)) {
                2
            } else {
                3
            };
            hits.push((rank, hit(e, snap, now_unix)));
        });
        hits.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.entry.reference.cmp(&b.1.entry.reference))
        });
        hits.into_iter().take(limit).map(|(_, h)| h).collect()
    }
}
