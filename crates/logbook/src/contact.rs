//! The activity-neutral Contact and its validation.

use std::fmt;

use chrono::{DateTime, Utc};

use crate::band::band_for_hz;
use crate::callsign::Callsign;
use crate::locator::{LocatorError, validate_grid};

/// Fields the writer emits itself; an extra may not reuse these names.
pub(crate) const CORE_FIELDS: [&str; 18] = [
    "STATION_CALLSIGN",
    "OPERATOR",
    "CALL",
    "GRIDSQUARE",
    "MY_GRIDSQUARE",
    "QSO_DATE",
    "TIME_ON",
    "QSO_DATE_OFF",
    "TIME_OFF",
    "BAND",
    "FREQ",
    "MODE",
    "SUBMODE",
    "RST_SENT",
    "RST_RCVD",
    "NAME",
    "COMMENT",
    "EOR",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sideband {
    Upper,
    Lower,
}

/// Operating mode. `Other` carries an ADIF `MODE` and optional `SUBMODE` verbatim
/// (uppercased), for modes the core does not name, such as FT8 or FT4 (MFSK / FT4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Fm,
    Ssb(Sideband),
    Cw,
    Am,
    Other {
        mode: String,
        submode: Option<String>,
    },
}

impl Mode {
    /// ADIF `MODE` and `SUBMODE` for this mode.
    pub fn adif(&self) -> (&str, Option<&str>) {
        match self {
            Mode::Fm => ("FM", None),
            Mode::Ssb(Sideband::Upper) => ("SSB", Some("USB")),
            Mode::Ssb(Sideband::Lower) => ("SSB", Some("LSB")),
            Mode::Cw => ("CW", None),
            Mode::Am => ("AM", None),
            Mode::Other { mode, submode } => (mode, submode.as_deref()),
        }
    }

    fn normalized(&self) -> Result<Mode, ContactError> {
        match self {
            Mode::Other { mode, submode } => {
                let mode = text("MODE", mode)?.to_ascii_uppercase();
                let submode = match submode.as_deref().filter(|s| !s.is_empty()) {
                    Some(s) => Some(text("SUBMODE", s)?.to_ascii_uppercase()),
                    None => None,
                };
                if mode.is_empty() {
                    return Err(ContactError::Empty { field: "MODE" });
                }
                Ok(Mode::Other { mode, submode })
            }
            other => Ok(other.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContactError {
    /// A required field was empty.
    Empty {
        field: &'static str,
    },
    /// A text field held a non-ASCII character.
    NonAscii {
        field: String,
        ch: char,
    },
    /// A text field held a control character.
    NonPrintable {
        field: String,
        ch: char,
    },
    /// An extra's name is not uppercase A-Z, 0-9 or `_`.
    BadExtraName(String),
    /// An extra reuses a name the core writes, or one used twice.
    ExtraCollision(String),
    /// The frequency is outside the band table.
    FrequencyOutsideBands(u64),
    /// An uplink or downlink band has no AMSAT mode letter.
    NoSatModeLetter {
        band: &'static str,
    },
    InvalidGrid(LocatorError),
    /// The end time is before the start time.
    EndBeforeStart,
}

impl fmt::Display for ContactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContactError::Empty { field } => write!(f, "{field} is required"),
            ContactError::NonAscii { field, ch } => {
                write!(f, "{field}: non-ASCII character {ch:?} is not allowed")
            }
            ContactError::NonPrintable { field, ch } => {
                write!(f, "{field}: non-printable character {ch:?} is not allowed")
            }
            ContactError::BadExtraName(n) => {
                write!(f, "extra field name {n:?} must be A-Z, 0-9 or '_'")
            }
            ContactError::ExtraCollision(n) => {
                write!(f, "extra field {n} collides with a field already written")
            }
            ContactError::FrequencyOutsideBands(hz) => {
                write!(f, "{hz} Hz is outside the amateur band table")
            }
            ContactError::NoSatModeLetter { band } => {
                write!(f, "band {band} has no satellite mode letter")
            }
            ContactError::InvalidGrid(e) => e.fmt(f),
            ContactError::EndBeforeStart => f.write_str("end time is before start time"),
        }
    }
}

impl std::error::Error for ContactError {}

impl From<LocatorError> for ContactError {
    fn from(e: LocatorError) -> Self {
        ContactError::InvalidGrid(e)
    }
}

/// Reject non-ASCII and control characters; return the text trimmed.
pub(crate) fn text<'a>(field: &str, value: &'a str) -> Result<&'a str, ContactError> {
    for ch in value.chars() {
        if !ch.is_ascii() {
            return Err(ContactError::NonAscii {
                field: field.to_string(),
                ch,
            });
        }
        if ch.is_ascii_control() {
            return Err(ContactError::NonPrintable {
                field: field.to_string(),
                ch,
            });
        }
    }
    Ok(value.trim())
}

