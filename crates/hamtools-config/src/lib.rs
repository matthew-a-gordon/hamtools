//! The shared `~/.config/hamtools/config.toml`: a typed read and a field-level write.
//!
//! The file is hand-edited as well as written by the apps, so a save must never reformat it.
//! [`ConfigFile`] therefore keeps the text as a `toml_edit` document, with [`Settings`] as the
//! typed view, and a save patches only the fields that changed. Comments, blank lines, key order
//! and keys this crate does not know (other tools' settings) stay byte for byte as written.
//!
//! # Dependency justification
//!
//! `toml_edit` is the one new crate in the workspace. Comment-, order- and unknown-key-preserving
//! writes cannot be done with a read-only parser such as `toml`. The content hash used for change
//! detection is `std`'s; nothing here needs Nix, a network or a rig.
//!
//! # Schema
//!
//! ```toml
//! callsign = "W1AW"
//! [station]
//! locator = "EN71DQ"      # 6 characters; latitude and longitude are never stored
//! license_class = "general"  # technician, general, extra, advanced, novice; no default
//! [logbook]
//! path = "/home/me/log.adi"
//! [rig]
//! default = "kx3"         # optional; the first profile when absent
//! [rig.kx3]               # one table per rig: a name, an address and a kind
//! addr = "127.0.0.1:4532"
//! kind = "kx3"            # "kx3", "ft847" or "generic"
//! cw = true               # optional capability overrides: cw, split, sat_mode
//! [satellite]             # optional: watch, horizon_hours, min_elevation_deg, short_pass_secs
//! watch = [25544]         # NORAD ids; empty or absent: every satellite with a usable transmitter
//! [satellite.names]       # name shown in the UI = name written as SAT_NAME
//! [macros]                # F1 to F6: the General Soft key set
//! F1 = "CQ DE {MYCALL}"
//! [softkeys.pota]         # a named set, same shape
//! F1 = "CQ POTA DE {MYCALL}"
//! ```
//!
//! The old single `[rig] addr` and the `[kx3]` table are not read: they are rejected with a
//! message naming the replacement (no compatibility, see ADR 0004).
//!
//! Unknown keys are [`Warning`]s, never errors and never dropped; a wrong value for a known key
//! is a [`ConfigError::Invalid`] naming the key.

pub mod bandplan;

use std::collections::BTreeMap;
use std::fmt;
use std::hash::{DefaultHasher, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

pub use logbook::{Callsign, Grid};
use toml_edit::{Document, DocumentMut, Item, Table, Value};

/// The operator's US license class. Runtime config with no default: the app never guesses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LicenseClass {
    Technician,
    General,
    Extra,
    Advanced,
    Novice,
}

impl LicenseClass {
    pub const ALL: [LicenseClass; 5] = [
        LicenseClass::Technician,
        LicenseClass::General,
        LicenseClass::Extra,
        LicenseClass::Advanced,
        LicenseClass::Novice,
    ];

    /// The lowercase name used in the config file.
    pub fn as_str(self) -> &'static str {
        match self {
            LicenseClass::Technician => "technician",
            LicenseClass::General => "general",
            LicenseClass::Extra => "extra",
            LicenseClass::Advanced => "advanced",
            LicenseClass::Novice => "novice",
        }
    }

    /// Parse a class name, ignoring case and surrounding space.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|c| c.as_str() == text)
    }
}

impl fmt::Display for LicenseClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The typed view of the file. `None` and an absent macro mean "not set".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    pub callsign: Option<Callsign>,
    /// The operator's 6-character Locator.
    pub locator: Option<Grid>,
    pub log_path: Option<PathBuf>,
    /// `station.license_class`; no default.
    pub license_class: Option<LicenseClass>,
    /// The named rig profiles, in file order.
    pub rigs: Vec<RigProfile>,
    /// `rig.default`: the profile to start on. `None`: the first.
    pub rig_default: Option<String>,
    /// Soft key text for F1..=F6, uppercase, may contain variables such as `{MYCALL}`. The
    /// General Soft key set.
    pub macros: BTreeMap<u8, String>,
    /// Named Soft key sets from `[softkeys.<name>]`, same shape as `macros`. Read only here; the
    /// config dialog does not write them.
    pub softkey_sets: BTreeMap<String, BTreeMap<u8, String>>,
    /// The `[satellite]` table. Read only here: the config dialog does not write it.
    pub satellite: SatelliteSettings,
}

