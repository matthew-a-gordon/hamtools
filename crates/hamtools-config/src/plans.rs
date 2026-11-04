//! `plans.toml`: Activation plans, read and written with the same care as the config file.
//!
//! Each plan is a `[plan.<id>]` table. The file is hand-edited as well as written by the app, so a
//! save patches only the fields that changed: comments, key order and keys this crate does not
//! know stay as written, and a change made on disk to a field the app did not touch is kept. When
//! both sides changed the same field differently nothing is written and the clash is reported.
//!
//! ```toml
//! [plan.pota-us-4571-20261105]
//! activity = "pota"             # or "sota"
//! references = ["US-4571"]      # an n-fer lists several, the first is primary; SOTA has one
//! date = "2026-11-05"           # UTC
//! bands = ["40M"]
//! modes = ["CW"]
//! softkey_set = "pota"          # a [softkeys.<name>] set in config.toml
//! notes = "bring the long wire"
//! status = "planned"            # or "done"; "active" is session-only and never stored
//! [plan.pota-us-4571-20261105.softkeys]   # single-key overrides
//! F1 = "CQ POTA DE {MYCALL}"
//! [plan.pota-us-4571-20261105.snapshot]   # the reference as last looked up, kept with the plan
//! reference = "US-4571"
//! name = "Potato Creek State Park"
//! ```
//!
//! The lifecycle is planned, active, done. Active lives only in the running app, so the file
//! holds `planned` or `done`, and a hand-written `active` reads back as planned.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml_edit::{Document, DocumentMut, Item, Table, Value};

use crate::{
    ConfigError, SaveError, Saved, Stamp, Warning, hash, invalid, line_at, parse_error,
    read_soft_keys, read_text, string, write_atomic,
};

/// Which program the plan is for. The Hunter Role has no plans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanActivity {
    Sota,
    Pota,
}

impl PlanActivity {
    pub fn as_str(self) -> &'static str {
        match self {
            PlanActivity::Sota => "sota",
            PlanActivity::Pota => "pota",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sota" => Some(PlanActivity::Sota),
            "pota" => Some(PlanActivity::Pota),
            _ => None,
        }
    }
}

/// A reference as it was when the plan was saved, kept so the plan works with an empty cache.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RefSnapshot {
    pub reference: String,
    pub name: String,
    pub location: Option<String>,
    pub grid: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub alt_m: Option<u32>,
    pub points: Option<u32>,
}

/// One saved intention to activate.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// The table name, `[plan.<id>]`. Stable across edits.
    pub id: String,
    pub activity: PlanActivity,
    /// The first is primary. SOTA holds exactly one.
    pub references: Vec<String>,
    /// UTC day, `YYYY-MM-DD`.
    pub date: String,
    pub bands: Vec<String>,
    pub modes: Vec<String>,
    /// A named `[softkeys.<name>]` set in `config.toml`.
    pub softkey_set: Option<String>,
    /// Single-key overrides, uppercase.
    pub softkeys: BTreeMap<u8, String>,
    pub notes: Option<String>,
    /// Set by hand. Planned is the default.
    pub done: bool,
    /// The primary reference as last resolved; `None` is unresolved.
    pub snapshot: Option<RefSnapshot>,
}

impl Plan {
    pub fn primary(&self) -> Option<&str> {
        self.references.first().map(String::as_str)
    }
}

/// Whether `s` is a real `YYYY-MM-DD` calendar date.
pub fn valid_date(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    let [y, m, d] = parts[..] else { return false };
    let num = |t: &str, len: usize| {
        (t.len() == len && t.bytes().all(|b| b.is_ascii_digit()))
            .then(|| t.parse::<u32>().ok())
            .flatten()
    };
    let (Some(y), Some(m), Some(d)) = (num(y, 4), num(m, 2), num(d, 2)) else {
        return false;
    };
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d)
}

/// `plan.<id>.<field>` style names for one stored field, to value text.
type Fields = BTreeMap<(String, String), String>;

const KNOWN: [&str; 8] = [
    "activity",
    "references",
    "date",
    "bands",
    "modes",
    "softkey_set",
    "notes",
    "status",
];

fn strings(v: &[String]) -> String {
    let mut a = toml_edit::Array::new();
    for s in v {
        a.push(s.as_str());
    }
    Value::from(a).to_string()
}

fn text(s: &str) -> String {
    Value::from(s).to_string()
}

