//! User configuration (`~/.config/hamtools/config.toml`).

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const DEFAULT_ADDR: &str = "127.0.0.1:4532";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Operator callsign, uppercase. Substituted for `{call}` in text you send.
    pub callsign: Option<String>,
    pub kx3: Kx3Config,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kx3Config {
    /// Address of the KX3's rigctld.
    pub addr: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            callsign: None,
            kx3: Kx3Config {
                addr: DEFAULT_ADDR.to_string(),
            },
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Parse(String),
    Read(String),
    Callsign(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Parse(e) | ConfigError::Read(e) => write!(f, "config: {e}"),
            ConfigError::Callsign(c) => write!(
                f,
                "config: invalid callsign {c:?} (3-12 letters, digits or '/')"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    callsign: Option<String>,
    #[serde(default)]
    kx3: RawKx3,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawKx3 {
    addr: Option<String>,
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        let callsign = raw.callsign.map(normalize_callsign).transpose()?;
        Ok(Self {
            callsign,
            kx3: Kx3Config {
                addr: raw.kx3.addr.unwrap_or_else(|| DEFAULT_ADDR.to_string()),
            },
        })
    }
}

fn normalize_callsign(raw: String) -> Result<String, ConfigError> {
    let call = raw.trim().to_uppercase();
    let valid = (3..=12).contains(&call.len())
        && call
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '/');
    if valid {
        Ok(call)
    } else {
        Err(ConfigError::Callsign(raw))
    }
}

/// Where the config file lives: `$XDG_CONFIG_HOME/hamtools/config.toml`, falling back to
/// `$HOME/.config/hamtools/config.toml`.
pub fn config_path(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match xdg_config_home.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(home?).join(".config"),
    };
    Some(base.join("hamtools").join("config.toml"))
}

impl Config {
    /// Load configuration. An `explicit` path must exist; the `default` path may be absent, which
    /// yields the built-in defaults. A file that exists but is invalid is always an error.
    pub fn load(explicit: Option<&Path>, default: Option<&Path>) -> Result<Self, ConfigError> {
        let (path, required) = match (explicit, default) {
            (Some(path), _) => (path, true),
            (None, Some(path)) => (path, false),
            (None, None) => return Ok(Self::default()),
        };
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(|e| match e {
                ConfigError::Parse(msg) => ConfigError::Parse(format!("{}: {msg}", path.display())),
                other => other,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => Ok(Self::default()),
            Err(e) => Err(ConfigError::Read(format!("{}: {e}", path.display()))),
        }
    }
}