/// The `[satellite]` table: what shapes the Satellite Activity's pass list. Every key is
/// optional; the defaults are those `sat-tui` had.
#[derive(Clone, Debug, PartialEq)]
pub struct SatelliteSettings {
    /// NORAD ids to show; empty means every satellite with a usable transmitter.
    pub watch: Vec<u32>,
    /// How far ahead the pass list looks, 1 to 168 hours.
    pub horizon_hours: u32,
    /// A pass counts only if it climbs at least this high, 0 to 90 degrees.
    pub min_elevation_deg: f64,
    /// A pass shorter than this many seconds is flagged as short, 1 to 3600.
    pub short_pass_secs: i64,
    /// Satellite name as shown in the UI, to the name written as `SAT_NAME`
    /// (`[satellite.names]`).
    pub names: BTreeMap<String, String>,
}

impl Default for SatelliteSettings {
    fn default() -> Self {
        Self {
            watch: Vec::new(),
            horizon_hours: 24,
            min_elevation_deg: 10.0,
            short_pass_secs: 180,
            names: BTreeMap::new(),
        }
    }
}

/// The kind of radio a profile describes. It keys the capability table in the apps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RigKind {
    Kx3,
    Ft847,
    /// Any other rig behind a `rigctld`: nothing is assumed.
    Generic,
}

impl RigKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RigKind::Kx3 => "kx3",
            RigKind::Ft847 => "ft847",
            RigKind::Generic => "generic",
        }
    }

    fn parse(text: &str) -> Option<RigKind> {
        match text {
            "kx3" => Some(RigKind::Kx3),
            "ft847" => Some(RigKind::Ft847),
            "generic" => Some(RigKind::Generic),
            _ => None,
        }
    }
}

/// One `[rig.NAME]` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RigProfile {
    pub name: String,
    /// `host:port` of this rig's rigctld.
    pub addr: String,
    pub kind: RigKind,
    /// Per-profile overrides of the kind's capability row. `None`: use the table.
    pub cw: Option<bool>,
    pub split: Option<bool>,
    pub sat_mode: Option<bool>,
}

impl Settings {
    pub fn rig(&self, name: &str) -> Option<&RigProfile> {
        self.rigs.iter().find(|r| r.name == name)
    }
}