/// Every stored field of a plan as `(plan id, dotted field)` to the TOML text of its value.
fn fields(plans: &[Plan]) -> Fields {
    let mut out = Fields::new();
    for p in plans {
        let mut put = |k: &str, v: String| {
            out.insert((p.id.clone(), k.to_string()), v);
        };
        put("activity", text(p.activity.as_str()));
        put("references", strings(&p.references));
        put("date", text(&p.date));
        put("bands", strings(&p.bands));
        put("modes", strings(&p.modes));
        if let Some(s) = &p.softkey_set {
            put("softkey_set", text(s));
        }
        if let Some(s) = &p.notes {
            put("notes", text(s));
        }
        put("status", text(if p.done { "done" } else { "planned" }));
        for (n, msg) in &p.softkeys {
            put(&format!("softkeys.F{n}"), text(msg));
        }
        if let Some(s) = &p.snapshot {
            put("snapshot.reference", text(&s.reference));
            put("snapshot.name", text(&s.name));
            if let Some(v) = &s.location {
                put("snapshot.location", text(v));
            }
            if let Some(v) = &s.grid {
                put("snapshot.grid", text(v));
            }
            if let Some(v) = s.lat {
                put("snapshot.lat", Value::from(v).to_string());
            }
            if let Some(v) = s.lon {
                put("snapshot.lon", Value::from(v).to_string());
            }
            if let Some(v) = s.alt_m {
                put("snapshot.alt_m", Value::from(i64::from(v)).to_string());
            }
            if let Some(v) = s.points {
                put("snapshot.points", Value::from(i64::from(v)).to_string());
            }
        }
    }
    out
}

fn string_list(item: &Item, key: &str) -> Result<Vec<String>, ConfigError> {
    let arr = item
        .as_array()
        .ok_or_else(|| invalid(key, "expected a list of strings"))?;
    arr.iter()
        .map(|v| {
            v.as_str()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| invalid(key, "expected a list of non-empty strings"))
        })
        .collect()
}

fn read_snapshot(t: &Table, prefix: &str) -> Result<RefSnapshot, ConfigError> {
    let get = |name: &str| t.get(name);
    let str_of = |name: &str| -> Result<Option<String>, ConfigError> {
        get(name)
            .map(|i| string(i, &format!("{prefix}.{name}")).map(str::to_string))
            .transpose()
    };
    let num_of = |name: &str| -> Result<Option<f64>, ConfigError> {
        get(name)
            .map(|i| {
                i.as_float()
                    .or_else(|| i.as_integer().map(|n| n as f64))
                    .ok_or_else(|| invalid(&format!("{prefix}.{name}"), "expected a number"))
            })
            .transpose()
    };
    let uint_of = |name: &str| -> Result<Option<u32>, ConfigError> {
        get(name)
            .map(|i| {
                i.as_integer()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| invalid(&format!("{prefix}.{name}"), "expected a whole number"))
            })
            .transpose()
    };
    Ok(RefSnapshot {
        reference: str_of("reference")?.unwrap_or_default(),
        name: str_of("name")?.unwrap_or_default(),
        location: str_of("location")?,
        grid: str_of("grid")?,
        lat: num_of("lat")?,
        lon: num_of("lon")?,
        alt_m: uint_of("alt_m")?,
        points: uint_of("points")?,
    })
}

