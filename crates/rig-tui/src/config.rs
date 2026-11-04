//! Settings for this app, read through the shared `hamtools-config` crate.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hamtools_config::{ConfigError, ConfigFile, Grid, Warning};

const DEFAULT_ADDR: &str = "127.0.0.1:4532";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Operator callsign, uppercase. Substituted for `{CALL}` in text you send.
    pub callsign: Option<String>,
    /// The operator's Locator (`[station] locator`), for `MY_GRIDSQUARE`.
    pub locator: Option<Grid>,
    /// `[logbook] path`: where the shared Log lives, if set.
    pub log_path: Option<PathBuf>,
    /// Address of the rigctld to talk to.
    pub rig_addr: String,
    /// Soft key messages, keyed 1..=6 for F1..F6. Text is uppercase and may contain `{CALL}`.
    pub soft_keys: BTreeMap<u8, String>,
    /// Keys in the file this app does not know. Reported by `main`, never an error.
    pub warnings: Vec<Warning>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            callsign: None,
            locator: None,
            log_path: None,
            rig_addr: DEFAULT_ADDR.to_string(),
            soft_keys: BTreeMap::new(),
            warnings: Vec::new(),
        }
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
    /// yields the built-in defaults. A file that exists but is invalid is always an error naming
    /// the key.
    pub fn load(explicit: Option<&Path>, default: Option<&Path>) -> Result<Self, ConfigError> {
        let path = match (explicit, default) {
            (Some(path), _) => {
                if !path.exists() {
                    return Err(ConfigError::Read {
                        path: path.to_path_buf(),
                        message: "no such file".to_string(),
                    });
                }
                path
            }
            (None, Some(path)) => path,
            (None, None) => return Ok(Self::default()),
        };
        let file = ConfigFile::load(path)?;
        let settings = file.settings();
        Ok(Self {
            callsign: settings.callsign.as_ref().map(|c| c.as_str().to_string()),
            locator: settings.locator.clone(),
            log_path: settings.log_path.clone(),
            rig_addr: settings
                .rig_addr
                .clone()
                .unwrap_or_else(|| DEFAULT_ADDR.to_string()),
            soft_keys: settings.macros.clone(),
            warnings: file.warnings().to_vec(),
        })
    }
}

/// Where the Log lives: `[logbook] path` from the config, else `$XDG_DATA_HOME/hamtools/log.adi`,
/// falling back to `$HOME/.local/share/hamtools/log.adi`. The same rule as sat-tui, so both tools
/// share one file.
pub fn log_path(
    config: Option<&Path>,
    xdg_data_home: Option<&str>,
    home: Option<&str>,
) -> Option<PathBuf> {
    if let Some(path) = config {
        return Some(path.to_path_buf());
    }
    let base = match xdg_data_home.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(home?).join(".local").join("share"),
    };
    Some(base.join("hamtools").join("log.adi"))
}