/// A key the crate does not know. It is kept in the file and only reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    /// 1-based line of the key (0 if unknown).
    pub line: usize,
    /// Dotted name such as `watch` or `station.foo`.
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    Read {
        path: PathBuf,
        message: String,
    },
    /// Not valid TOML.
    Parse {
        line: usize,
        message: String,
    },
    /// A known key with a wrong value.
    Invalid {
        key: String,
        message: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Read { path, message } => {
                write!(f, "config: {}: {message}", path.display())
            }
            ConfigError::Parse { line, message } => write!(f, "config: line {line}: {message}"),
            ConfigError::Invalid { key, message } => write!(f, "config: {key}: {message}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// One field edited here and differently on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldConflict {
    pub key: String,
    /// The value in the file now.
    pub file: Option<String>,
    /// The value being saved.
    pub mine: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// A symlink or non-writable target is never replaced.
    ReadOnly {
        path: PathBuf,
        reason: String,
    },
    /// The file on disk no longer parses; it is never saved over.
    Unparseable {
        line: usize,
        message: String,
    },
    /// The file changed and the same fields were changed differently. Nothing was written.
    Conflict(Vec<FieldConflict>),
    Io(String),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::ReadOnly { path, reason } => {
                write!(f, "{} is read-only ({reason})", path.display())
            }
            SaveError::Unparseable { line, message } => {
                write!(
                    f,
                    "config file is unparseable (line {line}: {message}); not saved"
                )
            }
            SaveError::Conflict(c) => write!(f, "config changed on disk ({} conflicting)", c.len()),
            SaveError::Io(m) => write!(f, "config: {m}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// What a successful save did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
    Written,
    /// The file had changed in untouched fields; those changes were kept.
    Rebased,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    mtime: Option<SystemTime>,
    size: u64,
    hash: u64,
}

/// The loaded file: text, typed settings, warnings and what is needed to detect disk changes.
#[derive(Debug)]
pub struct ConfigFile {
    path: PathBuf,
    doc: DocumentMut,
    settings: Settings,
    warnings: Vec<Warning>,
    /// `None` when the file did not exist at load.
    stamp: Option<Stamp>,
}

impl ConfigFile {
    /// Read and validate `path`. A missing file yields defaults (and is created on first save).
    pub fn load(path: &Path) -> Result<ConfigFile, ConfigError> {
        let (text, stamp) = match read_text(path)? {
            Some((text, stamp)) => (text, Some(stamp)),
            None => (String::new(), None),
        };
        let (settings, warnings) = parse(&text)?;
        let doc = text
            .parse::<DocumentMut>()
            .map_err(|e| parse_error(&text, e))?;
        Ok(ConfigFile {
            path: path.to_path_buf(),
            doc,
            settings,
            warnings,
            stamp,
        })
    }

    /// Discard in-memory state and read the file again.
    pub fn reload(&mut self) -> Result<(), ConfigError> {
        *self = ConfigFile::load(&self.path)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// Whether the file differs from what was loaded: mtime and size first, then a content hash.
    pub fn changed_on_disk(&self) -> bool {
        let meta = std::fs::metadata(&self.path);
        let Some(stamp) = &self.stamp else {
            return meta.is_ok();
        };
        let Ok(meta) = meta else { return true };
        if meta.len() == stamp.size && meta.modified().ok() == stamp.mtime {
            return false;
        }
        match std::fs::read(&self.path) {
            Ok(bytes) => hash(&bytes) != stamp.hash,
            Err(_) => true,
        }
    }

    /// Write `new`, patching only the fields that differ from what was loaded.
    ///
    /// If the file changed on disk, the edits are rebased onto the fresh text when those fields
    /// are untouched there, and returned as a [`SaveError::Conflict`] when they are not.
    pub fn save(&mut self, new: &Settings) -> Result<Saved, SaveError> {
        self.check_writable()?;
        let edited: Vec<Field> = Field::all(&self.settings, new)
            .into_iter()
            .filter(|f| f.get(&self.settings) != f.get(new))
            .collect();

        let mut rebased = false;
        let mut doc = self.doc.clone();
        if self.changed_on_disk() {
            let text = match read_text(&self.path) {
                Ok(t) => t.map(|(t, _)| t).unwrap_or_default(),
                Err(e) => return Err(SaveError::Io(e.to_string())),
            };
            let unparseable = |e: ConfigError| match e {
                ConfigError::Parse { line, message } => SaveError::Unparseable { line, message },
                ConfigError::Invalid { key, message } => SaveError::Unparseable {
                    line: 0,
                    message: format!("{key}: {message}"),
                },
                ConfigError::Read { message, .. } => SaveError::Io(message),
            };
            let (disk, _) = parse(&text).map_err(unparseable)?;
            let conflicts: Vec<FieldConflict> = edited
                .iter()
                .filter(|f| f.get(&disk) != f.get(&self.settings) && f.get(&disk) != f.get(new))
                .map(|f| FieldConflict {
                    key: f.key(),
                    file: f.get(&disk),
                    mine: f.get(new),
                })
                .collect();
            if !conflicts.is_empty() {
                return Err(SaveError::Conflict(conflicts));
            }
            doc = text
                .parse::<DocumentMut>()
                .map_err(|e| unparseable(parse_error(&text, e)))?;
            rebased = true;
        }

        for field in &edited {
            field.set(&mut doc, new);
        }
        let text = doc.to_string();
        write_atomic(&self.path, &text)?;

        let (settings, warnings) = parse(&text).map_err(|e| SaveError::Io(e.to_string()))?;
        self.stamp = read_text(&self.path).ok().flatten().map(|(_, s)| s);
        self.doc = doc;
        self.settings = settings;
        self.warnings = warnings;
        Ok(if rebased {
            Saved::Rebased
        } else {
            Saved::Written
        })
    }

    /// Whether a save could replace the file: a symlink or non-writable target never can, so a
    /// dialog can say so before the operator edits anything.
    pub fn writable(&self) -> Result<(), SaveError> {
        self.check_writable()
    }

    fn check_writable(&self) -> Result<(), SaveError> {
        let read_only = |reason: &str| SaveError::ReadOnly {
            path: self.path.clone(),
            reason: reason.to_string(),
        };
        match std::fs::symlink_metadata(&self.path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                Err(read_only("a symlink, possibly managed elsewhere"))
            }
            Ok(meta) if meta.permissions().readonly() => Err(read_only("not writable")),
            _ => Ok(()),
        }
    }
}

fn hash(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    h.write(bytes);
    h.finish()
}

fn read_text(path: &Path) -> Result<Option<(String, Stamp)>, ConfigError> {
    let read_err = |e: std::io::Error| ConfigError::Read {
        path: path.to_path_buf(),
        message: e.to_string(),
    };
    match std::fs::read(path) {
        Ok(bytes) => {
            let meta = std::fs::metadata(path).map_err(read_err)?;
            let stamp = Stamp {
                mtime: meta.modified().ok(),
                size: bytes.len() as u64,
                hash: hash(&bytes),
            };
            let text = String::from_utf8(bytes).map_err(|e| ConfigError::Read {
                path: path.to_path_buf(),
                message: e.to_string(),
            })?;
            Ok(Some((text, stamp)))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(read_err(e)),
    }
}

static TMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Temp file in the same directory, `fsync`, rename; the temp file is removed on failure.
fn write_atomic(path: &Path, text: &str) -> Result<(), SaveError> {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let io = |e: std::io::Error| match e.kind() {
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem => {
            SaveError::ReadOnly {
                path: path.to_path_buf(),
                reason: e.to_string(),
            }
        }
        _ => SaveError::Io(format!("{}: {e}", path.display())),
    };
    std::fs::create_dir_all(&dir).map_err(io)?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let tmp = dir.join(format!(
        ".{name}.tmp-{}-{}",
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = std::fs::File::create_new(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        if let Ok(meta) = std::fs::metadata(path) {
            file.set_permissions(meta.permissions())?;
        }
        drop(file);
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(e));
    }
    if let Ok(d) = std::fs::File::open(&dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

// ---- fields -------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum Field {
    Callsign,
    Locator,
    LicenseClass,
    LogPath,
    RigDefault,
    /// The address of the named profile.
    RigAddr(String),
    Macro(u8),
}

impl Field {
    /// Every field that can differ between `a` and `b`.
    fn all(a: &Settings, b: &Settings) -> Vec<Field> {
        let mut fields = vec![
            Field::Callsign,
            Field::Locator,
            Field::LicenseClass,
            Field::LogPath,
            Field::RigDefault,
        ];
        let mut names: Vec<&str> = Vec::new();
        for rig in a.rigs.iter().chain(&b.rigs) {
            if !names.contains(&rig.name.as_str()) {
                names.push(&rig.name);
            }
        }
        fields.extend(names.into_iter().map(|n| Field::RigAddr(n.to_string())));
        fields.extend((1..=6).map(Field::Macro));
        fields
    }

    fn key(&self) -> String {
        match self {
            Field::Callsign => "callsign".into(),
            Field::Locator => "station.locator".into(),
            Field::LicenseClass => "station.license_class".into(),
            Field::LogPath => "logbook.path".into(),
            Field::RigDefault => "rig.default".into(),
            Field::RigAddr(name) => format!("rig.{name}.addr"),
            Field::Macro(n) => format!("macros.F{n}"),
        }
    }

    /// The field as text, for comparing and for reporting a conflict.
    fn get(&self, s: &Settings) -> Option<String> {
        match self {
            Field::Callsign => s.callsign.as_ref().map(|c| c.as_str().to_string()),
            Field::Locator => s.locator.as_ref().map(|g| g.as_str().to_string()),
            Field::LicenseClass => s.license_class.map(|c| c.as_str().to_string()),
            Field::LogPath => s
                .log_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            Field::RigDefault => s.rig_default.clone(),
            Field::RigAddr(name) => s.rig(name).map(|r| r.addr.clone()),
            Field::Macro(n) => s.macros.get(n).cloned(),
        }
    }

    fn set(&self, doc: &mut DocumentMut, new: &Settings) {
        let value = self.get(new);
        match self {
            Field::Callsign => put(doc.as_table_mut(), "callsign", value),
            Field::Locator => put(table(doc, "station"), "locator", value),
            Field::LicenseClass => put(table(doc, "station"), "license_class", value),
            Field::LogPath => put(table(doc, "logbook"), "path", value),
            Field::RigDefault => {
                let rig = table(doc, "rig");
                // Created by a `[rig.NAME]` header alone, the table is implicit and would not
                // print a header for its own keys.
                rig.set_implicit(false);
                put(rig, "default", value);
            }
            Field::RigAddr(name) => {
                let rig = table(doc, "rig");
                if !rig.contains_key(name) {
                    rig.insert(name, Item::Table(Table::new()));
                }
                let profile = rig[name.as_str()].as_table_mut().expect("validated");
                put(profile, "addr", value);
            }
            Field::Macro(n) => put(table(doc, "macros"), &format!("F{n}"), value),
        }
    }
}

/// The named table, created (at the end of the file) if missing.
fn table<'a>(doc: &'a mut DocumentMut, name: &str) -> &'a mut Table {
    if !doc.contains_key(name) {
        doc.insert(name, Item::Table(Table::new()));
    }
    doc[name].as_table_mut().expect("validated as a table")
}

/// Set or remove a string key, keeping the existing value's surrounding whitespace and comment.
fn put(table: &mut Table, key: &str, value: Option<String>) {
    match value {
        None => {
            table.remove(key);
        }
        Some(text) => match table.get_mut(key).and_then(Item::as_value_mut) {
            Some(old) => {
                let decor = old.decor().clone();
                let mut new = Value::from(text);
                *new.decor_mut() = decor;
                *old = new;
            }
            None => {
                table.insert(key, toml_edit::value(text));
            }
        },
    }
}

// ---- parsing ------------------------------------------------------------------------------

fn parse_error(text: &str, e: toml_edit::TomlError) -> ConfigError {
    ConfigError::Parse {
        line: line_at(text, e.span()),
        message: e.message().to_string(),
    }
}

fn invalid(key: &str, message: impl fmt::Display) -> ConfigError {
    ConfigError::Invalid {
        key: key.to_string(),
        message: message.to_string(),
    }
}

/// A string value, or an error naming `key`.
fn string<'a>(item: &'a Item, key: &str) -> Result<&'a str, ConfigError> {
    item.as_str()
        .ok_or_else(|| invalid(key, "expected a string"))
}

fn sub_table<'a>(root: &'a Table, name: &str) -> Result<Option<&'a Table>, ConfigError> {
    match root.get(name) {
        None => Ok(None),
        Some(item) => item
            .as_table()
            .map(Some)
            .ok_or_else(|| invalid(name, "expected a table")),
    }
}