fn read_plan(id: &str, t: &Table) -> Result<Plan, ConfigError> {
    let key = |f: &str| format!("plan.{id}.{f}");
    let required = |f: &str| t.get(f).ok_or_else(|| invalid(&key(f), "is required"));
    let activity = {
        let s = string(required("activity")?, &key("activity"))?;
        PlanActivity::parse(s).ok_or_else(|| {
            invalid(
                &key("activity"),
                format!("expected sota or pota, got {s:?}"),
            )
        })?
    };
    let references: Vec<String> = string_list(required("references")?, &key("references"))?
        .into_iter()
        .map(|r| r.to_ascii_uppercase())
        .collect();
    if references.is_empty() {
        return Err(invalid(&key("references"), "needs at least one reference"));
    }
    if activity == PlanActivity::Sota && references.len() != 1 {
        return Err(invalid(
            &key("references"),
            "a SOTA plan holds exactly one summit",
        ));
    }
    let date = string(required("date")?, &key("date"))?.trim().to_string();
    if !valid_date(&date) {
        return Err(invalid(
            &key("date"),
            format!("expected YYYY-MM-DD, got {date:?}"),
        ));
    }
    let list = |f: &str| -> Result<Vec<String>, ConfigError> {
        match t.get(f) {
            Some(i) => Ok(string_list(i, &key(f))?
                .into_iter()
                .map(|s| s.to_ascii_uppercase())
                .collect()),
            None => Ok(Vec::new()),
        }
    };
    let opt_str = |f: &str| -> Result<Option<String>, ConfigError> {
        t.get(f)
            .map(|i| string(i, &key(f)).map(str::to_string))
            .transpose()
    };
    let done = match opt_str("status")?.as_deref().map(str::trim) {
        None | Some("planned" | "active") => false,
        Some("done") => true,
        Some(other) => {
            return Err(invalid(
                &key("status"),
                format!("expected planned or done, got {other:?}"),
            ));
        }
    };
    let softkeys = match t.get("softkeys") {
        None => BTreeMap::new(),
        Some(item) => {
            let sub = item
                .as_table()
                .ok_or_else(|| invalid(&key("softkeys"), "expected a table of F1 to F6"))?;
            read_soft_keys(sub, &key("softkeys"))?
        }
    };
    let snapshot = match t.get("snapshot") {
        None => None,
        Some(item) => {
            let sub = item
                .as_table()
                .ok_or_else(|| invalid(&key("snapshot"), "expected a table"))?;
            Some(read_snapshot(sub, &key("snapshot"))?)
        }
    };
    Ok(Plan {
        id: id.to_string(),
        activity,
        references,
        date,
        bands: list("bands")?,
        modes: list("modes")?,
        softkey_set: opt_str("softkey_set")?
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        softkeys,
        notes: opt_str("notes")?,
        done,
        snapshot,
    })
}

fn parse(text: &str) -> Result<(Vec<Plan>, Vec<Warning>), ConfigError> {
    let doc = Document::parse(text).map_err(|e| parse_error(text, e))?;
    let mut plans = Vec::new();
    let mut warnings = Vec::new();
    for (name, item) in doc.iter() {
        if name != "plan" {
            warnings.push(Warning {
                line: line_at(text, doc.get_key_value(name).and_then(|(k, _)| k.span())),
                name: name.to_string(),
            });
            continue;
        }
        let table = item
            .as_table()
            .ok_or_else(|| invalid("plan", "expected tables named [plan.<id>]"))?;
        for (id, item) in table {
            let t = item
                .as_table()
                .ok_or_else(|| invalid(&format!("plan.{id}"), "expected a table"))?;
            plans.push(read_plan(id, t)?);
            for (k, _) in t {
                if !KNOWN.contains(&k) && k != "softkeys" && k != "snapshot" {
                    warnings.push(Warning {
                        line: line_at(text, t.get_key_value(k).and_then(|(k, _)| k.span())),
                        name: format!("plan.{id}.{k}"),
                    });
                }
            }
        }
    }
    Ok((plans, warnings))
}

/// One field edited here and differently on disk, reported by plan and field.
fn conflict_key(id: &str, field: &str) -> String {
    format!("plan.{id}.{field}")
}

/// The loaded `plans.toml`: the text, the typed plans and what is needed to detect disk changes.
#[derive(Debug)]
pub struct PlansFile {
    path: PathBuf,
    doc: DocumentMut,
    plans: Vec<Plan>,
    warnings: Vec<Warning>,
    stamp: Option<Stamp>,
}

impl PlansFile {
    /// Read and validate `path`. A missing file is no plans (created on first save).
    pub fn load(path: &Path) -> Result<PlansFile, ConfigError> {
        let (text, stamp) = match read_text(path)? {
            Some((text, stamp)) => (text, Some(stamp)),
            None => (String::new(), None),
        };
        let (plans, warnings) = parse(&text)?;
        let doc = text
            .parse::<DocumentMut>()
            .map_err(|e| parse_error(&text, e))?;
        Ok(PlansFile {
            path: path.to_path_buf(),
            doc,
            plans,
            warnings,
            stamp,
        })
    }