fn optional(field: &str, value: Option<&str>) -> Result<Option<String>, ContactError> {
    match value {
        Some(v) => Ok(Some(text(field, v)?)
            .filter(|v| !v.is_empty())
            .map(String::from)),
        None => Ok(None),
    }
}

/// Everything needed to build a [`Contact`]. Empty optional strings count as absent.
#[derive(Clone, Debug)]
pub struct ContactInput {
    pub call: String,
    pub grid: Option<String>,
    pub name: Option<String>,
    pub comment: Option<String>,
    pub rst_sent: Option<String>,
    pub rst_rcvd: Option<String>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    /// Frequency in Hz; the logging station's transmit frequency on a satellite.
    pub freq_hz: u64,
    pub mode: Mode,
    /// Own locator override for this Contact (6 characters, validated).
    pub own_grid: Option<String>,
    /// Station callsign for this Contact when it differs from the operator's.
    pub station_callsign: Option<Callsign>,
    /// Extra ADIF fields, in order.
    pub extras: Vec<(String, String)>,
}

/// A validated Contact, ready to be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contact {
    pub(crate) call: String,
    pub(crate) grid: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) comment: Option<String>,
    pub(crate) rst_sent: Option<String>,
    pub(crate) rst_rcvd: Option<String>,
    pub(crate) start: DateTime<Utc>,
    pub(crate) end: DateTime<Utc>,
    pub(crate) freq_hz: u64,
    pub(crate) band: &'static str,
    pub(crate) mode: Mode,
    pub(crate) own_grid: Option<String>,
    pub(crate) station_callsign: Option<Callsign>,
    pub(crate) extras: Vec<(String, String)>,
}

impl Contact {
    pub fn new(input: ContactInput) -> Result<Contact, ContactError> {
        let call = text("CALL", &input.call)?.to_ascii_uppercase();
        if call.is_empty() {
            return Err(ContactError::Empty { field: "CALL" });
        }
        let grid = optional("GRIDSQUARE", input.grid.as_deref())?.map(|g| g.to_ascii_uppercase());
        let name = optional("NAME", input.name.as_deref())?;
        let comment = optional("COMMENT", input.comment.as_deref())?;
        let rst_sent = optional("RST_SENT", input.rst_sent.as_deref())?;
        let rst_rcvd = optional("RST_RCVD", input.rst_rcvd.as_deref())?;
        let own_grid = match input.own_grid.as_deref() {
            Some(g) => Some(validate_grid(g)?),
            None => None,
        };
        if input.end < input.start {
            return Err(ContactError::EndBeforeStart);
        }
        let band = band_for_hz(input.freq_hz)
            .ok_or(ContactError::FrequencyOutsideBands(input.freq_hz))?
            .name;
        let mode = input.mode.normalized()?;

        let mut extras: Vec<(String, String)> = Vec::new();
        for (key, value) in input.extras {
            let ok_name = !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
            if !ok_name || key.ends_with("_INTL") {
                return Err(ContactError::BadExtraName(key));
            }
            if CORE_FIELDS.contains(&key.as_str()) || extras.iter().any(|(k, _)| *k == key) {
                return Err(ContactError::ExtraCollision(key));
            }
            let value = text(&key, &value)?.to_string();
            extras.push((key, value));
        }

        Ok(Contact {
            call,
            grid,
            name,
            comment,
            rst_sent,
            rst_rcvd,
            start: input.start,
            end: input.end,
            freq_hz: input.freq_hz,
            band,
            mode,
            own_grid,
            station_callsign: input.station_callsign,
            extras,
        })
    }

    pub fn call(&self) -> &str {
        &self.call
    }

    pub fn grid(&self) -> Option<&str> {
        self.grid.as_deref()
    }

