//! User configuration (`~/.config/hamtools/config.toml`).

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const DEFAULT_ADDR: &str = "127.0.0.1:4532";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Operator callsign, uppercase. Substituted for `{call}` in text you send.
    pub callsign: Option<String>,
    pub kx3: Kx3Config,
    /// Function-key macros, keyed 1..=6 for F1..F6. Text is uppercase and may contain `{CALL}`.
    pub macros: BTreeMap<u8, String>,
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
            macros: BTreeMap::new(),
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
    Macro(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Parse(e) | ConfigError::Read(e) => write!(f, "config: {e}"),
            ConfigError::Macro(m) => write!(f, "config: invalid macro {m}"),
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
    #[serde(default)]
    macros: BTreeMap<String, String>,
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
        let macros = raw
            .macros
            .into_iter()
            .map(|(key, text)| parse_macro(&key, &text))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            callsign,
            macros,
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

/// `key` must be `F1`..`F6`; `text` must not be blank. Returns the key number and uppercase text.
fn parse_macro(key: &str, text: &str) -> Result<(u8, String), ConfigError> {
    let number = key
        .strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .filter(|n| (1..=6).contains(n) && key == format!("F{n}"))
        .ok_or_else(|| ConfigError::Macro(format!("{key:?} (only F1 to F6 are supported)")))?;
    let text = text.trim().to_uppercase();
    if text.is_empty() {
        return Err(ConfigError::Macro(format!("{key} has no text")));
    }
    Ok((number, text))
}
