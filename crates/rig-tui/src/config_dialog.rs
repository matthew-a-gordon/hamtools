//! The config dialog: one scrolling form over the shared `config.toml`.
//!
//! Pure apart from reading and writing that one file through `hamtools-config`. The address
//! lookup is a request out ([`Outcome::Lookup`]) and an answer in ([`ConfigDialog::on_geocode`]),
//! so `App` stays free of threads and sockets. The dialog speaks of message slots (Soft keys);
//! that they live in `[macros]` is `hamtools-config`'s business.

use std::path::{Path, PathBuf};

use hamtools_config::{Callsign, ConfigFile, FieldConflict, Grid, SaveError, Settings, Warning};
use hamtools_tui::form::{Form, TextField};
use hamtools_tui::list::{ListEvent, ListOverlay};
use hamtools_tui::modal::Modal;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::geocode::{Candidate, GeocodeResult};
use crate::style::{dim, red, yellow};

const LABEL_WIDTH: usize = 13;
const DISCLOSURE: [&str; 2] = [
    "Enter sends this address to OpenStreetMap (nominatim.openstreetmap.org);",
    "it is not saved.",
];

/// What a key did, for `App` to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    Stay,
    Close,
    /// Use these settings now and close (after a save, or `Ctrl-A` on a read-only file).
    Apply(Settings),
    /// Look this address up and answer with [`ConfigDialog::on_geocode`].
    Lookup(String),
}

enum Problem {
    /// The file does not parse (or could not be read); it is never saved over.
    Broken(String),
    /// A symlink, non-writable target or no location at all: edits apply for this run only.
    ReadOnly(String),
}

enum Mode {
    Editing,
    ConfirmDiscard,
    Candidates {
        list: ListOverlay,
        items: Vec<Candidate>,
    },
    Conflict {
        conflicts: Vec<FieldConflict>,
        next: usize,
    },
}

pub(crate) struct ConfigDialog {
    path: Option<PathBuf>,
    file: Option<ConfigFile>,
    problem: Option<Problem>,
    /// What the app is using now; stands in for the file's settings when there is no usable file.
    fallback: Settings,
    default_rig: String,
    form: Form,
    initial: Vec<String>,
    mode: Mode,
    looking_up: bool,
    note: Option<String>,
    message: Option<String>,
}

fn key_plain(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Number of Soft key slots (`F1` to `F6`).
const SLOTS: u8 = 6;

/// One row of the form. Adding a field is a variant here and an entry in [`Field::ALL`]; the
/// compiler then points at each `match` that needs an arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Callsign,
    Locator,
    /// Not a setting: the text sent to the address lookup.
    Address,
    LogPath,
    Rig,
    /// Soft key `F1` to `F6`, numbered from 1.
    Soft(u8),
}

