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

use crate::panel::{LogDefaults, LogMode, Role};
use crate::rig::RigMode;
use crate::style::{dim, red};
use crate::ui::format_hz;

/// What a form field holds. The Role decides which are shown and in what order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Call,
    Mode,
    Submode,
    Grid,
    Name,
    RstSent,
    RstRcvd,
    TheirRef,
    Comment,
}

fn kinds_for(role: Role) -> Vec<Kind> {
    use Kind::*;
    if role.has_their_ref() {
        // Quick-entry order: what a pileup needs first, the mode last.
        vec![Call, RstSent, RstRcvd, TheirRef, Comment, Mode, Submode]
    } else {
        vec![Call, Mode, Submode, Grid, Name, RstSent, RstRcvd, Comment]
    }
}

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

/// The form's in-progress values, which `App` publishes as Soft key variables.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContactView<'a> {
    pub their_call: &'a str,
    pub their_ref: &'a str,
    pub rst_sent: &'a str,
    pub rst_rcvd: &'a str,
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
    kinds: Vec<Kind>,
    role: Role,
    my_refs: Vec<String>,
    hz: u64,
    rig_mode: RigMode,
    started: DateTime<Utc>,
    extras: Vec<(String, String)>,
    /// A save problem not tied to a field (shown under the fields).
    error: Option<String>,
    /// A key changed a field's text since the form opened.
    dirty: bool,
    /// The field values as the form opened, to tell a real edit from a Tab or an arrow.
    baseline: Vec<String>,
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
        let role = defaults.role;
        let kinds = kinds_for(role);
        let their_ref = defaults.their_ref.unwrap_or_default();
        let fields = kinds
            .iter()
            .map(|kind| match kind {
                Kind::Call => TextField::new("Call").uppercase(),
                Kind::Mode => TextField::new(mode_label)
                    .uppercase()
                    .with_value(mode.clone()),
                Kind::Submode => TextField::new("Submode")
                    .uppercase()
                    .with_value(submode.clone()),
                Kind::Grid => TextField::new("Grid").uppercase(),
                Kind::Name => TextField::new("Name"),
                Kind::RstSent => TextField::new("RST sent").with_value(rst.clone()),
                Kind::RstRcvd => TextField::new("RST rcvd").with_value(rst.clone()),
                Kind::TheirRef => TextField::new("Their ref")
                    .uppercase()
                    .with_value(their_ref.clone()),
                Kind::Comment => TextField::new("Comment"),
            })
            .collect();
        let form = Form::new(fields);
        let baseline = (0..kinds.len())
            .map(|i| form.value(i).to_string())
            .collect();
        Self {
            form,
            kinds,
            role,
            my_refs: defaults.my_refs,
            hz,
            rig_mode,
            started,
            extras: defaults.extra_adif,
            error: None,
            dirty: false,
            baseline,
        }
    }

    /// Whether any field's text differs from what the form opened with.
    fn changed(&self) -> bool {
        self.baseline
            .iter()
            .enumerate()
            .any(|(i, v)| self.form.value(i) != v)
    }

    /// The label a field is drawn with.
    fn label(&self, kind: Kind) -> &'static str {
        match kind {
            Kind::Call => "Call",
            Kind::Mode => "Mode",
            Kind::Submode => "Submode",
            Kind::Grid => "Grid",
            Kind::Name => "Name",
            Kind::RstSent => "RST sent",
            Kind::RstRcvd => "RST rcvd",
            Kind::TheirRef => "Their ref",
            Kind::Comment => "Comment",
        }
    }

    fn value(&self, kind: Kind) -> &str {
        match self.kinds.iter().position(|k| *k == kind) {
            Some(i) => self.form.value(i),
            None => "",
        }
    }

    /// The reference extras as standard ADIF fields: POTA `MY_SIG`/`MY_SIG_INFO` and
    /// `SIG`/`SIG_INFO`, SOTA `MY_SOTA_REF`/`SOTA_REF`. No Activity field.
    fn reference_extras(&self) -> Vec<(String, String)> {
        let theirs = self.value(Kind::TheirRef).trim().to_string();
        let mut out = Vec::new();
        match self.role {
            Role::PotaActivator | Role::PotaHunter => {
                if !self.my_refs.is_empty() {
                    out.push(("MY_SIG".to_string(), "POTA".to_string()));
                    out.push(("MY_SIG_INFO".to_string(), self.my_refs.join(",")));
                }
                if !theirs.is_empty() {
                    out.push(("SIG".to_string(), "POTA".to_string()));
                    out.push(("SIG_INFO".to_string(), theirs));
                }
            }
            Role::SotaActivator | Role::SotaChaser => {
                if let Some(mine) = self.my_refs.first() {
                    out.push(("MY_SOTA_REF".to_string(), mine.clone()));
                }
                if !theirs.is_empty() {
                    out.push(("SOTA_REF".to_string(), theirs));
                }
            }
            Role::General | Role::Satellite => {}
        }
        out
    }

    /// Build the Contact ended at `end`. `Call is required` and `Mode is required` are checked
    /// here so the wording is the form's.
    fn contact(&self, end: DateTime<Utc>, own_grid: Option<&Grid>) -> Result<Contact, String> {
        let value = |kind| self.value(kind).to_string();
        if value(Kind::Call).trim().is_empty() {
            return Err("Call is required".to_string());
        }
        let mode = value(Kind::Mode);
        if mode.trim().is_empty() {
            return Err("Mode is required".to_string());
        }
        Contact::new(ContactInput {
            call: value(Kind::Call),
            grid: Some(value(Kind::Grid)),
            name: Some(value(Kind::Name)),
            comment: Some(value(Kind::Comment)),
            rst_sent: Some(value(Kind::RstSent)),
            rst_rcvd: Some(value(Kind::RstRcvd)),
            start: self.started,
            end,
            freq_hz: self.hz,
            mode: logbook_mode(&mode, &value(Kind::Submode)),
            own_grid: own_grid.cloned(),
            station_callsign: None,
            extras: self
                .extras
                .iter()
                .cloned()
                .chain(self.reference_extras())
                .collect(),
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
    /// Their reference, if the Contact was park-to-park or summit-to-summit.
    their_ref: String,
    /// `P` for park-to-park, `S` for summit-to-summit, else a space.
    mark: char,
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
            their_ref: String::new(),
            mark: ' ',
        }
    }

    fn with_ref(mut self, their_ref: &str, role: Role) -> Self {
        self.their_ref = their_ref.trim().to_string();
        self.mark = match (self.their_ref.is_empty(), role) {
            (true, _) => ' ',
            (false, Role::PotaActivator | Role::PotaHunter) => 'P',
            (false, Role::SotaActivator | Role::SotaChaser) => 'S',
            (false, _) => ' ',
        };
        self
    }

    /// The Activator's recent-list row: time, call, reports, Their ref and the mark.
    fn activator_line(&self) -> String {
        format!(
            "{}  {:<10} {:>3}/{:<3} {:<12} {}",
            self.start.format("%H:%MZ"),
            self.call,
            self.rst_sent,
            self.rst_rcvd,
            self.their_ref,
            self.mark
        )
        .trim_end()
        .to_string()
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
    /// An activator Contact was just saved: `App` reopens the form on a fresh snapshot.
    reopen: bool,
    /// Why an embedded form could not open (not connected, no callsign, ...).
    wait: Option<String>,
}

impl Logging {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn count(&self) -> usize {
        self.session.len()
    }

    /// Why an embedded form is not open yet, for its placeholder.
    pub fn set_wait(&mut self, why: Option<String>) {
        self.wait = why;
    }

    pub fn form_open(&self) -> bool {
        self.form.is_some()
    }

    /// The form has Contact text typed into it that closing would lose.
    pub fn form_dirty(&self) -> bool {
        self.form.as_ref().is_some_and(|f| f.dirty)
    }

    /// The form's in-progress values, for the Activity's Soft key variables.
    pub fn view(&self) -> Option<ContactView<'_>> {
        let form = self.form.as_ref()?;
        Some(ContactView {
            their_call: form.value(Kind::Call),
            their_ref: form.value(Kind::TheirRef),
            rst_sent: form.value(Kind::RstSent),
            rst_rcvd: form.value(Kind::RstRcvd),
        })
    }

    /// Whether the last save asks for the form to be opened again (Activator quick entry).
    /// Reading it clears it.
    pub fn take_reopen(&mut self) -> bool {
        std::mem::take(&mut self.reopen)
    }

    /// Put a Call in the open form (a spot's) and move to the next field, ready for the reports.
    /// Not counted as typing, so Esc still closes without asking.
    pub fn prefill_call(&mut self, call: &str) {
        if let Some(form) = &mut self.form
            && let Some(i) = form.kinds.iter().position(|k| *k == Kind::Call)
        {
            form.form.set_value(i, call.to_ascii_uppercase());
            form.form.set_focus(i + 1);
        }
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
        let call_was_empty = form.value(Kind::Call).is_empty();
        match form.form.handle_key(key) {
            FormEvent::Edited => {
                form.error = None;
                form.dirty = form.dirty || form.changed();
                // An Activator's Contact starts with its first call character.
                if form.role.is_activator() && call_was_empty && !form.value(Kind::Call).is_empty()
                {
                    form.started = now;
                }
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
        let logged = Logged::new(&contact).with_ref(form.value(Kind::TheirRef), form.role);
        let status = format!("logged {}", logged.call);
        self.session.push(logged);
        self.reopen = form.role.is_activator();
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

    /// The Activator's recent list, newest first.
    pub fn recent_activator(&self, n: usize) -> Vec<String> {
        self.session
            .iter()
            .rev()
            .take(n)
            .map(Logged::activator_line)
            .collect()
    }

    pub fn list_open(&self) -> bool {
        self.list.is_some()
    }

    pub fn form_mode(&self) -> String {
        self.form
            .as_ref()
            .map(|f| f.value(Kind::Mode).to_string())
            .unwrap_or_default()
    }

    pub fn form_hz(&self) -> Option<u64> {
        self.form.as_ref().map(|f| f.hz)
    }

    /// Throw away the open form (an embedded form is then reopened fresh by the host).
    pub fn discard(&mut self) {
        self.form = None;
    }

    /// Bring an open form up to date with the rig. A form nobody has typed into is rebuilt when
    /// the frequency or mode moved, so its Mode and RST follow the rig; a half-typed one keeps
    /// its text and only takes the new frequency. My references always follow the plan.
    pub fn refresh(&mut self, snapshot: Snapshot) {
        let Some(form) = &mut self.form else { return };
        form.my_refs.clone_from(&snapshot.defaults.my_refs);
        if snapshot.hz == form.hz && snapshot.rig_mode == form.rig_mode {
            return;
        }
        if form.dirty {
            form.hz = snapshot.hz;
            form.rig_mode = snapshot.rig_mode;
        } else {
            self.form = Some(ContactForm::new(snapshot));
        }
    }

    /// Draw the open form inside `area` (a bordered block of at least 6 rows): the fields in
    /// two columns, the rig's frequency and mode and the start time in the title.
    pub fn render_embedded(&self, frame: &mut Frame, area: Rect, focused: bool) {
        let border = crate::style::focus_border(focused);
        let Some(form) = &self.form else {
            frame.render_widget(
                Paragraph::new(
                    self.wait
                        .clone()
                        .unwrap_or_else(|| "waiting for the rig's frequency and mode".to_string()),
                )
                .style(dim())
                .block(
                    ratatui::widgets::Block::default()
                        .borders(ratatui::widgets::Borders::ALL)
                        .border_style(border)
                        .title("Contact"),
                ),
                area,
            );
            return;
        };
        let state = if focused {
            "typing"
        } else {
            "strip keys, Tab to type"
        };
        let mut title = vec![ratatui::text::Span::raw(format!(
            "Contact {} {} {}Z | {state}",
            format_hz(form.hz),
            form.rig_mode,
            form.started.format("%H:%M:%S")
        ))];
        if let Some(message) = &form.error {
            title.push(ratatui::text::Span::styled(format!(" | {message}"), red()));
        }
        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(border)
            .title(Line::from(title));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(inner);
        let rows = usize::from(inner.height).max(1);
        let focus = form.form.focus();
        for (i, kind) in form.kinds.iter().enumerate() {
            let (column, row) = (i / rows, i % rows);
            let cell = if column == 0 { left } else { right };
            if column > 1 {
                break;
            }
            let label = format!("{}: ", form.label(*kind));
            let style = if focused && i == focus {
                ratatui::style::Style::default().add_modifier(
                    ratatui::style::Modifier::BOLD | ratatui::style::Modifier::REVERSED,
                )
            } else {
                ratatui::style::Style::default()
            };
            let value = form.form.value(i).to_string();
            let line_area = Rect::new(cell.x, cell.y + row as u16, cell.width, 1);
            if focused && i == focus {
                let x = (label.chars().count() + value.chars().count()) as u16;
                if x < cell.width {
                    frame.set_cursor_position((cell.x + x, line_area.y));
                }
            }
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    ratatui::text::Span::styled(label, style),
                    ratatui::text::Span::raw(value),
                ])),
                line_area,
            );
        }
    }

    /// Draw the session list or the form as a modal; `embedded` leaves the form to the Activity.
    pub fn render(&self, frame: &mut Frame, area: Rect, has_locator: bool, embedded: bool) {
        if let Some(form) = self.form.as_ref().filter(|_| !embedded) {
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