fn host_port(addr: &str) -> bool {
    addr.rsplit_once(':')
        .is_some_and(|(host, port)| !host.is_empty() && port.parse::<u16>().is_ok())
}

fn check_addr(addr: &str, key: &str) -> Result<String, ConfigError> {
    let addr = addr.trim();
    if host_port(addr) {
        Ok(addr.to_string())
    } else {
        Err(invalid(key, format!("expected host:port, got {addr:?}")))
    }
}

const REMOVED: &str =
    "was removed; describe each rig as [rig.NAME] with addr and kind (see config.example.toml)";

fn read_profile(name: &str, item: &Item) -> Result<RigProfile, ConfigError> {
    let key = format!("rig.{name}");
    let table = item
        .as_table()
        .ok_or_else(|| invalid(&key, "expected a table"))?;
    let field = |k: &str| table.get(k).map(|i| (i, format!("{key}.{k}")));
    let (addr_item, addr_key) = field("addr").ok_or_else(|| invalid(&key, "missing addr"))?;
    let addr = check_addr(string(addr_item, &addr_key)?, &addr_key)?;
    let (kind_item, kind_key) = field("kind").ok_or_else(|| invalid(&key, "missing kind"))?;
    let kind_text = string(kind_item, &kind_key)?;
    let kind = RigKind::parse(kind_text).ok_or_else(|| {
        invalid(
            &kind_key,
            format!("expected \"kx3\", \"ft847\" or \"generic\", got {kind_text:?}"),
        )
    })?;
    let flag = |k: &str| -> Result<Option<bool>, ConfigError> {
        field(k)
            .map(|(i, key)| {
                i.as_bool()
                    .ok_or_else(|| invalid(&key, "expected true or false"))
            })
            .transpose()
    };
    Ok(RigProfile {
        name: name.to_string(),
        addr,
        kind,
        cw: flag("cw")?,
        split: flag("split")?,
        sat_mode: flag("sat_mode")?,
    })
}