impl Field {
    /// In display order; a field's position here is its index in the [`Form`].
    const ALL: [Field; 11] = [
        Field::Callsign,
        Field::Locator,
        Field::Address,
        Field::LogPath,
        Field::Rig,
        Field::Soft(1),
        Field::Soft(2),
        Field::Soft(3),
        Field::Soft(4),
        Field::Soft(5),
        Field::Soft(6),
    ];

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|f| *f == self)
            .expect("every field is in ALL")
    }

    fn at(index: usize) -> Field {
        Self::ALL[index]
    }

    fn label(self) -> String {
        match self {
            Field::Callsign => "Callsign".into(),
            Field::Locator => "Locator".into(),
            Field::Address => "Address".into(),
            Field::LogPath => "Log path".into(),
            Field::Rig => "Rig address".into(),
            Field::Soft(n) => format!("F{n}"),
        }
    }

    fn uppercase(self) -> bool {
        matches!(self, Field::Callsign | Field::Locator | Field::Soft(_))
    }

    /// The section heading drawn above this field, if it starts one.
    fn heading(self) -> Option<&'static str> {
        match self {
            Field::Callsign => Some("Station"),
            Field::LogPath => Some("Logging"),
            Field::Rig => Some("Rig"),
            Field::Soft(1) => Some("Soft keys"),
            _ => None,
        }
    }

    /// The field that holds the config key named in a conflict.
    fn from_key(key: &str) -> Option<Field> {
        match key {
            "callsign" => Some(Field::Callsign),
            "station.locator" => Some(Field::Locator),
            "logbook.path" => Some(Field::LogPath),
            "rig.addr" => Some(Field::Rig),
            _ => {
                let n: u8 = key.strip_prefix("macros.F")?.parse().ok()?;
                (1..=SLOTS).contains(&n).then_some(Field::Soft(n))
            }
        }
    }

    /// The text this field shows for `settings`.
    fn value_in(self, settings: &Settings, default_rig: &str) -> String {
        match self {
            Field::Callsign => settings
                .callsign
                .as_ref()
                .map_or(String::new(), |c| c.as_str().to_string()),
            Field::Locator => settings
                .locator
                .as_ref()
                .map_or(String::new(), |g| g.as_str().to_string()),
            Field::Address => String::new(),
            Field::LogPath => settings
                .log_path
                .as_ref()
                .map_or(String::new(), |p| p.to_string_lossy().into_owned()),
            Field::Rig => settings
                .rig_addr
                .clone()
                .unwrap_or_else(|| default_rig.to_string()),
            Field::Soft(n) => settings.macros.get(&n).cloned().unwrap_or_default(),
        }
    }

    /// Store the (already validated) form text into `settings`.
    fn apply(self, text: &str, settings: &mut Settings) {
        let text = text.trim();
        match self {
            Field::Callsign => settings.callsign = Callsign::new(text).ok(),
            Field::Locator => settings.locator = Grid::new(text).ok(),
            Field::Address => {}
            Field::LogPath => {
                settings.log_path = (!text.is_empty()).then(|| PathBuf::from(text));
            }
            Field::Rig => settings.rig_addr = Some(text.to_string()),
            Field::Soft(n) => {
                if text.is_empty() {
                    settings.macros.remove(&n);
                } else {
                    settings.macros.insert(n, text.to_string());
                }
            }
        }
    }

    fn check(self, value: &str) -> Result<(), String> {
        let value = value.trim();
        match self {
            Field::Callsign if !value.is_empty() => Callsign::new(value)
                .map(|_| ())
                .map_err(|_| "Callsign: 3 to 12 letters, digits or /".to_string()),
            Field::Locator if !value.is_empty() => Grid::new(value)
                .map(|_| ())
                .map_err(|_| "Locator: 6 characters like EN71DQ".to_string()),
            Field::Rig => {
                let ok = value.rsplit_once(':').is_some_and(|(host, port)| {
                    !host.is_empty()
                        && !host.chars().any(char::is_whitespace)
                        && port.parse::<u16>().is_ok_and(|p| p != 0)
                });
                if ok {
                    Ok(())
                } else {
                    Err("Rig address: required, like 127.0.0.1:4532 (host:port)".to_string())
                }
            }
            _ => Ok(()),
        }
    }
}

fn values_of(settings: &Settings, default_rig: &str) -> Vec<String> {
    Field::ALL
        .iter()
        .map(|f| f.value_in(settings, default_rig))
        .collect()
}

fn form_of(values: &[String]) -> Form {
    Form::new(
        Field::ALL
            .iter()
            .zip(values)
            .map(|(field, value)| {
                let text = TextField::new(field.label());
                let text = if field.uppercase() {
                    text.uppercase()
                } else {
                    text
                };
                text.with_value(value.clone())
            })
            .collect(),
    )
}

fn rows_needed(lines: &[Line], width: u16) -> u16 {
    let width = usize::from(width.max(1));
    lines
        .iter()
        .map(|l| l.width().div_ceil(width).max(1))
        .sum::<usize>() as u16
}

