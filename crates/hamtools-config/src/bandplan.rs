//! The band plan: three embedded layers of reference data about a band, in integer Hz.
//!
//! - **privilege**: FCC segments by license class (47 CFR 97.301), plus the 60 m channel list;
//! - **arrl**: the ARRL voluntary plan, terse facts with a citation and no copied text;
//! - **satellite**: satellite sub-bands.
//!
//! Each layer carries an `as_of` date and every row a `source`. The data is compiled in (no
//! override in v1) and checked when parsed. It is guidance for the operator, never a legal
//! authority: the privilege rows were transcribed from a research note and must be re-read from
//! the live eCFR before being relied on. No power limits are encoded.

use std::sync::OnceLock;

use logbook::{BANDS, Band};
use toml_edit::{DocumentMut, Item, Table, Value};

use crate::LicenseClass;

/// A band plan layer that failed its checks: which layer, and what is wrong with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandPlanError {
    pub layer: String,
    pub message: String,
}

impl std::fmt::Display for BandPlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.layer, self.message)
    }
}

impl std::error::Error for BandPlanError {}

fn bad(layer: &str, message: impl Into<String>) -> BandPlanError {
    BandPlanError {
        layer: layer.to_string(),
        message: message.into(),
    }
}

const PRIVILEGES: &str = include_str!("../data/privileges.toml");
const ARRL: &str = include_str!("../data/arrl.toml");
const SATELLITE: &str = include_str!("../data/satellite.toml");

/// One range of a layer. `classes` is only filled in the privilege layer; `mode` and `label`
/// only in the other two.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// ADIF band name, e.g. `20m`.
    pub band: String,
    pub lo_hz: u64,
    pub hi_hz: u64,
    pub label: String,
    /// The usual rig mode here (`CW`, `USB`, `LSB`, `FM`, `RTTY` or `phone`), if any.
    pub mode: Option<String>,
    pub classes: Vec<LicenseClass>,
    pub source: String,
}

/// One 60 m channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Channel {
    pub band: String,
    pub center_hz: u64,
    /// The upper-sideband dial frequency; what tuning sets.
    pub carrier_hz: u64,
    pub classes: Vec<LicenseClass>,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    /// `YYYY-MM-DD`.
    pub as_of: String,
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandPlan {
    pub privilege: Layer,
    pub arrl: Layer,
    pub satellite: Layer,
    pub channels: Vec<Channel>,
}

impl BandPlan {
    /// The compiled-in data, parsed once. A bad file is a build-time bug caught by the data
    /// tests, so this panics rather than limp on.
    pub fn embedded() -> &'static BandPlan {
        static PLAN: OnceLock<BandPlan> = OnceLock::new();
        PLAN.get_or_init(|| {
            Self::parse(PRIVILEGES, ARRL, SATELLITE).expect("embedded band plan data is valid")
        })
    }

    /// Parse and check the three layer files.
    pub fn parse(privileges: &str, arrl: &str, satellite: &str) -> Result<Self, BandPlanError> {
        let (privilege, channels) = read_layer(privileges, "privileges", true)?;
        let (arrl, _) = read_layer(arrl, "arrl", false)?;
        let (satellite, _) = read_layer(satellite, "satellite", false)?;
        Ok(Self {
            privilege,
            arrl,
            satellite,
            channels,
        })
    }

    /// The oldest `as_of` of the three layers (ISO dates compare as text).
    pub fn as_of(&self) -> &str {
        [&self.privilege, &self.arrl, &self.satellite]
            .iter()
            .map(|l| l.as_of.as_str())
            .min()
            .unwrap_or("")
    }

    /// The bands the plan covers, lowest first (the ADIF table minus any band with no privilege
    /// rows, such as 4 m).
    pub fn bands(&self) -> Vec<Band> {
        BANDS
            .iter()
            .filter(|b| self.privilege.rows.iter().any(|r| r.band == b.name))
            .copied()
            .collect()
    }

    /// Whether `class` may use `hz` per the privilege layer. On 60 m only the contiguous segment
    /// and the channel carriers count.
    pub fn in_privilege(&self, hz: u64, class: LicenseClass) -> bool {
        self.privilege
            .rows
            .iter()
            .any(|r| r.classes.contains(&class) && (r.lo_hz..=r.hi_hz).contains(&hz))
            || self
                .channels
                .iter()
                .any(|c| c.classes.contains(&class) && c.carrier_hz == hz)
    }
}

