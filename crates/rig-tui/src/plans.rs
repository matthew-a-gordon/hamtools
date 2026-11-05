//! Activation plans in the app: the stored list, their lifecycle, and the modal that creates,
//! edits, duplicates and deletes them.
//!
//! The file work is the config crate's [`PlansFile`] (field-level writes, conflicts, parse
//! errors). This module adds what only the app knows: which plan is active this session (never
//! stored, so a restart returns it to planned), overdue, the sort order and the snapshot of the
//! plan's reference taken from the reference data.

use std::path::{Path, PathBuf};

use hamtools_config::{Plan, PlanActivity, PlansFile, RefSnapshot, SaveError, valid_date};
use hamtools_tui::form::{Form, FormEvent, TextField};
use hamtools_tui::modal::Modal;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use crate::refdata::{Lookup, RefEntry, ReferenceData};
use crate::softkeys;
use crate::style::{dim, red, yellow};

/// `plans.toml` sits next to the config file.
pub fn plans_path(config_path: &Path) -> PathBuf {
    config_path.with_file_name("plans.toml")
}

/// Where a plan is in its life. Active is held only for the session; overdue is planned with a
/// date that has passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Planned,
    Active,
    Overdue,
    Done,
}

impl Status {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Status::Planned => "planned",
            Status::Active => "active",
            Status::Overdue => "overdue",
            Status::Done => "done",
        }
    }
}

fn snapshot_of(entry: &RefEntry) -> RefSnapshot {
    RefSnapshot {
        reference: entry.reference.clone(),
        name: entry.name.clone(),
        location: entry.location.clone(),
        grid: entry.grid.clone(),
        lat: entry.lat,
        lon: entry.lon,
        alt_m: entry.alt_m,
        points: entry.points,
    }
}

/// The stored plans plus the one active this session.
pub(crate) struct Plans {
    file: Option<PlansFile>,
    /// Why there is no file: no place for it, or it would not parse.
    error: Option<String>,
    active: Option<String>,
}

impl Plans {
    pub(crate) fn none() -> Self {
        Self {
            file: None,
            error: Some("no config directory: plans cannot be stored".to_string()),
            active: None,
        }
    }