/// The `[rig]` table: profiles, and `default`. The old `addr` key and the `[kx3]` table are
/// errors, not aliases.
fn read_rigs(root: &Table, s: &mut Settings, errs: &mut Vec<ConfigError>) {
    if root.contains_key("kx3") {
        errs.push(invalid("kx3", REMOVED));
    }
    let rig = match sub_table(root, "rig") {
        Ok(Some(rig)) => rig,
        Ok(None) => return,
        Err(e) => {
            errs.push(e);
            return;
        }
    };
    for (name, item) in rig {
        match name {
            "default" => match string(item, "rig.default") {
                Ok(text) => s.rig_default = Some(text.trim().to_string()),
                Err(e) => errs.push(e),
            },
            "addr" => errs.push(invalid("rig.addr", REMOVED)),
            _ => match read_profile(name, item) {
                Ok(profile) => s.rigs.push(profile),
                Err(e) => errs.push(e),
            },
        }
    }
    if let Some(default) = &s.rig_default
        && s.rig(default).is_none()
    {
        errs.push(invalid(
            "rig.default",
            format!("{default:?} is not a [rig.NAME] profile"),
        ));
        s.rig_default = None;
    }
}

/// Read every known key, collecting an error per bad key instead of stopping at the first. A
/// bad key stays unset in the returned [`Settings`].
fn read_settings_lenient(root: &Table) -> (Settings, Vec<ConfigError>) {
    let mut s = Settings::default();
    let mut errs = Vec::new();
    let mut keep = |r: Result<(), ConfigError>| {
        if let Err(e) = r {
            errs.push(e);
        }
    };
    keep((|| {
        if let Some(item) = root.get("callsign") {
            s.callsign =
                Some(Callsign::new(string(item, "callsign")?).map_err(|e| invalid("callsign", e))?);
        }
        Ok(())
    })());
    keep((|| {
        if let Some(item) = sub_table(root, "station")?.and_then(|t| t.get("locator")) {
            s.locator = Some(
                Grid::new(string(item, "station.locator")?)
                    .map_err(|e| invalid("station.locator", e))?,
            );
        }
        Ok(())
    })());
    keep((|| {
        if let Some(item) = sub_table(root, "station")?.and_then(|t| t.get("license_class")) {
            s.license_class = Some(
                LicenseClass::parse(string(item, "station.license_class")?).ok_or_else(|| {
                    invalid(
                        "station.license_class",
                        "expected technician, general, extra, advanced or novice",
                    )
                })?,
            );
        }
        Ok(())
    })());
    keep((|| {
        if let Some(item) = sub_table(root, "logbook")?.and_then(|t| t.get("path")) {
            let path = string(item, "logbook.path")?.trim();
            if path.is_empty() {
                return Err(invalid("logbook.path", "must not be empty"));
            }
            s.log_path = Some(PathBuf::from(path));
        }
        Ok(())
    })());
    keep((|| {
        if let Some(macros) = sub_table(root, "macros")? {
            s.macros = read_soft_keys(macros, "macros")?;
        }
        Ok(())
    })());
    keep((|| {
        if let Some(sets) = sub_table(root, "softkeys")? {
            for (name, item) in sets {
                let key = format!("softkeys.{name}");
                let table = item
                    .as_table()
                    .ok_or_else(|| invalid(&key, "expected a table of F1 to F6"))?;
                s.softkey_sets
                    .insert(name.to_string(), read_soft_keys(table, &key)?);
            }
        }
        Ok(())
    })());
    match sub_table(root, "satellite").and_then(|t| t.map(read_satellite).transpose()) {
        Ok(Some(sat)) => s.satellite = sat,
        Ok(None) => {}
        Err(e) => errs.push(e),
    }
    read_rigs(root, &mut s, &mut errs);
    (s, errs)
}