impl ConfigDialog {
    /// Open on a fresh read of the file at `path`. `current` is what the app is using now and
    /// `default_rig` the address in use, shown when the file does not set one.
    pub(crate) fn open(path: Option<&Path>, current: &Settings, default_rig: &str) -> Self {
        let (file, problem, settings) = match path {
            None => (
                None,
                Some(Problem::ReadOnly(
                    "no config file location (set HOME or pass --config)".to_string(),
                )),
                current.clone(),
            ),
            Some(path) => match ConfigFile::load(path) {
                Ok(file) => {
                    let problem = file
                        .writable()
                        .err()
                        .map(|e| Problem::ReadOnly(e.to_string()));
                    let settings = file.settings().clone();
                    (Some(file), problem, settings)
                }
                Err(e) => (None, Some(Problem::Broken(e.to_string())), current.clone()),
            },
        };
        let initial = values_of(&settings, default_rig);
        Self {
            path: path.map(Path::to_path_buf),
            file,
            problem,
            fallback: current.clone(),
            default_rig: default_rig.to_string(),
            form: form_of(&initial),
            initial,
            mode: Mode::Editing,
            looking_up: false,
            note: None,
            message: None,
        }
    }

    fn warnings(&self) -> &[Warning] {
        self.file.as_ref().map_or(&[], ConfigFile::warnings)
    }

    fn dirty(&self) -> bool {
        (0..Field::ALL.len())
            .any(|i| Field::at(i) != Field::Address && self.form.value(i) != self.initial[i])
    }

    fn changed(&self, index: usize) -> bool {
        self.form.value(index) != self.initial[index]
    }

    /// The settings the file holds now, or what the app uses when there is no usable file.
    fn base(&self) -> Settings {
        self.file
            .as_ref()
            .map_or_else(|| self.fallback.clone(), |f| f.settings().clone())
    }

    /// The base with only the fields the operator edited replaced, so untouched fields (and
    /// anything changed on disk meanwhile) are left alone.
    fn build_settings(&self) -> Settings {
        let mut s = self.base();
        for (i, field) in Field::ALL.iter().enumerate() {
            if self.changed(i) {
                field.apply(self.form.value(i), &mut s);
            }
        }
        s
    }

    fn validate_field(&mut self, index: usize) -> bool {
        match Field::at(index).check(self.form.value(index)) {
            Ok(()) => {
                self.form.clear_error(index);
                true
            }
            Err(message) => {
                self.form.set_error(index, message);
                false
            }
        }
    }