    pub(crate) fn load(path: &Path) -> Self {
        match PlansFile::load(path) {
            Ok(file) => Self {
                file: Some(file),
                error: None,
                active: None,
            },
            Err(e) => Self {
                file: None,
                error: Some(e.to_string()),
                active: None,
            },
        }
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn list(&self) -> &[Plan] {
        self.file.as_ref().map_or(&[], PlansFile::plans)
    }

    pub(crate) fn active(&self) -> Option<&Plan> {
        let id = self.active.as_deref()?;
        self.list().iter().find(|p| p.id == id)
    }

    pub(crate) fn set_active(&mut self, id: Option<String>) {
        self.active = id;
    }

    pub(crate) fn status(&self, plan: &Plan, today: &str) -> Status {
        if plan.done {
            Status::Done
        } else if self.active.as_deref() == Some(plan.id.as_str()) {
            Status::Active
        } else if valid_date(today) && plan.date.as_str() < today {
            Status::Overdue
        } else {
            Status::Planned
        }
    }

    /// Not done first by date, done plans last.
    pub(crate) fn sorted(&self) -> Vec<&Plan> {
        let mut v: Vec<&Plan> = self.list().iter().collect();
        v.sort_by(|a, b| (a.done, &a.date, &a.id).cmp(&(b.done, &b.date, &b.id)));
        v
    }

    fn store(&mut self, plans: Vec<Plan>) -> Result<(), String> {
        let Some(file) = &mut self.file else {
            return Err(self.error.clone().unwrap_or_else(|| "no plans file".into()));
        };
        file.save(&plans).map(|_| ()).map_err(|e| match e {
            SaveError::Conflict(c) => {
                let keys: Vec<&str> = c.iter().map(|f| f.key.as_str()).collect();
                format!(
                    "plans.toml changed on disk ({}); r reloads it and drops this edit",
                    keys.join(", ")
                )
            }
            other => other.to_string(),
        })
    }

    /// Add `plan`, or replace the one with its id.
    pub(crate) fn upsert(&mut self, plan: Plan) -> Result<(), String> {
        let mut plans = self.list().to_vec();
        match plans.iter_mut().find(|p| p.id == plan.id) {
            Some(slot) => *slot = plan,
            None => plans.push(plan),
        }
        self.store(plans)
    }

    pub(crate) fn delete(&mut self, id: &str) -> Result<(), String> {
        let plans: Vec<Plan> = self.list().iter().filter(|p| p.id != id).cloned().collect();
        self.store(plans)?;
        if self.active.as_deref() == Some(id) {
            self.active = None;
        }
        Ok(())
    }

    pub(crate) fn set_done(&mut self, id: &str, done: bool) -> Result<(), String> {
        let mut plans = self.list().to_vec();
        if let Some(p) = plans.iter_mut().find(|p| p.id == id) {
            p.done = done;
        }
        self.store(plans)?;
        if done && self.active.as_deref() == Some(id) {
            self.active = None;
        }
        Ok(())
    }

    /// Read the file again, dropping any unsaved edit.
    pub(crate) fn reload(&mut self, path: Option<&Path>) {
        let active = self.active.take();
        *self = path.map_or_else(Plans::none, Plans::load);
        self.active = active;
    }

    /// Whether any plan still has no snapshot of its primary reference.
    pub(crate) fn has_unresolved(&self) -> bool {
        self.list().iter().any(unresolved)
    }

    /// Fill in snapshots from the cache (never fetching) for plans saved offline. Returns whether
    /// anything was stored.
    pub(crate) fn resolve_pending(&mut self, data: &ReferenceData, now_unix: u64) -> bool {
        let mut plans = self.list().to_vec();
        let mut changed = false;
        for p in plans.iter_mut().filter(|p| unresolved(p)) {
            if let Some(primary) = p.primary()
                && let Lookup::Found(hit) = data.lookup(primary, now_unix)
            {
                p.snapshot = Some(snapshot_of(&hit.entry));
                changed = true;
            }
        }
        changed && self.store(plans).is_ok()
    }
}

fn unresolved(plan: &Plan) -> bool {
    plan.snapshot.as_ref().map(|s| s.reference.as_str()) != plan.primary()
}

/// The snapshot to store with `plan`: the cached reference if there is one (asking the client to
/// fetch it when nothing is cached), else the plan's own earlier snapshot of the same reference.
/// `None` is unresolved; the plan saves anyway.
pub(crate) fn snapshot_for(
    plan: &Plan,
    data: Option<&ReferenceData>,
    now_unix: u64,
) -> Option<RefSnapshot> {
    let primary = plan.primary()?;
    if let Some(data) = data
        && let Lookup::Found(hit) = data.lookup_or_fetch(primary, now_unix)
    {
        return Some(snapshot_of(&hit.entry));
    }
    plan.snapshot
        .clone()
        .filter(|s| s.reference.as_str() == primary)
}

/// The POTA location (`US-IN`) of a plan's primary park, from its snapshot or the cache. A SOTA
/// summit or anything that is not a `CC-SS` location gives `None`.
pub(crate) fn state_of(plan: &Plan, data: &ReferenceData, now_unix: u64) -> Option<String> {
    if plan.activity != PlanActivity::Pota {
        return None;
    }
    let primary = plan.primary()?;
    let from_snapshot = plan
        .snapshot
        .as_ref()
        .filter(|s| s.reference == primary)
        .and_then(|s| s.location.clone());
    let location = from_snapshot.or_else(|| match data.lookup(primary, now_unix) {
        Lookup::Found(hit) => hit.entry.location,
        Lookup::NotCached => None,
    })?;
    let first = location.split(',').next()?.trim().to_ascii_uppercase();
    let (country, state) = first.split_once('-')?;
    let ok = |s: &str| (2..=3).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphabetic());
    (ok(country) && ok(state)).then_some(first)
}