fn read_satellite(table: &Table) -> Result<SatelliteSettings, ConfigError> {
    let mut sat = SatelliteSettings::default();
    if let Some(item) = table.get("watch") {
        let array = item
            .as_array()
            .ok_or_else(|| invalid("satellite.watch", "expected a list of NORAD ids"))?;
        sat.watch = array
            .iter()
            .map(|v| {
                v.as_integer()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| invalid("satellite.watch", "expected NORAD ids (integers)"))
            })
            .collect::<Result<_, _>>()?;
    }
    if let Some(item) = table.get("horizon_hours") {
        let hours = item
            .as_integer()
            .filter(|n| (1..=168).contains(n))
            .ok_or_else(|| invalid("satellite.horizon_hours", "expected 1 to 168"))?;
        sat.horizon_hours = hours as u32;
    }
    if let Some(item) = table.get("min_elevation_deg") {
        sat.min_elevation_deg = item
            .as_float()
            .or_else(|| item.as_integer().map(|n| n as f64))
            .filter(|d| (0.0..=90.0).contains(d))
            .ok_or_else(|| invalid("satellite.min_elevation_deg", "expected 0 to 90"))?;
    }
    if let Some(item) = table.get("short_pass_secs") {
        sat.short_pass_secs = item
            .as_integer()
            .filter(|n| (1..=3600).contains(n))
            .ok_or_else(|| invalid("satellite.short_pass_secs", "expected 1 to 3600"))?;
    }
    if let Some(item) = table.get("names") {
        let names = item
            .as_table()
            .ok_or_else(|| invalid("satellite.names", "expected a table of names"))?;
        for (shown, written) in names {
            let key = format!("satellite.names.{shown}");
            let written = string(written, &key)?.trim();
            if written.is_empty() {
                return Err(invalid(&key, "has no name"));
            }
            sat.names.insert(shown.to_string(), written.to_string());
        }
    }
    Ok(sat)
}