fn read_layer(
    text: &str,
    name: &str,
    privilege: bool,
) -> Result<(Layer, Vec<Channel>), BandPlanError> {
    let doc: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| bad(name, e.to_string()))?;
    let as_of = doc
        .get("meta")
        .and_then(Item::as_table_like)
        .and_then(|t| t.get("as_of"))
        .and_then(Item::as_str)
        .ok_or_else(|| bad(name, "meta.as_of is missing"))?
        .to_string();
    let b = as_of.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return Err(bad(name, "meta.as_of must be YYYY-MM-DD"));
    }

    let mut rows = Vec::new();
    for table in tables(&doc, "segment", name)? {
        let row = read_row(&table, name, privilege)?;
        check_band(&row.band, row.lo_hz, row.hi_hz, name)?;
        rows.push(row);
    }
    for pair in rows.windows(2) {
        if pair[0].hi_hz > pair[1].lo_hz {
            return Err(bad(
                name,
                format!(
                    "rows overlap or are not sorted: {}-{} then {}-{}",
                    pair[0].lo_hz, pair[0].hi_hz, pair[1].lo_hz, pair[1].hi_hz
                ),
            ));
        }
    }

    let mut channels = Vec::new();
    if privilege {
        for table in tables(&doc, "channel", name)? {
            let band = text_of(&table, "band", name)?;
            let carrier_hz = int_of(&table, "carrier_hz", name)?;
            check_band(&band, carrier_hz, carrier_hz, name)?;
            channels.push(Channel {
                band,
                center_hz: int_of(&table, "center_hz", name)?,
                carrier_hz,
                classes: classes_of(&table, name)?,
                source: source_of(&table, name)?,
            });
        }
        if channels
            .windows(2)
            .any(|p| p[0].carrier_hz >= p[1].carrier_hz)
        {
            return Err(bad(name, "channels are not sorted"));
        }
    }
    Ok((Layer { as_of, rows }, channels))
}

/// The inline tables of an array-of-inline-tables key.
fn tables(doc: &DocumentMut, key: &str, name: &str) -> Result<Vec<Table>, BandPlanError> {
    let Some(item) = doc.get(key) else {
        return Ok(Vec::new());
    };
    let array = item
        .as_array()
        .ok_or_else(|| bad(name, format!("{key} must be an array of tables")))?;
    array
        .iter()
        .map(|v| match v {
            Value::InlineTable(t) => Ok(t.clone().into_table()),
            _ => Err(bad(name, format!("{key} must hold tables"))),
        })
        .collect()
}

fn read_row(t: &Table, name: &str, privilege: bool) -> Result<Row, BandPlanError> {
    let (lo_hz, hi_hz) = (int_of(t, "lo_hz", name)?, int_of(t, "hi_hz", name)?);
    if lo_hz > hi_hz {
        return Err(bad(name, format!("lo_hz {lo_hz} is above hi_hz {hi_hz}")));
    }
    Ok(Row {
        band: text_of(t, "band", name)?,
        lo_hz,
        hi_hz,
        label: t
            .get("label")
            .and_then(Item::as_str)
            .unwrap_or_default()
            .to_string(),
        mode: t.get("mode").and_then(Item::as_str).map(str::to_string),
        classes: if privilege {
            classes_of(t, name)?
        } else {
            Vec::new()
        },
        source: source_of(t, name)?,
    })
}

fn text_of(t: &Table, key: &str, name: &str) -> Result<String, BandPlanError> {
    t.get(key)
        .and_then(Item::as_str)
        .map(str::to_string)
        .ok_or_else(|| bad(name, format!("a row is missing {key}")))
}

fn int_of(t: &Table, key: &str, name: &str) -> Result<u64, BandPlanError> {
    t.get(key)
        .and_then(Item::as_integer)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| bad(name, format!("a row is missing a whole-number {key}")))
}

fn source_of(t: &Table, name: &str) -> Result<String, BandPlanError> {
    let source = text_of(t, "source", name)?;
    if source.trim().is_empty() {
        return Err(bad(name, "a row has an empty source"));
    }
    Ok(source)
}

/// `x` extra, `a` advanced, `g` general, `t` technician, `n` novice.
fn classes_of(t: &Table, name: &str) -> Result<Vec<LicenseClass>, BandPlanError> {
    text_of(t, "classes", name)?
        .chars()
        .map(|c| match c {
            'x' => Ok(LicenseClass::Extra),
            'a' => Ok(LicenseClass::Advanced),
            'g' => Ok(LicenseClass::General),
            't' => Ok(LicenseClass::Technician),
            'n' => Ok(LicenseClass::Novice),
            other => Err(bad(name, format!("unknown class letter {other:?}"))),
        })
        .collect()
}

fn check_band(band: &str, lo_hz: u64, hi_hz: u64, name: &str) -> Result<(), BandPlanError> {
    let known = BANDS
        .iter()
        .find(|b| b.name == band)
        .ok_or_else(|| bad(name, format!("unknown band {band}")))?;
    if lo_hz < known.lower_hz || hi_hz > known.upper_hz {
        return Err(bad(
            name,
            format!(
                "{lo_hz}-{hi_hz} is outside band {band} ({}-{})",
                known.lower_hz, known.upper_hz
            ),
        ));
    }
    Ok(())
}