/// What to show for a plan's reference: its own snapshot first, then the cache, so a wiped cache
/// changes nothing.
pub(crate) fn display_name(
    plan: &Plan,
    data: Option<&ReferenceData>,
    now_unix: u64,
) -> Option<String> {
    let primary = plan.primary()?;
    if let Some(s) = plan.snapshot.as_ref().filter(|s| s.reference == primary) {
        return Some(s.name.clone());
    }
    match data?.lookup(primary, now_unix) {
        Lookup::Found(hit) => Some(hit.entry.name),
        Lookup::NotCached => None,
    }
}

pub(crate) fn activity_label(activity: PlanActivity) -> &'static str {
    match activity {
        PlanActivity::Sota => "SOTA",
        PlanActivity::Pota => "POTA",
    }
}

/// `US-4571`, or `US-4571 +1` for an n-fer.
pub(crate) fn refs_label(plan: &Plan) -> String {
    let primary = plan.primary().unwrap_or("-");
    match plan.references.len() {
        0 | 1 => primary.to_string(),
        n => format!("{primary} +{}", n - 1),
    }
}

// ---- the modal ----------------------------------------------------------------------------

/// One plan as the modal lists it.
#[derive(Clone)]
pub(crate) struct PlanRow {
    pub plan: Plan,
    pub status: Status,
    /// From the snapshot or the cache; `None` is unresolved.
    pub name: Option<String>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum PlansOutcome {
    Stay,
    Close,
    Save(Box<Plan>),
    Delete(String),
    ToggleDone(String, bool),
    Export(String),
    Reload,
    RefreshParkList,
}

const F_ACTIVITY: usize = 0;
const F_REFS: usize = 1;
const F_DATE: usize = 2;
const F_BANDS: usize = 3;
const F_MODES: usize = 4;
const F_SET: usize = 5;
const F_KEYS: usize = 6;
const F_NOTES: usize = 7;

struct Editing {
    form: Form,
    /// The plan being edited, or the plan a duplicate starts from; carries what the form does not.
    carry: Plan,
    /// `Some` when editing in place, `None` for a new plan or a duplicate.
    id: Option<String>,
}

enum Mode {
    List,
    Edit(Box<Editing>),
    ConfirmDelete(String),
}

pub(crate) struct PlansModal {
    rows: Vec<PlanRow>,
    selected: usize,
    mode: Mode,
    notice: Option<String>,
    /// Why nothing can be stored (no file, unparseable file).
    blocked: Option<String>,
    today: String,
}

fn join(v: &[String], sep: &str) -> String {
    v.join(sep)
}

fn split_list(text: &str) -> Vec<String> {
    text.split([',', ' '])
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect()
}

fn slug(plan: &Plan) -> String {
    let raw = format!(
        "{}-{}-{}",
        plan.activity.as_str(),
        plan.primary().unwrap_or("plan"),
        plan.date.replace('-', "")
    );
    let mut out = String::new();
    for c in raw.to_ascii_lowercase().chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '-' };
        if c != '-' || !out.ends_with('-') {
            out.push(c);
        }
    }
    out.trim_matches('-').to_string()
}

impl PlansModal {
    pub(crate) fn new(rows: Vec<PlanRow>, blocked: Option<String>, today: String) -> Self {
        Self {
            rows,
            selected: 0,
            mode: Mode::List,
            notice: None,
            blocked,
            today,
        }
    }

    /// Replace the list after a change, keeping the selection on the same plan if it is there.
    pub(crate) fn set_rows(&mut self, rows: Vec<PlanRow>, blocked: Option<String>) {
        let id = self.rows.get(self.selected).map(|r| r.plan.id.clone());
        self.rows = rows;
        self.blocked = blocked;
        self.selected = id
            .and_then(|id| self.rows.iter().position(|r| r.plan.id == id))
            .unwrap_or_else(|| self.selected.min(self.rows.len().saturating_sub(1)));
    }