/// F1..F6 messages from one table, uppercased. `prefix` names the table in errors.
fn read_soft_keys(table: &Table, prefix: &str) -> Result<BTreeMap<u8, String>, ConfigError> {
    let mut keys = BTreeMap::new();
    for (name, item) in table {
        let key = format!("{prefix}.{name}");
        let n = name
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| (1..=6).contains(n) && name == format!("F{n}"))
            .ok_or_else(|| invalid(&key, "only F1 to F6 are supported"))?;
        let text = string(item, &key)?.trim().to_uppercase();
        if text.is_empty() {
            return Err(invalid(&key, "has no text"));
        }
        keys.insert(n, text);
    }
    Ok(keys)
}

fn read_settings(root: &Table) -> Result<Settings, ConfigError> {
    let (settings, errs) = read_settings_lenient(root);
    errs.into_iter().next().map_or(Ok(settings), Err)
}

/// Read `path` keeping whatever is valid: the settings from every key that parsed, plus one
/// error per key that did not. For a tool that consumes only some keys and must not lose them
/// to a mistake in another tool's section.
///
/// A missing file is all defaults. A file that is not valid TOML is salvaged by table: the text
/// is cut at each table header, the pieces that parse alone are read, and the syntax error is
/// reported first, so a broken `[kx3]` does not cost `[station]`. An unreadable file is `Err`.
pub fn load_lenient(path: &Path) -> Result<(Settings, Vec<ConfigError>), ConfigError> {
    let Some((text, _)) = read_text(path)? else {
        return Ok((Settings::default(), Vec::new()));
    };
    let e = match Document::parse(&text) {
        Ok(doc) => return Ok(read_settings_lenient(&doc)),
        Err(e) => e,
    };
    let syntax = parse_error(&text, e);
    let mut good = String::new();
    let mut dropped = Vec::new();
    let mut piece = String::new();
    let mut take = |piece: &mut String| {
        match Document::parse(piece.as_str()) {
            Ok(_) => good.push_str(piece),
            Err(e) => {
                let first = piece.lines().next().unwrap_or("").trim();
                let key = first
                    .strip_prefix('[')
                    .map_or("", |h| h.trim_matches(|c| c == '[' || c == ']' || c == ' '));
                dropped.push(ConfigError::Invalid {
                    key: key.to_string(),
                    message: format!("not valid TOML: {}", e.message()),
                });
            }
        }
        piece.clear();
    };
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with('[') {
            take(&mut piece);
        }
        piece.push_str(line);
    }
    take(&mut piece);
    let (settings, mut errs) = match Document::parse(&good) {
        Ok(doc) => read_settings_lenient(&doc),
        Err(_) => (Settings::default(), Vec::new()),
    };
    // `dropped` names the tables lost (empty key: the top level); the whole-file error stands in
    // when the pieces each parse but the file does not (a repeated table).
    if dropped.is_empty() {
        errs.insert(0, syntax);
    } else {
        errs.splice(0..0, dropped);
    }
    Ok((settings, errs))
}

