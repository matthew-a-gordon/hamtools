//! User configuration (`~/.config/hamtools/sat-tui.toml`).

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use logbook::{Callsign, Grid};
use serde::Deserialize;

use crate::observer::Spec;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// NORAD ids to show; empty means every satellite with a usable transmitter.
    pub watch: Vec<u32>,
    /// Address of the FT-847's rigctld.
    pub rig_addr: String,
    /// How far ahead the pass list looks.
    pub horizon_hours: u32,
    /// A pass counts only if it climbs at least this high.
    pub min_elevation_deg: f64,
    /// A pass shorter than this many seconds is flagged as short.
    pub short_pass_secs: i64,
    /// Where the operator is, as far as the file says (see [`crate::observer::resolve`]).
    pub observer: Spec,
    pub logbook: LogbookConfig,
}

/// The `[logbook]` section. The callsign and the Log path are not here: they come from the
/// shared config.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LogbookConfig {
    /// Own locator, used verbatim instead of the observer's.
    pub grid: Option<Grid>,
    /// Satellite name as shown in the UI, to the name written as `SAT_NAME`.
    pub satellites: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            watch: Vec::new(),
            rig_addr: "127.0.0.1:4533".to_string(),
            horizon_hours: 24,
            min_elevation_deg: 10.0,
            short_pass_secs: 180,
            observer: Spec::default(),
            logbook: LogbookConfig::default(),
        }
    }
}

#[derive(Debug)]
pub struct ConfigError(String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "config: {}", self.0)
    }
}