    pub(crate) fn selected_plan(&self) -> Option<&Plan> {
        self.selected_row().map(|r| &r.plan)
    }

    pub(crate) fn set_notice(&mut self, notice: Option<String>) {
        self.notice = notice;
    }

    /// Return to the list (after a successful save or delete).
    pub(crate) fn show_list(&mut self) {
        self.mode = Mode::List;
        self.notice = None;
    }

    /// Select the plan with this id, if listed.
    pub(crate) fn select(&mut self, id: &str) {
        if let Some(i) = self.rows.iter().position(|r| r.plan.id == id) {
            self.selected = i;
        }
    }

    fn blank_plan(&self) -> Plan {
        Plan {
            id: String::new(),
            activity: PlanActivity::Pota,
            references: Vec::new(),
            date: self.today.clone(),
            bands: Vec::new(),
            modes: Vec::new(),
            softkey_set: None,
            softkeys: Default::default(),
            notes: None,
            done: false,
            snapshot: None,
        }
    }

    fn open_form(&mut self, carry: Plan, id: Option<String>) {
        let keys = join(
            &carry
                .softkeys
                .iter()
                .map(|(n, t)| format!("F{n}={t}"))
                .collect::<Vec<_>>(),
            "; ",
        );
        let form = Form::new(vec![
            TextField::new("Activity (SOTA or POTA)")
                .uppercase()
                .with_value(activity_label(carry.activity)),
            TextField::new("References (first is primary)")
                .uppercase()
                .with_value(join(&carry.references, ", ")),
            TextField::new("UTC date (YYYY-MM-DD)").with_value(carry.date.clone()),
            TextField::new("Bands")
                .uppercase()
                .with_value(join(&carry.bands, ", ")),
            TextField::new("Modes")
                .uppercase()
                .with_value(join(&carry.modes, ", ")),
            TextField::new("Soft key set")
                .with_value(carry.softkey_set.clone().unwrap_or_default()),
            TextField::new("Keys (F1=text; F2=text)").with_value(keys),
            TextField::new("Notes").with_value(carry.notes.clone().unwrap_or_default()),
        ]);
        self.notice = None;
        self.mode = Mode::Edit(Box::new(Editing { form, carry, id }));
    }

    fn selected_row(&self) -> Option<&PlanRow> {
        self.rows.get(self.selected)
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> PlansOutcome {
        match &mut self.mode {
            Mode::List => self.on_list_key(key),
            Mode::ConfirmDelete(id) => {
                let id = id.clone();
                match key.code {
                    KeyCode::Char('y') => {
                        self.mode = Mode::List;
                        PlansOutcome::Delete(id)
                    }
                    _ => {
                        self.mode = Mode::List;
                        PlansOutcome::Stay
                    }
                }
            }
            Mode::Edit(_) => self.on_edit_key(key),
        }
    }

    fn on_list_key(&mut self, key: KeyEvent) -> PlansOutcome {
        let last = self.rows.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return PlansOutcome::Close,
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.selected = (self.selected + 1).min(last),
            KeyCode::Char('n') => {
                let blank = self.blank_plan();
                self.open_form(blank, None);
            }
            KeyCode::Char('e') | KeyCode::Enter => {
                if let Some(plan) = self.selected_row().map(|r| r.plan.clone()) {
                    let id = plan.id.clone();
                    self.open_form(plan, Some(id));
                }
            }
            KeyCode::Char('d') => {
                if let Some(mut plan) = self.selected_row().map(|r| r.plan.clone()) {
                    plan.done = false;
                    plan.date = self.today.clone();
                    self.open_form(plan, None);
                }
            }
            KeyCode::Char('x') | KeyCode::Delete => {
                if let Some(id) = self.selected_row().map(|r| r.plan.id.clone()) {
                    self.mode = Mode::ConfirmDelete(id);
                }
            }
            KeyCode::Char('m') => {
                if let Some(r) = self.selected_row() {
                    return PlansOutcome::ToggleDone(r.plan.id.clone(), !r.plan.done);
                }
            }
            KeyCode::Char('r') => return PlansOutcome::Reload,
            KeyCode::Char('R') => return PlansOutcome::RefreshParkList,
            KeyCode::Char('E') => {
                if let Some(id) = self.selected_row().map(|r| r.plan.id.clone()) {
                    return PlansOutcome::Export(id);
                }
            }
            _ => {}
        }
        PlansOutcome::Stay
    }

