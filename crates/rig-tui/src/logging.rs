//! Logging a Contact: the snapshot form, the in-memory session list and the mapping from the
//! rig's mode onto `logbook`. No I/O beyond the [`Log`] it is given; `App` supplies the clock.

use chrono::{DateTime, Utc};
use hamtools_tui::form::{Form, FormEvent, TextField};
use hamtools_tui::list::{ListEvent, ListOverlay};
use hamtools_tui::modal::Modal;
use logbook::{Contact, ContactInput, Grid, Log, Mode, Sideband};
use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::panel::{LogDefaults, LogMode};
use crate::rig::RigMode;
use crate::style::{dim, red};
use crate::ui::format_hz;

// Field positions in the form.
const CALL: usize = 0;
const MODE: usize = 1;
const SUBMODE: usize = 2;
const GRID: usize = 3;
const NAME: usize = 4;
const RST_SENT: usize = 5;
const RST_RCVD: usize = 6;
const COMMENT: usize = 7;

/// The mapping used when no Mode panel supplies one. `None` for a data mode or anything unclear:
/// the form then asks for the ADIF mode to be typed.
pub(crate) fn default_log_mode(mode: &RigMode) -> Option<LogMode> {
    let (adif, submode, rst) = match mode {
        RigMode::Usb => ("SSB", Some("USB"), "59"),
        RigMode::Lsb => ("SSB", Some("LSB"), "59"),
        RigMode::Cw | RigMode::Cwr => ("CW", None, "599"),
        RigMode::Am => ("AM", None, "59"),
        RigMode::Fm => ("FM", None, "59"),
        RigMode::Rtty | RigMode::RttyR => ("RTTY", None, "599"),
        RigMode::Other(_) => return None,
    };
    Some(LogMode {
        mode: adif.to_string(),
        submode: submode.map(String::from),
        default_rst: Some(rst.to_string()),
    })
}

/// What `App` captured when the operator asked to log.
pub(crate) struct Snapshot {
    pub hz: u64,
    pub rig_mode: RigMode,
    pub started: DateTime<Utc>,
    /// The panel's mapping, if it has one, else [`default_log_mode`].
    pub log_mode: Option<LogMode>,
    pub defaults: LogDefaults,
}

/// The log form. Frequency, mode and start time are fixed at open; only the typed fields change.
struct ContactForm {
    form: Form,
    hz: u64,
    rig_mode: RigMode,
    started: DateTime<Utc>,
    extras: Vec<(String, String)>,
    /// A save problem not tied to a field (shown under the fields).
    error: Option<String>,
    /// A key edited a field since the form opened.
    dirty: bool,
}

impl ContactForm {
    fn new(snapshot: Snapshot) -> Self {
        let Snapshot {
            hz,
            rig_mode,
            started,
            log_mode,
            defaults,
        } = snapshot;
        let log_mode = defaults.mode.or(log_mode);
        let (mode, submode, mode_rst) = match &log_mode {
            Some(m) => (
                m.mode.clone(),
                m.submode.clone().unwrap_or_default(),
                m.default_rst.clone(),
            ),
            None => (String::new(), String::new(), None),
        };
        let rst = defaults.rst.or(mode_rst).unwrap_or_default();
        let mode_label = if mode.is_empty() {
            "Mode (required)"
        } else {
            "Mode"
        };
        let form = Form::new(vec![
            TextField::new("Call").uppercase(),
            TextField::new(mode_label).uppercase().with_value(mode),
            TextField::new("Submode").uppercase().with_value(submode),
            TextField::new("Grid").uppercase(),
            TextField::new("Name"),
            TextField::new("RST sent").with_value(rst.clone()),
            TextField::new("RST rcvd").with_value(rst),
            TextField::new("Comment"),
        ]);
        Self {
            form,
            hz,
            rig_mode,
            started,
            extras: defaults.extra_adif,
            error: None,
            dirty: false,
        }
    }

    /// Build the Contact ended at `end`. `Call is required` and `Mode is required` are checked
    /// here so the wording is the form's.
    fn contact(&self, end: DateTime<Utc>, own_grid: Option<&Grid>) -> Result<Contact, String> {
        let value = |i| self.form.value(i).to_string();
        if value(CALL).trim().is_empty() {
            return Err("Call is required".to_string());
        }
        let mode = value(MODE);
        if mode.trim().is_empty() {
            return Err("Mode is required".to_string());
        }
        Contact::new(ContactInput {
            call: value(CALL),
            grid: Some(value(GRID)),
            name: Some(value(NAME)),
            comment: Some(value(COMMENT)),
            rst_sent: Some(value(RST_SENT)),
            rst_rcvd: Some(value(RST_RCVD)),
            start: self.started,
            end,
            freq_hz: self.hz,
            mode: logbook_mode(&mode, &value(SUBMODE)),
            own_grid: own_grid.cloned(),
            station_callsign: None,
            extras: self.extras.clone(),
        })
        .map_err(|e| e.to_string())
    }
}

/// The typed ADIF mode and submode as `logbook`'s, using its named modes where they match.
fn logbook_mode(mode: &str, submode: &str) -> Mode {
    let (mode, submode) = (
        mode.trim().to_ascii_uppercase(),
        submode.trim().to_ascii_uppercase(),
    );
    match (mode.as_str(), submode.as_str()) {
        ("SSB", "USB") => Mode::Ssb(Sideband::Upper),
        ("SSB", "LSB") => Mode::Ssb(Sideband::Lower),
        ("CW", "") => Mode::Cw,
        ("AM", "") => Mode::Am,
        ("FM", "") => Mode::Fm,
        _ => Mode::Other {
            mode,
            submode: (!submode.is_empty()).then_some(submode),
        },
    }
}

