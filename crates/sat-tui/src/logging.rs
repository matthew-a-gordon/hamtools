//! Logging a Contact: the modal form, the session list and the mapping onto `logbook`.
//! No I/O beyond the [`Log`] the [`App`](crate::app::App) is given.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use logbook::{Contact, ContactInput, Grid, Log, Mode, Sideband, satellite};
use orbit::Observer;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::satnogs::RigMode;

/// Everything logging needs, present only when the operator has a callsign.
pub struct Logging {
    pub log: Log,
    /// Own locator used verbatim instead of the one derived from the observer.
    pub grid_override: Option<Grid>,
    /// Satellite name as shown in the UI, to the name written as `SAT_NAME`.
    pub sat_names: BTreeMap<String, String>,
    form: Option<ContactForm>,
    session: Vec<LoggedContact>,
    /// `Some(n)` while the session list is open, scrolled `n` lines back from the newest.
    session_scroll: Option<usize>,
}

impl Logging {
    pub fn new(log: Log) -> Self {
        Self {
            log,
            grid_override: None,
            sat_names: BTreeMap::new(),
            form: None,
            session: Vec::new(),
            session_scroll: None,
        }
    }

    /// The operator's own locator for a Contact: the config override, else the observer's.
    pub(crate) fn own_grid(&self, observer: &Observer) -> Grid {
        (self.grid_override.clone()).unwrap_or_else(|| Grid::at(observer.lat_deg, observer.lon_deg))
    }

    pub fn form(&self) -> Option<&ContactForm> {
        self.form.as_ref()
    }

    /// Show `form`, unless the Log cannot be written to.
    pub(crate) fn open_form(&mut self, form: ContactForm) -> Result<(), String> {
        self.log.check().map_err(|e| format!("log: {e}"))?;
        self.form = Some(form);
        Ok(())
    }

    /// Feed a key to the open form. Returns a status message when the key finished the form
    /// with news for the operator.
    pub(crate) fn handle_form_key(&mut self, key: KeyEvent, now: DateTime<Utc>) -> Option<String> {
        match self.form.as_mut()?.handle_key(key) {
            FormEvent::Edited => None,
            FormEvent::Cancel => {
                self.form = None;
                None
            }
            FormEvent::Save => self.save_form(now),
        }
    }

    fn save_form(&mut self, now: DateTime<Utc>) -> Option<String> {
        let form = self.form.as_mut()?;
        let sat_name = (self.sat_names.get(&form.satellite))
            .unwrap_or(&form.satellite)
            .clone();
        let contact = match form.contact(now, &sat_name) {
            Ok(contact) => contact,
            Err(message) => {
                form.error = Some(message);
                return None;
            }
        };
        if let Err(e) = self.log.append(&contact) {
            form.error = None;
            return Some(format!("not logged: {e}"));
        }
        let logged = LoggedContact::new(&contact, sat_name);
        let status = format!("logged {}", logged.call);
        self.session.push(logged);
        self.form = None;
        Some(status)
    }

    /// This session's Contacts, oldest first.
    pub fn session(&self) -> &[LoggedContact] {
        &self.session
    }

    /// Hand the session list over, so it can survive the app being rebuilt.
    pub fn take_session(&mut self) -> Vec<LoggedContact> {
        std::mem::take(&mut self.session)
    }

    pub fn restore_session(&mut self, session: Vec<LoggedContact>) {
        self.session = session;
    }

    pub fn session_open(&self) -> bool {
        self.session_scroll.is_some()
    }

    /// Lines the session list is scrolled back from the newest Contact.
    pub fn session_scroll(&self) -> usize {
        self.session_scroll.unwrap_or(0)
    }

    pub(crate) fn open_session(&mut self) {
        self.session_scroll = Some(0);
    }

    pub(crate) fn handle_session_key(&mut self, key: KeyEvent) {
        let Some(scroll) = self.session_scroll else {
            return;
        };
        let oldest = self.session.len().saturating_sub(1);
        match key.code {
            KeyCode::Char('L') | KeyCode::Esc => self.session_scroll = None,
            KeyCode::Up | KeyCode::Char('k') => {
                self.session_scroll = Some((scroll + 1).min(oldest))
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.session_scroll = Some(scroll.saturating_sub(1));
            }
            _ => {}
        }
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

/// One side of the radio link: a frequency and the mode it is worked in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub hz: u64,
    pub mode: RigMode,
}

/// The Contact form. Everything above the input fields is a snapshot taken when it was opened.
#[derive(Debug, Clone)]
pub struct ContactForm {
    pub satellite: String,
    pub transmitter: String,
    pub started: DateTime<Utc>,
    /// The operator's own locator for this Contact.
    pub own_grid: Grid,
    /// The frequencies last sent to the radio when the form was opened.
    pub rx: Link,
    pub tx: Link,
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
        own_grid: Grid,
        rx: Link,
        tx: Link,
    ) -> Self {
        // RST sent is the report for our uplink, RST received the one for what we hear.
        let rst = |mode| if mode == RigMode::Cw { "599" } else { "59" };
        Self {
            satellite,
            transmitter,
            started,
            own_grid,
            rx,
            tx,
            call: String::new(),
            grid: String::new(),
            name: String::new(),
            rst_sent: rst(tx.mode).to_string(),
            rst_rcvd: rst(rx.mode).to_string(),
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
    pub(crate) fn contact(&self, end: DateTime<Utc>, sat_name: &str) -> Result<Contact, String> {
        if self.call.trim().is_empty() {
            return Err("Call is required".to_string());
        }
        let extras =
            satellite::extras(sat_name, self.tx.hz, self.rx.hz).map_err(|e| e.to_string())?;
        Contact::new(ContactInput {
            call: self.call.clone(),
            grid: Some(self.grid.clone()),
            name: Some(self.name.clone()),
            comment: Some(self.comment.clone()),
            rst_sent: Some(self.rst_sent.clone()),
            rst_rcvd: Some(self.rst_rcvd.clone()),
            start: self.started,
            end,
            freq_hz: self.tx.hz,
            mode: logbook_mode(self.tx.mode),
            own_grid: Some(self.own_grid.clone()),
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

impl LoggedContact {
    fn new(contact: &Contact, satellite: String) -> Self {
        Self {
            time_on: contact.start(),
            call: contact.call().to_string(),
            grid: contact.grid().unwrap_or_default().to_string(),
            rst_sent: contact.rst_sent().unwrap_or_default().to_string(),
            rst_rcvd: contact.rst_rcvd().unwrap_or_default().to_string(),
            satellite,
        }
    }
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
            Grid::new("EN71DQ").unwrap(),
            Link {
                hz: 435_000_000,
                mode: rx,
            },
            Link {
                hz: 145_900_000,
                mode: tx,
            },
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