    fn on_edit_key(&mut self, key: KeyEvent) -> PlansOutcome {
        let ids: Vec<String> = self.rows.iter().map(|r| r.plan.id.clone()).collect();
        let Mode::Edit(editing) = &mut self.mode else {
            return PlansOutcome::Stay;
        };
        match editing.form.handle_key(key) {
            FormEvent::Edited => PlansOutcome::Stay,
            FormEvent::Cancel => {
                self.mode = Mode::List;
                self.notice = None;
                PlansOutcome::Stay
            }
            FormEvent::Submit => match plan_from_form(editing, &ids) {
                Ok(plan) => PlansOutcome::Save(Box::new(plan)),
                Err((field, message)) => {
                    editing.form.set_error(field, message);
                    editing.form.set_focus(field);
                    PlansOutcome::Stay
                }
            },
        }
    }

    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        match &self.mode {
            Mode::Edit(editing) => {
                let inner = Modal::new("Activation plan", 78, 18).render(frame, area);
                let [form, foot] = split_foot(inner, 2);
                editing.form.render(frame, form);
                let mut lines = vec![Line::styled(
                    "Tab next field   Enter save   Esc cancel",
                    dim(),
                )];
                if let Some(n) = &self.notice {
                    lines.insert(0, Line::styled(n.clone(), red()));
                }
                frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), foot);
            }
            _ => self.render_list(frame, area),
        }
    }

    fn render_list(&self, frame: &mut Frame, area: Rect) {
        let height = (self.rows.len() as u16 + 7).clamp(10, 22);
        let inner = Modal::new("Activation plans", 78, height).render(frame, area);
        let [body, foot] = split_foot(inner, 3);
        let mut lines: Vec<Line> = Vec::new();
        if let Some(b) = &self.blocked {
            lines.push(Line::styled(b.clone(), red()));
        }
        if self.rows.is_empty() {
            lines.push(Line::styled("No plans yet. Press n to make one.", dim()));
        }
        for (i, row) in self.rows.iter().enumerate() {
            let p = &row.plan;
            let marker = if i == self.selected { '>' } else { ' ' };
            let name = row.name.as_deref().unwrap_or("unresolved");
            let name: String = name.chars().take(26).collect();
            let text = format!(
                "{marker} {:<4} {:<12} {:<26} {}  {}",
                activity_label(p.activity),
                refs_label(p),
                name,
                p.date,
                row.status.label()
            );
            let mut style = match row.status {
                Status::Done => dim(),
                Status::Overdue => red(),
                Status::Active => Style::default().add_modifier(Modifier::BOLD),
                Status::Planned => Style::default(),
            };
            if i == self.selected {
                style = style.add_modifier(Modifier::REVERSED);
            }
            lines.push(Line::styled(text, style));
        }
        let visible = usize::from(body.height);
        let skip = (self.selected + 1 + lines.len() - self.rows.len()).saturating_sub(visible);
        let lines: Vec<Line> = lines.into_iter().skip(skip).collect();
        frame.render_widget(Paragraph::new(lines), body);

        let mut foot_lines = Vec::new();
        match (&self.mode, &self.notice) {
            (Mode::ConfirmDelete(_), _) => foot_lines.push(Line::styled(
                "Delete this plan? y deletes, any other key keeps it",
                yellow(),
            )),
            (_, Some(n)) => foot_lines.push(Line::styled(n.clone(), red())),
            _ => {}
        }
        foot_lines.push(Line::styled(
            "n new   e edit   d duplicate   x delete   m done/planned   r reload   R parks   E export   Esc close",
            dim(),
        ));
        frame.render_widget(Paragraph::new(foot_lines).wrap(Wrap { trim: true }), foot);
    }
}