const KNOWN: [(&str, &[&str]); 6] = [
    (
        "satellite",
        &[
            "watch",
            "horizon_hours",
            "min_elevation_deg",
            "short_pass_secs",
            "names",
        ],
    ),
    ("station", &["locator", "license_class"]),
    ("logbook", &["path"]),
    ("rig", &["default"]),
    ("macros", &[]),
    ("softkeys", &[]),
];

/// Keys a `[rig.NAME]` table may hold.
const PROFILE_KEYS: [&str; 5] = ["addr", "kind", "cw", "split", "sat_mode"];

fn line_at(text: &str, span: Option<std::ops::Range<usize>>) -> usize {
    span.map_or(0, |s| {
        1 + text.as_bytes()[..s.start]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
    })
}

fn parse(text: &str) -> Result<(Settings, Vec<Warning>), ConfigError> {
    let doc = Document::parse(text).map_err(|e| parse_error(text, e))?;
    let settings = read_settings(&doc)?;
    let mut warnings = Vec::new();
    for (name, item) in doc.iter() {
        let line = line_at(text, doc.get_key_value(name).and_then(|(k, _)| k.span()));
        match KNOWN.iter().find(|(t, _)| *t == name) {
            Some((_, keys)) => {
                let Some(table) = item.as_table() else {
                    continue;
                };
                for (key, sub) in table {
                    if name == "macros" || name == "softkeys" || keys.contains(&key) {
                        continue;
                    }
                    if name == "rig"
                        && let Some(profile) = sub.as_table()
                    {
                        for (k, _) in profile {
                            if !PROFILE_KEYS.contains(&k) {
                                warnings.push(Warning {
                                    line: line_at(
                                        text,
                                        profile.get_key_value(k).and_then(|(k, _)| k.span()),
                                    ),
                                    name: format!("rig.{key}.{k}"),
                                });
                            }
                        }
                        continue;
                    }
                    warnings.push(Warning {
                        line: line_at(text, table.get_key_value(key).and_then(|(k, _)| k.span())),
                        name: format!("{name}.{key}"),
                    });
                }
            }
            None if name == "callsign" => {}
            None => warnings.push(Warning {
                line,
                name: name.to_string(),
            }),
        }
    }
    Ok((settings, warnings))
}