    pub fn rst_sent(&self) -> Option<&str> {
        self.rst_sent.as_deref()
    }

    pub fn rst_rcvd(&self) -> Option<&str> {
        self.rst_rcvd.as_deref()
    }

    pub fn start(&self) -> DateTime<Utc> {
        self.start
    }

    pub fn end(&self) -> DateTime<Utc> {
        self.end
    }

    pub fn freq_hz(&self) -> u64 {
        self.freq_hz
    }

    pub fn band(&self) -> &'static str {
        self.band
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    pub(crate) fn input() -> ContactInput {
        ContactInput {
            call: "k9xyz".into(),
            grid: None,
            name: None,
            comment: None,
            rst_sent: None,
            rst_rcvd: None,
            start: Utc.with_ymd_and_hms(2026, 11, 4, 12, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2026, 11, 4, 12, 1, 0).unwrap(),
            freq_hz: 145_900_000,
            mode: Mode::Fm,
            own_grid: None,
            station_callsign: None,
            extras: vec![],
        }
    }

    #[test]
    fn mode_mapping() {
        assert_eq!(Mode::Fm.adif(), ("FM", None));
        assert_eq!(Mode::Ssb(Sideband::Upper).adif(), ("SSB", Some("USB")));
        assert_eq!(Mode::Ssb(Sideband::Lower).adif(), ("SSB", Some("LSB")));
        assert_eq!(Mode::Cw.adif(), ("CW", None));
        assert_eq!(Mode::Am.adif(), ("AM", None));
        let other = Mode::Other {
            mode: "mfsk".into(),
            submode: Some("ft4".into()),
        };
        let mut i = input();
        i.mode = other;
        let c = Contact::new(i).unwrap();
        assert_eq!(c.mode().adif(), ("MFSK", Some("FT4")));
    }

    #[test]
    fn call_is_uppercased_and_required() {
        assert_eq!(Contact::new(input()).unwrap().call(), "K9XYZ");
        let mut i = input();
        i.call = "  ".into();
        assert_eq!(Contact::new(i), Err(ContactError::Empty { field: "CALL" }));
    }

    #[test]
    fn non_ascii_names_field_and_character() {
        let mut i = input();
        i.name = Some("José".into());
        let e = Contact::new(i).unwrap_err();
        assert_eq!(
            e,
            ContactError::NonAscii {
                field: "NAME".into(),
                ch: 'é'
            }
        );
        let msg = e.to_string();
        assert!(msg.contains("NAME") && msg.contains('é'), "{msg}");
    }

    #[test]
    fn extras_are_validated() {
        let mut i = input();
        i.extras = vec![("sota_ref".into(), "W9/SE-001".into())];
        assert!(matches!(
            Contact::new(i),
            Err(ContactError::BadExtraName(_))
        ));
        let mut i = input();
        i.extras = vec![("FREQ".into(), "1".into())];
        assert_eq!(
            Contact::new(i),
            Err(ContactError::ExtraCollision("FREQ".into()))
        );
        let mut i = input();
        i.extras = vec![("X_INTL".into(), "1".into())];
        assert!(matches!(
            Contact::new(i),
            Err(ContactError::BadExtraName(_))
        ));
        let mut i = input();
        i.extras = vec![("X".into(), "a\nb".into())];
        assert!(matches!(
            Contact::new(i),
            Err(ContactError::NonPrintable { .. })
        ));
        let mut i = input();
        i.extras = vec![("X".into(), "1".into()), ("X".into(), "2".into())];
        assert!(matches!(
            Contact::new(i),
            Err(ContactError::ExtraCollision(_))
        ));
    }

    #[test]
    fn frequency_must_be_in_a_band() {
        let mut i = input();
        i.freq_hz = 100_000_000;
        assert_eq!(
            Contact::new(i),
            Err(ContactError::FrequencyOutsideBands(100_000_000))
        );
    }

    #[test]
    fn own_grid_and_time_order_are_validated() {
        let mut i = input();
        i.own_grid = Some("EN71".into());
        assert!(matches!(Contact::new(i), Err(ContactError::InvalidGrid(_))));
        let mut i = input();
        std::mem::swap(&mut i.start, &mut i.end);
        assert_eq!(Contact::new(i), Err(ContactError::EndBeforeStart));
    }
}
