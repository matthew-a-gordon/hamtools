//! Logging a Contact: the modal form, the session list and the mapping onto `logbook`.
//! No I/O beyond the [`Log`] the [`App`](crate::app::App) is given.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use logbook::{Contact, ContactError, ContactInput, Log, Mode, Sideband, satellite};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::satnogs::RigMode;

/// Everything logging needs, present only when the operator has a callsign.
pub struct Logging {
    pub log: Log,
    /// Own 6-character locator used verbatim instead of the one derived from the observer.
    pub grid_override: Option<String>,
    /// Satellite name as shown in the UI, to the name written as `SAT_NAME`.
    pub sat_names: BTreeMap<String, String>,
}

impl Logging {
    pub fn new(log: Log) -> Self {
        Self {
            log,
            grid_override: None,
            sat_names: BTreeMap::new(),
        }
    }

    pub(crate) fn sat_name<'a>(&'a self, shown: &'a str) -> &'a str {
        self.sat_names.get(shown).map_or(shown, String::as_str)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Call,
    Grid,
    Name,
    RstSent,
    RstRcvd,
    Comment,
}

const FIELDS: [Field; 6] = [
    Field::Call,
    Field::Grid,
    Field::Name,
    Field::RstSent,
    Field::RstRcvd,
    Field::Comment,
];

/// The Contact form. Everything above the input fields is a snapshot taken when it was opened.
#[derive(Debug, Clone)]
pub struct ContactForm {
    pub satellite: String,
    pub transmitter: String,
    pub started: DateTime<Utc>,
    /// The frequencies last sent to the radio when the form was opened.
    pub rx_hz: u64,
    pub tx_hz: u64,
    pub rx_mode: RigMode,
    pub tx_mode: RigMode,
    pub call: String,
    pub grid: String,
    pub name: String,
    pub rst_sent: String,
    pub rst_rcvd: String,
    pub comment: String,
    pub field: Field,
    pub error: Option<String>,
}

/// What a key did to the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FormEvent {
    Edited,
    Save,
    Cancel,
}

impl ContactForm {
    pub(crate) fn new(
        satellite: String,
        transmitter: String,
        started: DateTime<Utc>,
        (rx_hz, rx_mode): (u64, RigMode),
        (tx_hz, tx_mode): (u64, RigMode),
    ) -> Self {
        let rst = if tx_mode == RigMode::Cw { "599" } else { "59" };
        Self {
            satellite,
            transmitter,
            started,
            rx_hz,
            tx_hz,
            rx_mode,
            tx_mode,
            call: String::new(),
            grid: String::new(),
            name: String::new(),
            rst_sent: rst.to_string(),
            rst_rcvd: rst.to_string(),
            comment: String::new(),
            field: Field::Call,
            error: None,
        }
    }

    fn text_mut(&mut self) -> &mut String {
        match self.field {
            Field::Call => &mut self.call,
            Field::Grid => &mut self.grid,
            Field::Name => &mut self.name,
            Field::RstSent => &mut self.rst_sent,
            Field::RstRcvd => &mut self.rst_rcvd,
            Field::Comment => &mut self.comment,
        }
    }

    fn step(&mut self, delta: usize) {
        let at = FIELDS.iter().position(|f| *f == self.field).unwrap_or(0);
        self.field = FIELDS[(at + delta) % FIELDS.len()];
    }

    /// The form captures every key.
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> FormEvent {
        match key.code {
            KeyCode::Esc => return FormEvent::Cancel,
            KeyCode::Enter => return FormEvent::Save,
            KeyCode::Tab => self.step(1),
            KeyCode::BackTab => self.step(FIELDS.len() - 1),
            KeyCode::Backspace => {
                self.text_mut().pop();
            }
            KeyCode::Char(c) => {
                let upper = matches!(self.field, Field::Call | Field::Grid);
                self.text_mut().extend(if upper {
                    c.to_uppercase().collect::<Vec<_>>()
                } else {
                    vec![c]
                });
            }
            _ => {}
        }
        FormEvent::Edited
    }

    /// Build the Contact ended at `end`. `Call is required` is checked here so the wording is
    /// the form's, not the field name's.
    pub(crate) fn contact(
        &self,
        end: DateTime<Utc>,
        own_grid: String,
        sat_name: &str,
    ) -> Result<Contact, String> {
        if self.call.trim().is_empty() {
            return Err("Call is required".to_string());
        }
        let extras = satellite::extras(sat_name, self.tx_hz, self.rx_hz).map_err(describe)?;
        Contact::new(ContactInput {
            call: self.call.clone(),
            grid: Some(self.grid.clone()),
            name: Some(self.name.clone()),
            comment: Some(self.comment.clone()),
            rst_sent: Some(self.rst_sent.clone()),
            rst_rcvd: Some(self.rst_rcvd.clone()),
            start: self.started,
            end: end.max(self.started),
            freq_hz: self.tx_hz,
            mode: logbook_mode(self.tx_mode),
            own_grid: Some(own_grid),
            station_callsign: None,
            extras,
        })
        .map_err(describe)
    }
}

fn describe(e: ContactError) -> String {
    e.to_string()
}

/// sat-tui's rig mode as `logbook`'s; `Other` is never needed here.
pub fn logbook_mode(mode: RigMode) -> Mode {
    match mode {
        RigMode::Fm => Mode::Fm,
        RigMode::Usb => Mode::Ssb(Sideband::Upper),
        RigMode::Lsb => Mode::Ssb(Sideband::Lower),
        RigMode::Cw => Mode::Cw,
        RigMode::Am => Mode::Am,
    }
}

/// One Contact logged this session, as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedContact {
    pub time_on: DateTime<Utc>,
    pub call: String,
    pub grid: String,
    pub rst_sent: String,
    pub rst_rcvd: String,
    pub satellite: String,
}
