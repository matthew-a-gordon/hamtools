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
//! [logbook]
//! path = "/home/me/log.adi"
//! [rig]                   # `[kx3]` is read as an alias
//! addr = "127.0.0.1:4532"
//! [macros]                # F1 to F6
//! F1 = "CQ DE {CALL}"
//! ```
//!
//! Unknown keys are [`Warning`]s, never errors and never dropped; a wrong value for a known key
//! is a [`ConfigError::Invalid`] naming the key.

use std::collections::BTreeMap;
use std::fmt;
use std::hash::{DefaultHasher, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

pub use logbook::{Callsign, Grid};
use toml_edit::{Document, DocumentMut, Item, Table, Value};

/// The typed view of the file. `None` and an absent macro mean "not set".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub callsign: Option<Callsign>,
    /// The operator's 6-character Locator.
    pub locator: Option<Grid>,
    pub log_path: Option<PathBuf>,
    /// `host:port` of the rigctld to talk to.
    pub rig_addr: Option<String>,
    /// Soft key text for F1..=F6, uppercase, may contain `{CALL}`.
    pub macros: BTreeMap<u8, String>,
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
        let edited: Vec<Field> = Field::ALL
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Callsign,
    Locator,
    LogPath,
    RigAddr,
    Macro(u8),
}

impl Field {
    const ALL: [Field; 10] = [
        Field::Callsign,
        Field::Locator,
        Field::LogPath,
        Field::RigAddr,
        Field::Macro(1),
        Field::Macro(2),
        Field::Macro(3),
        Field::Macro(4),
        Field::Macro(5),
        Field::Macro(6),
    ];

    fn key(self) -> String {
        match self {
            Field::Callsign => "callsign".into(),
            Field::Locator => "station.locator".into(),
            Field::LogPath => "logbook.path".into(),
            Field::RigAddr => "rig.addr".into(),
            Field::Macro(n) => format!("macros.F{n}"),
        }
    }

    /// The field as text, for comparing and for reporting a conflict.
    fn get(self, s: &Settings) -> Option<String> {
        match self {
            Field::Callsign => s.callsign.as_ref().map(|c| c.as_str().to_string()),
            Field::Locator => s.locator.as_ref().map(|g| g.as_str().to_string()),
            Field::LogPath => s
                .log_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            Field::RigAddr => s.rig_addr.clone(),
            Field::Macro(n) => s.macros.get(&n).cloned(),
        }
    }

    fn set(self, doc: &mut DocumentMut, new: &Settings) {
        let value = self.get(new);
        match self {
            Field::Callsign => put(doc.as_table_mut(), "callsign", value),
            Field::Locator => put(table(doc, "station"), "locator", value),
            Field::LogPath => put(table(doc, "logbook"), "path", value),
            Field::RigAddr => {
                // Patch where the address lives: `[rig]` wins, then the `[kx3]` alias.
                let has = |doc: &DocumentMut, t: &str| {
                    doc.get(t)
                        .and_then(Item::as_table)
                        .is_some_and(|t| t.contains_key("addr"))
                };
                if value.is_none() {
                    for t in ["rig", "kx3"] {
                        if has(doc, t) {
                            put(table(doc, t), "addr", None);
                        }
                    }
                } else if !has(doc, "rig") && has(doc, "kx3") {
                    put(table(doc, "kx3"), "addr", value);
                } else {
                    put(table(doc, "rig"), "addr", value);
                }
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

fn read_addr(root: &Table, table_name: &str) -> Result<Option<String>, ConfigError> {
    let Some(item) = sub_table(root, table_name)?.and_then(|t| t.get("addr")) else {
        return Ok(None);
    };
    let key = format!("{table_name}.addr");
    let addr = string(item, &key)?.trim();
    if host_port(addr) {
        Ok(Some(addr.to_string()))
    } else {
        Err(invalid(&key, format!("expected host:port, got {addr:?}")))
    }
}

fn read_settings(root: &Table) -> Result<Settings, ConfigError> {
    let mut s = Settings::default();
    if let Some(item) = root.get("callsign") {
        s.callsign =
            Some(Callsign::new(string(item, "callsign")?).map_err(|e| invalid("callsign", e))?);
    }
    if let Some(item) = sub_table(root, "station")?.and_then(|t| t.get("locator")) {
        s.locator = Some(
            Grid::new(string(item, "station.locator")?)
                .map_err(|e| invalid("station.locator", e))?,
        );
    }
    if let Some(item) = sub_table(root, "logbook")?.and_then(|t| t.get("path")) {
        let path = string(item, "logbook.path")?.trim();
        if path.is_empty() {
            return Err(invalid("logbook.path", "must not be empty"));
        }
        s.log_path = Some(PathBuf::from(path));
    }
    s.rig_addr = match read_addr(root, "rig")? {
        Some(addr) => Some(addr),
        None => read_addr(root, "kx3")?,
    };
    if let Some(macros) = sub_table(root, "macros")? {
        for (name, item) in macros {
            let key = format!("macros.{name}");
            let n = name
                .strip_prefix('F')
                .and_then(|n| n.parse::<u8>().ok())
                .filter(|n| (1..=6).contains(n) && name == format!("F{n}"))
                .ok_or_else(|| invalid(&key, "only F1 to F6 are supported"))?;
            let text = string(item, &key)?.trim().to_uppercase();
            if text.is_empty() {
                return Err(invalid(&key, "has no text"));
            }
            s.macros.insert(n, text);
        }
    }
    Ok(s)
}

const KNOWN: [(&str, &[&str]); 5] = [
    ("station", &["locator"]),
    ("logbook", &["path"]),
    ("rig", &["addr"]),
    ("kx3", &["addr"]),
    ("macros", &[]),
];

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
                for (key, _) in table {
                    if name == "macros" || keys.contains(&key) {
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