/// One Contact logged this run, as the list shows it.
#[derive(Debug, Clone)]
struct Logged {
    start: DateTime<Utc>,
    call: String,
    hz: u64,
    mode: String,
    rst_sent: String,
    rst_rcvd: String,
}

impl Logged {
    fn new(contact: &Contact) -> Self {
        let (mode, submode) = contact.mode().adif();
        Self {
            start: contact.start(),
            call: contact.call().to_string(),
            hz: contact.freq_hz(),
            mode: submode.unwrap_or(mode).to_string(),
            rst_sent: contact.rst_sent().unwrap_or_default().to_string(),
            rst_rcvd: contact.rst_rcvd().unwrap_or_default().to_string(),
        }
    }

    fn line(&self) -> String {
        format!(
            "{}  {:<10}  {} {}  {}/{}",
            self.start.format("%H:%M:%SZ"),
            self.call,
            format_hz(self.hz),
            self.mode,
            self.rst_sent,
            self.rst_rcvd
        )
    }
}

/// Everything logging needs: the Log, the operator's Locator, the form and the session.
#[derive(Default)]
pub(crate) struct Logging {
    form: Option<ContactForm>,
    list: Option<ListOverlay>,
    session: Vec<Logged>,
}

impl Logging {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn count(&self) -> usize {
        self.session.len()
    }

    /// A form or the session list is open and gets every key.
    pub fn modal_open(&self) -> bool {
        self.form.is_some() || self.list.is_some()
    }

    pub fn form_open(&self) -> bool {
        self.form.is_some()
    }

    /// The form has Contact text typed into it that closing would lose.
    pub fn form_dirty(&self) -> bool {
        self.form.as_ref().is_some_and(|f| f.dirty)
    }

    /// Only the session list is open (it holds nothing to lose).
    pub fn list_only(&self) -> bool {
        self.form.is_none() && self.list.is_some()
    }

    pub fn open_form(&mut self, snapshot: Snapshot) {
        self.list = None;
        self.form = Some(ContactForm::new(snapshot));
    }

    pub fn open_list(&mut self) {
        let items = self.session.iter().rev().map(Logged::line).collect();
        self.list = Some(ListOverlay::new(items));
    }

    /// Feed a key to the open form or list. Returns a status message when the key finished the
    /// form with news for the operator.
    pub fn on_key(
        &mut self,
        key: KeyEvent,
        now: DateTime<Utc>,
        log: Result<Log, String>,
        own_grid: Option<&Grid>,
    ) -> Option<String> {
        if let Some(list) = &mut self.list {
            if matches!(list.handle_key(key), ListEvent::Close) {
                self.list = None;
            }
            return None;
        }
        let form = self.form.as_mut()?;
        match form.form.handle_key(key) {
            FormEvent::Edited => {
                form.error = None;
                form.dirty = true;
                None
            }
            FormEvent::Cancel => {
                self.form = None;
                None
            }
            FormEvent::Submit => self.save(now, log, own_grid),
        }
    }

    fn save(
        &mut self,
        now: DateTime<Utc>,
        log: Result<Log, String>,
        own_grid: Option<&Grid>,
    ) -> Option<String> {
        let form = self.form.as_mut()?;
        let contact = match form.contact(now, own_grid) {
            Ok(contact) => contact,
            Err(message) => {
                form.error = Some(message);
                return None;
            }
        };
        let log = match log {
            Ok(log) => log,
            Err(why) => return Some(format!("not logged: {why}")),
        };
        if let Err(e) = log.append(&contact) {
            form.error = None;
            return Some(format!("not logged: {e}"));
        }
        let logged = Logged::new(&contact);
        let status = format!("logged {}", logged.call);
        self.session.push(logged);
        self.form = None;
        Some(status)
    }

    /// The newest `n` Contacts of this run, newest first, one line each.
    pub fn recent(&self, n: usize) -> Vec<String> {
        self.session
            .iter()
            .rev()
            .take(n)
            .map(Logged::line)
            .collect()
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, has_locator: bool) {
        if let Some(form) = &self.form {
            let title = if has_locator {
                "Log contact".to_string()
            } else {
                "Log contact (no locator)".to_string()
            };
            let inner = Modal::new(title, 60, 14).render(frame, area);
            let [head, fields, error] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .areas(inner);
            let when = form.started.format("%H:%M:%SZ");
            frame.render_widget(
                Paragraph::new(format!("{}  {}  {when}", format_hz(form.hz), form.rig_mode))
                    .style(dim()),
                head,
            );
            form.form.render(frame, fields);
            if let Some(message) = &form.error {
                frame.render_widget(Paragraph::new(Line::styled(message.clone(), red())), error);
            }
        } else if let Some(list) = &self.list {
            let rows = self.session.len().clamp(1, 14) as u16;
            let inner = Modal::new(
                format!("Session: {} logged", self.session.len()),
                64,
                rows + 2,
            )
            .render(frame, area);
            if self.session.is_empty() {
                frame.render_widget(Paragraph::new("No contacts logged this session."), inner);
            } else {
                list.render(frame, inner);
            }
        }
    }
}
