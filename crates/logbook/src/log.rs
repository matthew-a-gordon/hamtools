//! The Log: one append-only ADI file, written safely.

// File::lock is stable since Rust 1.89, above clippy.toml's msrv; the toolchain is newer.
#![allow(clippy::incompatible_msrv)]

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::adif;
use crate::callsign::Callsign;
use crate::contact::Contact;

/// How much of the file's end is searched for the last marker.
const TAIL_WINDOW: u64 = 64 * 1024;

#[derive(Debug)]
pub enum AppendError {
    Io(io::Error),
    /// The file ends in a record that never reached its end-of-record marker.
    PartialRecord {
        path: PathBuf,
        offset: u64,
    },
    /// A non-empty file that does not look like ADI.
    NotAdif {
        path: PathBuf,
    },
    /// More than the tail window follows the last marker, or there is none in it.
    NoMarkerInTail {
        path: PathBuf,
    },
}

impl fmt::Display for AppendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppendError::Io(e) => write!(f, "log I/O error: {e}"),
            AppendError::PartialRecord { path, offset } => write!(
                f,
                "{} ends in a partial record starting at byte {offset}; delete everything after the last <EOR> marker",
                path.display()
            ),
            AppendError::NotAdif { path } => {
                write!(
                    f,
                    "{} is not an ADI file; refusing to append",
                    path.display()
                )
            }
            AppendError::NoMarkerInTail { path } => write!(
                f,
                "{} has no <EOR> or <EOH> marker in its last {} bytes; refusing to append",
                path.display(),
                TAIL_WINDOW
            ),
        }
    }
}

impl std::error::Error for AppendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppendError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for AppendError {
    fn from(e: io::Error) -> Self {
        AppendError::Io(e)
    }
}

/// State of an existing file's tail.
enum Tail {
    Empty,
    Clean { ends_with_newline: bool },
}

/// A Log file and the operator whose Contacts go in it. Holds no handle and no cache.
#[derive(Clone, Debug)]
pub struct Log {
    path: PathBuf,
    operator: Callsign,
}

impl Log {
    /// No I/O. `STATION_CALLSIGN` defaults to `operator`; `OPERATOR` is always `operator`.
    pub fn new(path: impl Into<PathBuf>, operator: Callsign) -> Self {
        Log {
            path: path.into(),
            operator,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Run the header and tail checks `append` runs, writing nothing. A missing file is fine.
    pub fn check(&self) -> Result<(), AppendError> {
        let mut file = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        file.lock_shared()?;
        inspect(&mut file, &self.path).map(|_| ())
    }

    /// Append one record, creating the file (with a header) and its directory if needed.
    /// Returns `Ok` only after the data is synced to disk. Existing bytes are never changed.
    pub fn append(&self, contact: &Contact) -> Result<(), AppendError> {
        if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            fs::create_dir_all(dir)?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&self.path)?;
        file.lock()?;
        let mut out = String::new();
        match inspect(&mut file, &self.path)? {
            Tail::Empty => out.push_str(&adif::header()),
            Tail::Clean {
                ends_with_newline: false,
            } => out.push('\n'),
            Tail::Clean { .. } => {}
        }
        out.push_str(&adif::record(contact, &self.operator));
        file.write_all(out.as_bytes())?;
        file.sync_data()?;
        Ok(())
    }
}

fn find_last_marker(buf: &[u8]) -> Option<usize> {
    // Index just past the last <EOR> or <EOH> (ASCII case-insensitive).
    (0..buf.len().saturating_sub(4)).rev().find_map(|i| {
        let w = &buf[i..i + 5];
        (w.eq_ignore_ascii_case(b"<EOR>") || w.eq_ignore_ascii_case(b"<EOH>")).then_some(i + 5)
    })
}

fn inspect(file: &mut File, path: &Path) -> Result<Tail, AppendError> {
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(Tail::Empty);
    }
    let start = len.saturating_sub(TAIL_WINDOW);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::with_capacity((len - start) as usize);
    file.read_to_end(&mut buf)?;
    match find_last_marker(&buf) {
        Some(end) => {
            let rest = &buf[end..];
            if rest.iter().all(u8::is_ascii_whitespace) {
                Ok(Tail::Clean {
                    ends_with_newline: buf.last() == Some(&b'\n'),
                })
            } else {
                Err(AppendError::PartialRecord {
                    path: path.to_path_buf(),
                    offset: start + (end + rest.len() - rest.trim_ascii_start().len()) as u64,
                })
            }
        }
        None if start > 0 => Err(AppendError::NoMarkerInTail {
            path: path.to_path_buf(),
        }),
        None if buf[0] == b'<' => Err(AppendError::PartialRecord {
            path: path.to_path_buf(),
            offset: 0,
        }),
        None => Err(AppendError::NotAdif {
            path: path.to_path_buf(),
        }),
    }
}