    pub fn reload(&mut self) -> Result<(), ConfigError> {
        *self = PlansFile::load(&self.path)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn plans(&self) -> &[Plan] {
        &self.plans
    }

    /// Keys in the file this crate does not know. They are kept and only reported.
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// Whether a save could replace the file (a symlink or non-writable target never can).
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

    /// Whether the file differs from what was loaded.
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

    /// Write `new` as the full set of plans, patching only the fields that differ from what was
    /// loaded (a plan missing from `new` is deleted). If the file changed on disk the edits are
    /// rebased onto the fresh text when those fields are untouched there, and returned as a
    /// [`SaveError::Conflict`] when they are not; nothing is written then.
    pub fn save(&mut self, new: &[Plan]) -> Result<Saved, SaveError> {
        self.check_writable()?;
        let before = fields(&self.plans);
        let after = fields(new);
        let edited: Vec<&(String, String)> = before
            .keys()
            .chain(after.keys())
            .filter(|k| before.get(*k) != after.get(*k))
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
            let (disk_plans, _) = parse(&text).map_err(unparseable)?;
            let disk = fields(&disk_plans);
            let mut conflicts: Vec<crate::FieldConflict> = edited
                .iter()
                .filter(|k| disk.get(**k) != before.get(**k) && disk.get(**k) != after.get(**k))
                .map(|(id, f)| crate::FieldConflict {
                    key: conflict_key(id, f),
                    file: disk.get(&(id.clone(), f.clone())).cloned(),
                    mine: after.get(&(id.clone(), f.clone())).cloned(),
                })
                .collect();
            conflicts.dedup_by(|a, b| a.key == b.key);
            if !conflicts.is_empty() {
                return Err(SaveError::Conflict(conflicts));
            }
            doc = text
                .parse::<DocumentMut>()
                .map_err(|e| unparseable(parse_error(&text, e)))?;
            rebased = true;
        }

        let kept: Vec<&str> = new.iter().map(|p| p.id.as_str()).collect();
        for p in &self.plans {
            if !kept.contains(&p.id.as_str()) {
                remove_plan(&mut doc, &p.id);
            }
        }
        for (id, field) in edited {
            match after.get(&(id.clone(), field.clone())) {
                Some(repr) => set_field(&mut doc, id, field, repr),
                None if kept.contains(&id.as_str()) => remove_field(&mut doc, id, field),
                None => {}
            }
        }
        let out = doc.to_string();
        write_atomic(&self.path, &out)?;

        let (plans, warnings) = parse(&out).map_err(|e| SaveError::Io(e.to_string()))?;
        self.stamp = read_text(&self.path).ok().flatten().map(|(_, s)| s);
        self.doc = doc;
        self.plans = plans;
        self.warnings = warnings;
        Ok(if rebased {
            Saved::Rebased
        } else {
            Saved::Written
        })
    }
}

fn plan_table<'a>(doc: &'a mut DocumentMut, id: &str) -> &'a mut Table {
    if !doc.contains_key("plan") {
        let mut t = Table::new();
        t.set_implicit(true);
        doc.insert("plan", Item::Table(t));
    }
    let root = doc["plan"].as_table_mut().expect("validated as a table");
    if !root.contains_key(id) {
        root.insert(id, Item::Table(Table::new()));
    }
    root[id].as_table_mut().expect("validated as a table")
}

fn remove_plan(doc: &mut DocumentMut, id: &str) {
    if let Some(root) = doc.get_mut("plan").and_then(Item::as_table_mut) {
        root.remove(id);
    }
}

fn set_field(doc: &mut DocumentMut, id: &str, field: &str, repr: &str) {
    let value: Value = repr.parse().expect("generated TOML value");
    let plan = plan_table(doc, id);
    let table = match field.split_once('.') {
        Some((sub, _)) => {
            if !plan.contains_key(sub) {
                plan.insert(sub, Item::Table(Table::new()));
            }
            plan[sub].as_table_mut().expect("validated as a table")
        }
        None => plan,
    };
    let leaf = field.split_once('.').map_or(field, |(_, l)| l);
    match table.get_mut(leaf).and_then(Item::as_value_mut) {
        Some(old) => {
            let decor = old.decor().clone();
            let mut new = value;
            *new.decor_mut() = decor;
            *old = new;
        }
        None => {
            table.insert(leaf, Item::Value(value));
        }
    }
}

fn remove_field(doc: &mut DocumentMut, id: &str, field: &str) {
    let Some(plan) = doc
        .get_mut("plan")
        .and_then(Item::as_table_mut)
        .and_then(|r| r.get_mut(id))
        .and_then(Item::as_table_mut)
    else {
        return;
    };
    match field.split_once('.') {
        Some((sub, leaf)) => {
            if let Some(t) = plan.get_mut(sub).and_then(Item::as_table_mut) {
                t.remove(leaf);
                if t.is_empty() {
                    plan.remove(sub);
                }
            }
        }
        None => {
            plan.remove(field);
        }
    }
}
