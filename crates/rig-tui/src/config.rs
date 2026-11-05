//! Settings for this app, read through the shared `hamtools-config` crate.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hamtools_config::{
    ConfigError, ConfigFile, LicenseClass, RigProfile, SatelliteSettings, Warning,
};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Config {
    /// Operator callsign, uppercase. Substituted for `{CALL}` in text you send.
    pub callsign: Option<String>,
    /// The rig profiles, in file order. Empty: one stand-in rig on the default address.
    pub rigs: Vec<RigProfile>,
    /// `rig.default`: the profile to start on; `None` means the first.
    pub rig_default: Option<String>,
    /// The operator's 6-character Locator, uppercase.
    pub locator: Option<String>,
    /// `station.license_class`; no default.
    pub license_class: Option<LicenseClass>,
    /// Where the Log lives; `None` means the default.
    pub log_path: Option<PathBuf>,
    /// `[export] dir`; `None` means `~/hamtools-exports/`.
    pub export_dir: Option<PathBuf>,
    /// Soft key messages, keyed 1..=6 for F1..F6. Text is uppercase and may contain variables such as `{MYCALL}`.
    pub soft_keys: BTreeMap<u8, String>,
    /// Named Soft key sets from `[softkeys.<name>]`.
    pub softkey_sets: BTreeMap<String, BTreeMap<u8, String>>,
    /// The `[satellite]` table (defaults when absent).
    pub satellite: SatelliteSettings,
    /// Keys in the file this app does not know. Reported by `main`, never an error.
    pub warnings: Vec<Warning>,
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
            rigs: settings.rigs.clone(),
            rig_default: settings.rig_default.clone(),
            locator: settings.locator.as_ref().map(|g| g.as_str().to_string()),
            license_class: settings.license_class,
            log_path: settings.log_path.clone(),
            export_dir: settings.export_dir.clone(),
            soft_keys: settings.macros.clone(),
            softkey_sets: settings.softkey_sets.clone(),
            satellite: settings.satellite.clone(),
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
