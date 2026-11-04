//! Reading the Log back: records as ordered fields, with the extras kept.

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};

use crate::contact::CORE_FIELDS;

/// One record of the Log, as written. Field names are uppercased on read and compared
/// case-insensitively.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    fields: Vec<(String, String)>,
}

impl Record {
    pub fn fields(&self) -> &[(String, String)] {
        &self.fields
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn call(&self) -> Option<&str> {
        self.get("CALL")
    }

    pub fn band(&self) -> Option<&str> {
        self.get("BAND")
    }

    pub fn mode(&self) -> Option<&str> {
        self.get("MODE")
    }

    /// `QSO_DATE` and `TIME_ON` (`HHMMSS` or `HHMM`) as UTC.
    pub fn start(&self) -> Option<DateTime<Utc>> {
        let date = NaiveDate::parse_from_str(self.get("QSO_DATE")?, "%Y%m%d").ok()?;
        let time = self.get("TIME_ON")?;
        let time = match time.len() {
            4 => NaiveTime::parse_from_str(time, "%H%M").ok()?,
            6 => NaiveTime::parse_from_str(time, "%H%M%S").ok()?,
            _ => return None,
        };
        Some(Utc.from_utc_datetime(&date.and_time(time)))
    }

    /// The fields that are not part of the core Contact, in file order.
    pub fn extras(&self) -> Vec<(&str, &str)> {
        self.fields
            .iter()
            .filter(|(n, _)| !CORE_FIELDS.contains(&n.as_str()))
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .collect()
    }
}

#[derive(Debug)]
pub enum ReadError {
    Io(io::Error),
    /// The text is not well-formed ADI at this byte offset.
    Malformed {
        offset: usize,
        why: &'static str,
    },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Io(e) => write!(f, "log I/O error: {e}"),
            ReadError::Malformed { offset, why } => {
                write!(f, "log is not well-formed ADI at byte {offset}: {why}")
            }
        }
    }
}

impl std::error::Error for ReadError {}

impl From<io::Error> for ReadError {
    fn from(e: io::Error) -> Self {
        ReadError::Io(e)
    }
}

/// Read every record of the file; a missing file has none.
pub(crate) fn read(path: &Path) -> Result<Vec<Record>, ReadError> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    parse(&String::from_utf8_lossy(&bytes))
}

fn parse(text: &str) -> Result<Vec<Record>, ReadError> {
    let mut pos = 0;
    // Free text before the first field is a header only if it ends in <EOH>.
    if !text.starts_with('<') {
        match text.to_ascii_lowercase().find("<eoh>") {
            Some(i) => pos = i + 5,
            None if text.trim().is_empty() => return Ok(Vec::new()),
            None => {
                return Err(ReadError::Malformed {
                    offset: 0,
                    why: "no <EOH> in a file that does not start with a field",
                });
            }
        }
    }
    let mut records = Vec::new();
    let mut fields: Vec<(String, String)> = Vec::new();
    loop {
        let rest = &text[pos..];
        let trimmed = rest.trim_start();
        pos += rest.len() - trimmed.len();
        if trimmed.is_empty() {
            if fields.is_empty() {
                return Ok(records);
            }
            return Err(ReadError::Malformed {
                offset: pos,
                why: "record without <EOR>",
            });
        }
        if !trimmed.starts_with('<') {
            return Err(ReadError::Malformed {
                offset: pos,
                why: "text between fields",
            });
        }
        let close = trimmed.find('>').ok_or(ReadError::Malformed {
            offset: pos,
            why: "unterminated tag",
        })?;
        let tag = &trimmed[1..close];
        pos += close + 1;
        if tag.eq_ignore_ascii_case("EOR") {
            records.push(Record {
                fields: std::mem::take(&mut fields),
            });
            continue;
        }
        let mut parts = tag.split(':');
        let name = parts.next().unwrap_or_default();
        let len: usize = parts
            .next()
            .and_then(|l| l.parse().ok())
            .ok_or(ReadError::Malformed {
                offset: pos,
                why: "field without a length",
            })?;
        let value = text.get(pos..pos + len).ok_or(ReadError::Malformed {
            offset: pos,
            why: "value shorter than its length",
        })?;
        fields.push((name.to_ascii_uppercase(), value.to_string()));
        pos += len;
    }
}