    /// Validate every field; focus goes to the first invalid one.
    fn validate_all(&mut self) -> bool {
        let mut first = None;
        for i in 0..Field::ALL.len() {
            if !self.validate_field(i) {
                first.get_or_insert(i);
            }
        }
        if let Some(i) = first {
            self.form.set_focus(i);
        }
        first.is_none()
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> Outcome {
        self.message = None;
        match std::mem::replace(&mut self.mode, Mode::Editing) {
            Mode::Editing => self.edit_key(key),
            Mode::ConfirmDiscard => {
                if matches!(key.code, KeyCode::Char('y' | 'Y')) {
                    Outcome::Close
                } else {
                    Outcome::Stay
                }
            }
            Mode::Candidates { mut list, items } => {
                match list.handle_key(key) {
                    ListEvent::Moved => self.mode = Mode::Candidates { list, items },
                    ListEvent::Close => {}
                    ListEvent::Select(i) => {
                        self.form
                            .set_value(Field::Locator.index(), items[i].locator.as_str());
                        self.form.set_focus(Field::Locator.index());
                    }
                }
                Outcome::Stay
            }
            Mode::Conflict { conflicts, next } => self.conflict_key(key, conflicts, next),
        }
    }

    fn edit_key(&mut self, key: KeyEvent) -> Outcome {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Char('s') => self.save(),
                KeyCode::Char('a') => self.apply_unsaved(),
                KeyCode::Char('r') => {
                    self.reload();
                    Outcome::Stay
                }
                _ => Outcome::Stay,
            };
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            return Outcome::Stay;
        }
        match key.code {
            KeyCode::Esc => {
                if self.dirty() {
                    self.mode = Mode::ConfirmDiscard;
                    Outcome::Stay
                } else {
                    Outcome::Close
                }
            }
            KeyCode::Enter if Field::at(self.form.focus()) == Field::Address => self.start_lookup(),
            KeyCode::Enter => {
                self.move_focus(key_plain(KeyCode::Tab));
                Outcome::Stay
            }
            _ => {
                self.move_focus(key);
                Outcome::Stay
            }
        }
    }

    /// Hand the key to the form; a field that was left is validated.
    fn move_focus(&mut self, key: KeyEvent) {
        let before = self.form.focus();
        self.form.handle_key(key);
        if self.form.focus() != before {
            self.validate_field(before);
        }
    }

    fn start_lookup(&mut self) -> Outcome {
        if self.looking_up {
            return Outcome::Stay;
        }
        let address = self.form.value(Field::Address.index()).trim().to_string();
        if address.is_empty() {
            self.note = Some("Type an address first.".to_string());
            return Outcome::Stay;
        }
        self.looking_up = true;
        self.note = Some("looking up…".to_string());
        Outcome::Lookup(address)
    }

    /// The answer to an [`Outcome::Lookup`]. Ignored if nothing is in flight.
    pub(crate) fn on_geocode(&mut self, result: GeocodeResult) {
        if !self.looking_up {
            return;
        }
        self.looking_up = false;
        if !matches!(self.mode, Mode::Editing) {
            self.note = None;
            return;
        }
        match result {
            Ok(found) if found.is_empty() => {
                self.note = Some("no match for that address; the Locator is untouched".to_string());
            }
            Ok(found) => {
                self.note = None;
                let items = found
                    .iter()
                    .map(|c| format!("{}  [{}]", c.name, c.locator))
                    .collect();
                self.mode = Mode::Candidates {
                    list: ListOverlay::new(items),
                    items: found,
                };
            }
            Err(e) => self.note = Some(e.to_string()),
        }
    }

    fn save(&mut self) -> Outcome {
        match &self.problem {
            Some(Problem::Broken(_)) => {
                self.message = Some(
                    "Save is disabled: the config file does not parse. Fix it, then Ctrl-R."
                        .to_string(),
                );
                return Outcome::Stay;
            }
            Some(Problem::ReadOnly(why)) => {
                self.message = Some(format!(
                    "Save is disabled: {why}. Ctrl-A applies for this run only."
                ));
                return Outcome::Stay;
            }
            None => {}
        }
        if !self.validate_all() {
            return Outcome::Stay;
        }
        let new = self.build_settings();
        let Some(file) = self.file.as_mut() else {
            return Outcome::Stay;
        };
        match file.save(&new) {
            Ok(_) => Outcome::Apply(file.settings().clone()),
            Err(SaveError::Conflict(conflicts)) => {
                self.mode = Mode::Conflict { conflicts, next: 0 };
                Outcome::Stay
            }
            Err(e @ SaveError::ReadOnly { .. }) => {
                self.problem = Some(Problem::ReadOnly(e.to_string()));
                Outcome::Stay
            }
            Err(SaveError::Unparseable { line, message }) => {
                self.problem = Some(Problem::Broken(format!("config: line {line}: {message}")));
                self.file = None;
                Outcome::Stay
            }
            Err(e @ SaveError::Io(_)) => {
                self.message = Some(e.to_string());
                Outcome::Stay
            }
        }
    }

    /// `Ctrl-A`: use the values for this run without writing the (read-only) file.
    fn apply_unsaved(&mut self) -> Outcome {
        if matches!(self.problem, Some(Problem::ReadOnly(_))) && self.validate_all() {
            Outcome::Apply(self.build_settings())
        } else {
            Outcome::Stay
        }
    }

    /// `Ctrl-R` on a file that did not parse: read it again, dropping unsaved edits.
    fn reload(&mut self) {
        if !matches!(self.problem, Some(Problem::Broken(_))) {
            return;
        }
        let Some(path) = &self.path else { return };
        match ConfigFile::load(path) {
            Ok(file) => {
                self.problem = file
                    .writable()
                    .err()
                    .map(|e| Problem::ReadOnly(e.to_string()));
                self.initial = values_of(file.settings(), &self.default_rig);
                self.form = form_of(&self.initial);
                self.file = Some(file);
            }
            Err(e) => self.problem = Some(Problem::Broken(e.to_string())),
        }
    }

    fn conflict_key(
        &mut self,
        key: KeyEvent,
        conflicts: Vec<FieldConflict>,
        next: usize,
    ) -> Outcome {
        let keep_file = match key.code {
            KeyCode::Char('f' | 'F') => true,
            KeyCode::Char('y' | 'Y') => false,
            KeyCode::Esc => return Outcome::Stay,
            _ => {
                self.mode = Mode::Conflict { conflicts, next };
                return Outcome::Stay;
            }
        };
        let conflict = &conflicts[next];
        if keep_file && let Some(index) = Field::from_key(&conflict.key).map(Field::index) {
            self.form
                .set_value(index, conflict.file.clone().unwrap_or_default());
        }
        if next + 1 < conflicts.len() {
            self.mode = Mode::Conflict {
                conflicts,
                next: next + 1,
            };
            return Outcome::Stay;
        }
        // Every conflict is settled: take the file as it is now, so the save that follows
        // changes only what the operator chose.
        match self.file.as_mut().map(ConfigFile::reload) {
            Some(Ok(())) => {
                self.message = Some("Conflicts settled; Ctrl-S to save.".to_string());
            }
            Some(Err(e)) => {
                self.problem = Some(Problem::Broken(e.to_string()));
                self.file = None;
            }
            None => {}
        }
        Outcome::Stay
    }

    // ---- drawing --------------------------------------------------------------------------

    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        let inner = Modal::new("Config", 90, 40).render(frame, area);

        let mut banner: Vec<Line> = Vec::new();
        match &self.problem {
            Some(Problem::Broken(why)) => {
                banner.push(Line::styled(
                    format!("The config file does not parse: {why}"),
                    red().add_modifier(Modifier::BOLD),
                ));
                banner.push(Line::styled(
                    "Save is disabled. Fix the file, then press Ctrl-R to reload.",
                    red(),
                ));
            }
            Some(Problem::ReadOnly(why)) => banner.push(Line::styled(
                format!(
                    "Read-only: {why}. Save is disabled; Ctrl-A applies these values for this run only."
                ),
                yellow(),
            )),
            None => {}
        }
        let unknown = self.warnings().len();
        if unknown > 0 {
            let plural = if unknown == 1 { "" } else { "s" };
            banner.push(Line::styled(
                format!("{unknown} unknown key{plural} kept"),
                dim(),
            ));
        }
        let banner_rows = rows_needed(&banner, inner.width);
        let [banner_area, body, footer] = Layout::vertical([
            Constraint::Length(banner_rows),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .areas(inner);
        frame.render_widget(
            Paragraph::new(banner).wrap(Wrap { trim: false }),
            banner_area,
        );

        match &self.mode {
            Mode::Candidates { list, .. } => {
                let [title, rest] =
                    Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(body);
                frame.render_widget(
                    Paragraph::new("Pick a place (Up/Down, Enter selects, Esc cancels)"),
                    title,
                );
                list.render(frame, rest);
            }
            Mode::Conflict { conflicts, next } => {
                self.render_conflicts(frame, body, conflicts, *next)
            }
            Mode::Editing | Mode::ConfirmDiscard => self.render_form(frame, body),
        }
        self.render_footer(frame, footer);
    }

    fn render_form(&self, frame: &mut Frame, area: Rect) {
        let callsign = self.form.value(Field::Callsign.index()).trim();
        let mut lines: Vec<Line> = Vec::new();
        let mut focus_top = 0;
        let mut focus_end = 0;
        let mut cursor = None;
        let heading = |text: &str| {
            Line::styled(
                text.to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            )
        };
        for (index, field) in Field::ALL.iter().copied().enumerate() {
            if let Some(text) = field.heading() {
                lines.push(heading(text));
            }
            let focused = index == self.form.focus();
            let label = format!("  {:<LABEL_WIDTH$}", field.label());
            let value = self.form.value(index);
            let label_style = if focused {
                Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default()
            };
            let mut spans = vec![Span::styled(label.clone(), label_style), Span::raw(value)];
            if field == Field::LogPath && value.is_empty() && !focused {
                spans.push(Span::styled("(blank: default)", dim()));
            }
            if focused {
                focus_top = lines.len();
                cursor = Some((label.chars().count() + value.chars().count(), lines.len()));
            }
            lines.push(Line::from(spans));
            if let Some(error) = self.form.error(index) {
                lines.push(Line::styled(format!("    {error}"), red()));
            }
            if focused && matches!(field, Field::Soft(_)) && !value.trim().is_empty() {
                let shown = if callsign.is_empty() {
                    value.to_string()
                } else {
                    value.replace("{CALL}", callsign)
                };
                lines.push(Line::styled(format!("    sends: {shown}"), dim()));
            }
            if field == Field::Address {
                for text in DISCLOSURE {
                    lines.push(Line::styled(format!("    {text}"), dim()));
                }
                if let Some(note) = &self.note {
                    lines.push(Line::styled(format!("    {note}"), yellow()));
                }
            }
            if focused {
                focus_end = lines.len().saturating_sub(1);
            }
        }
        let height = usize::from(area.height);
        let top = (focus_end + 1).saturating_sub(height).min(focus_top);
        let shown: Vec<Line> = lines.into_iter().skip(top).take(height).collect();
        frame.render_widget(Paragraph::new(shown), area);
        if let Some((x, y)) = cursor
            && y >= top
            && y - top < height
            && x < usize::from(area.width)
        {
            frame.set_cursor_position((area.x + x as u16, area.y + (y - top) as u16));
        }
    }

    fn render_conflicts(
        &self,
        frame: &mut Frame,
        area: Rect,
        conflicts: &[FieldConflict],
        next: usize,
    ) {
        let show = |v: &Option<String>| v.clone().unwrap_or_else(|| "(unset)".to_string());
        let mut lines = vec![
            Line::styled(
                "The file changed on disk and the same setting differs.",
                yellow(),
            ),
            Line::raw("f keeps the file's value, y keeps yours."),
            Line::raw(""),
        ];
        for (i, c) in conflicts.iter().enumerate() {
            let style = if i == next {
                Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                dim()
            };
            lines.push(Line::styled(
                format!(
                    "{}: file {}  |  yours {}",
                    c.key,
                    show(&c.file),
                    show(&c.mine)
                ),
                style,
            ));
        }
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let [top, bottom] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
        if let Some(message) = &self.message {
            frame.render_widget(Paragraph::new(message.as_str()).style(red()), top);
        }
        let (text, style) = match &self.mode {
            Mode::ConfirmDiscard => ("Discard changes? (y/N)", yellow()),
            Mode::Candidates { .. } => ("© OpenStreetMap contributors", dim()),
            Mode::Conflict { .. } => ("f keep file's  y keep mine  Esc back", dim()),
            Mode::Editing => (
                "Ctrl-S save  Tab/Shift-Tab move  Enter on Address looks up  Esc close",
                dim(),
            ),
        };
        frame.render_widget(Paragraph::new(text).style(style), bottom);
    }
}
