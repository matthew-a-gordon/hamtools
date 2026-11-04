//! Logging a Contact: the modal form, the session list and the mapping onto `logbook`.
//! No I/O beyond the [`Log`] the [`App`](crate::app::App) is given.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use logbook::{Contact, ContactInput, Log, Mode, Sideband, satellite};
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
        // RST sent is the report for our uplink, RST received the one for what we hear.
        let rst = |mode| if mode == RigMode::Cw { "599" } else { "59" };
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
            rst_sent: rst(tx_mode).to_string(),
            rst_rcvd: rst(rx_mode).to_string(),
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
                if matches!(self.field, Field::Call | Field::Grid) {
                    // Check before uppercasing: `\u{df}` uppercases to the ASCII "SS".
                    if !c.is_ascii() {
                        let name = if self.field == Field::Call {
                            "Call"
                        } else {
                            "Grid"
                        };
                        self.error = Some(format!("{name}: {c:?} is not ASCII"));
                        return FormEvent::Edited;
                    }
                    self.text_mut().push(c.to_ascii_uppercase());
                } else {
                    self.text_mut().push(c);
                }
                self.error = None;
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
        let extras =
            satellite::extras(sat_name, self.tx_hz, self.rx_hz).map_err(|e| e.to_string())?;
        Contact::new(ContactInput {
            call: self.call.clone(),
            grid: Some(self.grid.clone()),
            name: Some(self.name.clone()),
            comment: Some(self.comment.clone()),
            rst_sent: Some(self.rst_sent.clone()),
            rst_rcvd: Some(self.rst_rcvd.clone()),
            start: self.started,
            end,
            freq_hz: self.tx_hz,
            mode: logbook_mode(self.tx_mode),
            own_grid: Some(own_grid),
            station_callsign: None,
            extras,
        })
        .map_err(|e| e.to_string())
    }
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

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn form(rx: RigMode, tx: RigMode) -> ContactForm {
        let started = Utc.with_ymd_and_hms(2026, 10, 26, 13, 15, 0).unwrap();
        ContactForm::new(
            "AO-7".to_string(),
            "Mode B".to_string(),
            started,
            (435_000_000, rx),
            (145_900_000, tx),
        )
    }

    #[test]
    fn rst_sent_follows_the_uplink_mode_and_rst_received_the_downlink_mode() {
        let f = form(RigMode::Usb, RigMode::Cw);
        assert_eq!((f.rst_sent.as_str(), f.rst_rcvd.as_str()), ("599", "59"));
        let f = form(RigMode::Cw, RigMode::Usb);
        assert_eq!((f.rst_sent.as_str(), f.rst_rcvd.as_str()), ("59", "599"));
        let f = form(RigMode::Cw, RigMode::Cw);
        assert_eq!((f.rst_sent.as_str(), f.rst_rcvd.as_str()), ("599", "599"));
    }
}
