//! User configuration (`~/.config/hamtools/sat-tui.toml`).

use std::fmt;
use std::path::{Path, PathBuf};

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
    /// Where the operator is, as far as the file says (see [`crate::observer::resolve`]).
    pub observer: Spec,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            watch: Vec::new(),
            rig_addr: "127.0.0.1:4533".to_string(),
            horizon_hours: 24,
            min_elevation_deg: 10.0,
            observer: Spec::default(),
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
        Ok(Self {
            watch: raw.watch.unwrap_or(defaults.watch),
            rig_addr: raw.rig.addr.unwrap_or(defaults.rig_addr),
            horizon_hours,
            min_elevation_deg,
            observer: Spec {
                grid: raw.observer.grid,
                lat_deg: raw.observer.lat_deg,
                lon_deg: raw.observer.lon_deg,
                alt_m: raw.observer.alt_m,
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
