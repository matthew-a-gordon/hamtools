//! Settings for this app, read through the shared `hamtools-config` crate.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hamtools_config::{ConfigError, ConfigFile, LicenseClass, Warning};

const DEFAULT_ADDR: &str = "127.0.0.1:4532";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Operator callsign, uppercase. Substituted for `{CALL}` in text you send.
    pub callsign: Option<String>,
    /// Address of the rigctld to talk to.
    pub rig_addr: String,
    /// The operator's 6-character Locator, uppercase.
    pub locator: Option<String>,
    /// `station.license_class`; no default.
    pub license_class: Option<LicenseClass>,
    /// Where the Log lives; `None` means the default.
    pub log_path: Option<PathBuf>,
    /// Soft key messages, keyed 1..=6 for F1..F6. Text is uppercase and may contain variables such as `{MYCALL}`.
    pub soft_keys: BTreeMap<u8, String>,
    /// Named Soft key sets from `[softkeys.<name>]`.
    pub softkey_sets: BTreeMap<String, BTreeMap<u8, String>>,
    /// Keys in the file this app does not know. Reported by `main`, never an error.
    pub warnings: Vec<Warning>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            callsign: None,
            rig_addr: DEFAULT_ADDR.to_string(),
            locator: None,
            license_class: None,
            log_path: None,
            soft_keys: BTreeMap::new(),
            softkey_sets: BTreeMap::new(),
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
            rig_addr: settings
                .rig_addr
                .clone()
                .unwrap_or_else(|| DEFAULT_ADDR.to_string()),
            locator: settings.locator.as_ref().map(|g| g.as_str().to_string()),
            license_class: settings.license_class,
            log_path: settings.log_path.clone(),
            soft_keys: settings.macros.clone(),
            softkey_sets: settings.softkey_sets.clone(),
            warnings: file.warnings().to_vec(),
        })
    }
}

/// The default Log: `$XDG_DATA_HOME/hamtools/log.adi`, falling back to
/// `$HOME/.local/share/hamtools/log.adi`. The same place sat-tui uses, so both tools share one
/// file. The config's `[logbook] path` overrides it.
pub fn default_log_path(xdg_data_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match xdg_data_home.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(home?).join(".local").join("share"),
    };
    Some(base.join("hamtools").join("log.adi"))
}