impl std::error::Error for ConfigError {}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Raw {
    watch: Option<Vec<u32>>,
    #[serde(default)]
    rig: RawRig,
    #[serde(default)]
    passes: RawPasses,
    #[serde(default)]
    observer: RawObserver,
    #[serde(default)]
    logbook: RawLogbook,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawLogbook {
    grid: Option<String>,
    #[serde(default)]
    satellites: BTreeMap<String, String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawRig {
    addr: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawPasses {
    horizon_hours: Option<u32>,
    min_elevation_deg: Option<f64>,
    short_pass_secs: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawObserver {
    grid: Option<String>,
    lat_deg: Option<f64>,
    lon_deg: Option<f64>,
    alt_m: Option<f64>,
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let raw: Raw = toml::from_str(text).map_err(|e| ConfigError(e.to_string()))?;
        let defaults = Self::default();
        let horizon_hours = raw.passes.horizon_hours.unwrap_or(defaults.horizon_hours);
        if !(1..=168).contains(&horizon_hours) {
            return Err(ConfigError(format!(
                "passes.horizon_hours {horizon_hours} must be 1 to 168"
            )));
        }
        let min_elevation_deg = raw
            .passes
            .min_elevation_deg
            .unwrap_or(defaults.min_elevation_deg);
        if !(0.0..=90.0).contains(&min_elevation_deg) {
            return Err(ConfigError(format!(
                "passes.min_elevation_deg {min_elevation_deg} must be 0 to 90"
            )));
        }
        let short_pass_secs = raw
            .passes
            .short_pass_secs
            .unwrap_or(defaults.short_pass_secs);
        if !(1..=3600).contains(&short_pass_secs) {
            return Err(ConfigError(format!(
                "passes.short_pass_secs {short_pass_secs} must be 1 to 3600"
            )));
        }
        let grid = raw
            .logbook
            .grid
            .map(|g| {
                Grid::new(&g).map_err(|_| {
                    ConfigError(format!(
                        "logbook.grid {g:?} must be a 6-character locator such as EN71DQ"
                    ))
                })
            })
            .transpose()?;
        Ok(Self {
            watch: raw.watch.unwrap_or(defaults.watch),
            rig_addr: raw.rig.addr.unwrap_or(defaults.rig_addr),
            horizon_hours,
            min_elevation_deg,
            short_pass_secs,
            observer: Spec {
                grid: raw.observer.grid,
                lat_deg: raw.observer.lat_deg,
                lon_deg: raw.observer.lon_deg,
                alt_m: raw.observer.alt_m,
            },
            logbook: LogbookConfig {
                grid,
                satellites: raw.logbook.satellites,
            },
        })
    }

    /// Load configuration. An `explicit` path must exist; the `default` path may be absent,
    /// which yields the built-in defaults. A file that exists but is invalid is always an error.
    pub fn load(explicit: Option<&Path>, default: Option<&Path>) -> Result<Self, ConfigError> {
        let (path, required) = match (explicit, default) {
            (Some(path), _) => (path, true),
            (None, Some(path)) => (path, false),
            (None, None) => return Ok(Self::default()),
        };
        match std::fs::read_to_string(path) {
            Ok(text) => {
                Self::parse(&text).map_err(|e| ConfigError(format!("{}: {}", path.display(), e.0)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => Ok(Self::default()),
            Err(e) => Err(ConfigError(format!("{}: {e}", path.display()))),
        }
    }
}

/// `$XDG_CONFIG_HOME/hamtools/sat-tui.toml`, falling back to `$HOME/.config/hamtools/sat-tui.toml`.
pub fn config_path(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match xdg_config_home.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(home?).join(".config"),
    };
    Some(base.join("hamtools").join("sat-tui.toml"))
}

/// The config shared with the other tools: `$XDG_CONFIG_HOME/hamtools/config.toml`, falling back
/// to `$HOME/.config/hamtools/config.toml`. sat-tui reads only the callsign, Locator and Log
/// path from it.
pub fn shared_config_path(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match xdg_config_home.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(home?).join(".config"),
    };
    Some(base.join("hamtools").join("config.toml"))
}

/// What sat-tui takes from the shared config, read through `hamtools-config` (which owns the
/// schema).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shared {
    pub callsign: Option<Callsign>,
    /// The operator's Locator: the position source after the flags and `[observer]`.
    pub locator: Option<Grid>,
    /// `[logbook] path`: where the shared Log lives.
    pub log_path: Option<PathBuf>,
}

/// Read the shared config without ever failing. A missing file (or `None`) is all defaults; an
/// unreadable one is defaults plus a note; one with bad keys keeps the good ones and notes only
/// errors in the keys sat-tui uses (a mistake in another tool's section is not its business). The note says why logging is
/// off.
pub fn load_shared(path: Option<&Path>) -> (Shared, Option<String>) {
    let Some(path) = path else {
        return (Shared::default(), None);
    };
    match hamtools_config::load_lenient(path) {
        Ok((s, errs)) => {
            let shared = Shared {
                callsign: s.callsign,
                locator: s.locator,
                log_path: s.log_path,
            };
            // Only errors in the keys sat-tui consumes (or in a part of the file that could not
            // be told apart from them) are worth a note; `[rig]`, `[kx3]` and `[macros]` belong
            // to other tools.
            let mine = |e: &&hamtools_config::ConfigError| match e {
                hamtools_config::ConfigError::Invalid { key, .. } => {
                    key.is_empty()
                        || key == "callsign"
                        || key == "station"
                        || key == "logbook"
                        || key.starts_with("station.")
                        || key.starts_with("logbook.")
                }
                _ => true,
            };
            let note = errs.iter().find(mine).map(|e| format!("logging off: {e}"));
            (shared, note)
        }
        Err(e) => (Shared::default(), Some(format!("logging off: {e}"))),
    }
}

/// Where the Log lives: `--log`, else the shared config's `[logbook] path`, else `$XDG_DATA_HOME/hamtools/log.adi`,
/// falling back to `$HOME/.local/share/hamtools/log.adi`.
pub fn log_path(
    cli: Option<&Path>,
    config: Option<&Path>,
    xdg_data_home: Option<&str>,
    home: Option<&str>,
) -> Option<PathBuf> {
    if let Some(path) = cli.or(config) {
        return Some(path.to_path_buf());
    }
    let base = match xdg_data_home.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(home?).join(".local").join("share"),
    };
    Some(base.join("hamtools").join("log.adi"))
}