fn split_foot(area: Rect, foot: u16) -> [Rect; 2] {
    let foot = foot.min(area.height);
    let body = Rect::new(area.x, area.y, area.width, area.height - foot);
    let foot = Rect::new(area.x, area.y + body.height, area.width, foot);
    [body, foot]
}

/// Read the form into a plan, or the field that is wrong and why.
fn plan_from_form(editing: &Editing, ids: &[String]) -> Result<Plan, (usize, String)> {
    let f = &editing.form;
    let activity =
        PlanActivity::parse(f.value(F_ACTIVITY)).ok_or((F_ACTIVITY, "SOTA or POTA".to_string()))?;
    let references = split_list(f.value(F_REFS))
        .into_iter()
        .map(|r| r.replace(' ', ""))
        .collect::<Vec<_>>();
    if references.is_empty() {
        return Err((F_REFS, "needs at least one reference".into()));
    }
    match activity {
        PlanActivity::Sota => {
            if references.len() != 1 {
                return Err((F_REFS, "a SOTA plan holds exactly one summit".into()));
            }
            if !references[0].contains('/') {
                return Err((F_REFS, "a summit looks like W4C/CM-001".into()));
            }
        }
        PlanActivity::Pota => {
            if let Some(bad) = references
                .iter()
                .find(|r| r.contains('/') || !r.contains('-'))
            {
                return Err((F_REFS, format!("{bad} is not a park such as US-4571")));
            }
        }
    }
    let date = f.value(F_DATE).trim().to_string();
    if !valid_date(&date) {
        return Err((F_DATE, "use YYYY-MM-DD".into()));
    }
    let softkeys = parse_keys(f.value(F_KEYS)).map_err(|m| (F_KEYS, m))?;
    let set = f.value(F_SET).trim();
    let notes = f.value(F_NOTES).trim();
    let mut plan = Plan {
        id: editing.id.clone().unwrap_or_default(),
        activity,
        references,
        date,
        bands: split_list(f.value(F_BANDS)),
        modes: split_list(f.value(F_MODES)),
        softkey_set: (!set.is_empty()).then(|| set.to_string()),
        softkeys,
        notes: (!notes.is_empty()).then(|| notes.to_string()),
        done: editing.carry.done,
        snapshot: editing
            .carry
            .snapshot
            .clone()
            .filter(|s| Some(s.reference.as_str()) == editing.carry.primary()),
    };
    if plan.snapshot.as_ref().map(|s| s.reference.as_str()) != plan.primary() {
        plan.snapshot = None;
    }
    if editing.id.is_none() {
        let base = slug(&plan);
        let mut id = base.clone();
        let mut n = 2;
        while ids.contains(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        plan.id = id;
    }
    Ok(plan)
}

/// `F1=text; F2=text`, each validated by the same validator as the config dialog.
fn parse_keys(text: &str) -> Result<std::collections::BTreeMap<u8, String>, String> {
    let mut out = std::collections::BTreeMap::new();
    for part in text.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let (name, msg) = part
            .split_once('=')
            .ok_or_else(|| format!("{part:?}: write F1=message"))?;
        let n = name
            .trim()
            .to_ascii_uppercase()
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| (1..=softkeys::SLOTS).contains(n))
            .ok_or_else(|| format!("{:?}: only F1 to F6", name.trim()))?;
        let msg = msg.trim().to_uppercase();
        if msg.is_empty() {
            return Err(format!("F{n} has no text"));
        }
        let report = softkeys::validate(&msg);
        if let Some(problem) = report.problems.first() {
            return Err(format!("F{n}: {problem}"));
        }
        out.insert(n, msg);
    }
    Ok(out)
}
