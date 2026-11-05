//! The export preview modal: what would be written, before anything is.
//!
//! Opened from the plan manager with `E`. It shows file names, counts, duplicates, exclusions,
//! warnings and the first lines of each file; `d` toggles other dates; Enter writes into the
//! export directory (asking first when a file exists); `?` lists what is unverified about the
//! uploaders. It never uploads and never touches the Log or the plan.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hamtools_config::Plan;
use hamtools_tui::modal::Modal;
use logbook::Record;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use crate::export::{self, Preview, Request, UNVERIFIED, WriteError};
use crate::refdata::parse_locations;
use crate::style::{dim, red, yellow};

enum Stage {
    Preview,
    ConfirmOverwrite(Vec<String>),
    Help,
}

pub(crate) struct ExportModal {
    plan: Plan,
    operator: String,
    records: Vec<Record>,
    /// Each plan reference's cached location, resolved when the modal opened.
    locations: BTreeMap<String, String>,
    dir: Option<PathBuf>,
    other_dates: bool,
    /// The states of the plan's multi-state parks, the primary park's first. Empty when no
    /// park spans more than one, so nothing is asked.
    states: Vec<String>,
    /// Which of `states` is the `MY_STATE` choice.
    state_idx: usize,
    preview: Preview,
    stage: Stage,
    scroll: usize,
    /// The last result of a write, or why none could be made.
    notice: Option<(String, bool)>,
}

/// The states to choose from, deduplicated, and the index of the default. The pool is the
/// states of every park spanning more than one. The default is the plan's primary park's
/// state (its first reference): its first state when it spans several, its only state when
/// that is in the pool; otherwise the pool's first. A park's cached or snapshot location is
/// `US-IN` or `US-IN,US-MI`.
fn multi_state_choices(plan: &Plan, locations: &BTreeMap<String, String>) -> (Vec<String>, usize) {
    let states_of = |r: &str| {
        locations
            .get(&r.to_ascii_uppercase())
            .map(|loc| parse_locations(loc))
            .unwrap_or_default()
    };
    let mut out: Vec<String> = Vec::new();
    for r in &plan.references {
        let states = states_of(r);
        if states.len() > 1 {
            for s in states {
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
    }
    let primary = plan.references.first().map(|r| states_of(r));
    let default = primary
        .and_then(|s| s.first().cloned())
        .and_then(|s| out.iter().position(|o| *o == s))
        .unwrap_or(0);
    (out, default)
}

impl ExportModal {
    pub(crate) fn new(
        plan: Plan,
        operator: String,
        records: Vec<Record>,
        locations: BTreeMap<String, String>,
        dir: Option<PathBuf>,
    ) -> Self {
        let (states, state_idx) = multi_state_choices(&plan, &locations);
        let mut modal = Self {
            plan,
            operator,
            records,
            locations,
            dir,
            other_dates: false,
            states,
            state_idx,
            preview: Preview::default(),
            stage: Stage::Preview,
            scroll: 0,
            notice: None,
        };
        modal.rebuild();
        modal
    }

    fn rebuild(&mut self) {
        let locations = &self.locations;
        let lookup = |r: &str| locations.get(r).cloned();
        self.preview = export::build(&Request {
            plan: &self.plan,
            operator: &self.operator,
            records: &self.records,
            include_other_dates: self.other_dates,
            location_of: &lookup,
            my_state: self.states.get(self.state_idx).map(String::as_str),
        });
        self.scroll = 0;
    }

    /// Returns `true` to close.
    pub(crate) fn on_key(&mut self, key: KeyEvent) -> bool {
        match &self.stage {
            Stage::ConfirmOverwrite(_) => {
                if key.code == KeyCode::Char('y') {
                    self.write(true);
                } else {
                    self.notice = Some(("Nothing written".into(), false));
                }
                self.stage = Stage::Preview;
                false
            }
            Stage::Help => {
                self.stage = Stage::Preview;
                false
            }
            Stage::Preview => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => true,
                KeyCode::Up | KeyCode::Char('k') => {
                    self.scroll = self.scroll.saturating_sub(1);
                    false
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.scroll += 1;
                    false
                }
                KeyCode::PageUp => {
                    self.scroll = self.scroll.saturating_sub(8);
                    false
                }
                KeyCode::PageDown => {
                    self.scroll += 8;
                    false
                }
                KeyCode::Char('d') => {
                    self.other_dates = !self.other_dates;
                    self.notice = None;
                    self.rebuild();
                    false
                }
                KeyCode::Char('s') if !self.states.is_empty() => {
                    self.state_idx = (self.state_idx + 1) % self.states.len();
                    self.notice = None;
                    self.rebuild();
                    false
                }
                KeyCode::Char('?') => {
                    self.stage = Stage::Help;
                    false
                }
                KeyCode::Enter => {
                    self.try_write();
                    false
                }
                _ => false,
            },
        }
    }

    fn try_write(&mut self) {
        let Some(dir) = &self.dir else {
            self.notice = Some(("No export directory: set [export] dir".into(), true));
            return;
        };
        if self.preview.files.is_empty() {
            self.notice = Some(("No file to write".into(), true));
            return;
        }
        let clash = export::existing(dir, &self.preview);
        if clash.is_empty() {
            self.write(false);
        } else {
            self.stage = Stage::ConfirmOverwrite(clash);
        }
    }

    fn write(&mut self, overwrite: bool) {
        let Some(dir) = &self.dir else { return };
        self.notice = Some(match export::write(dir, &self.preview, overwrite) {
            Ok(paths) => (
                format!("Wrote {} file(s) to {}", paths.len(), dir.display()),
                false,
            ),
            Err(WriteError::Exists(names)) => {
                (format!("Exists, not written: {}", names.join(", ")), true)
            }
            Err(e) => (e.to_string(), true),
        });
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let p = &self.preview;
        let mut out: Vec<Line> = Vec::new();
        out.push(Line::raw(format!(
            "{} {}  UTC date {}{}",
            crate::plans::activity_label(self.plan.activity),
            crate::plans::refs_label(&self.plan),
            self.plan.date,
            if self.other_dates {
                "  (other dates included)"
            } else {
                ""
            }
        )));
        out.push(Line::raw(format!(
            "{} Contact(s) selected, {} file(s), {} duplicate(s), {} left out, {} without a reference",
            p.selected,
            p.files.len(),
            p.duplicates.len(),
            p.excluded.len(),
            p.no_reference.len()
        )));
        match &self.dir {
            Some(d) => out.push(Line::styled(format!("Directory: {}", d.display()), dim())),
            None => out.push(Line::styled("Directory: none (set [export] dir)", red())),
        }
        if let Some(state) = self.states.get(self.state_idx) {
            out.push(Line::raw(format!("State: {state}  (s changes MY_STATE)")));
        }
        for f in &p.files {
            out.push(Line::raw(String::new()));
            out.push(Line::raw(format!(
                "File {}  ({} Contact(s))",
                f.name, f.contacts
            )));
            for h in &f.head {
                out.push(Line::styled(format!("  {h}"), dim()));
            }
        }
        let mut section = |title: &str, items: &[String], mark: &str, warn: bool| {
            if items.is_empty() {
                return;
            }
            out.push(Line::raw(String::new()));
            out.push(Line::raw(title.to_string()));
            for i in items {
                let text = format!("  {mark} {i}");
                out.push(if warn {
                    Line::styled(text, yellow())
                } else {
                    Line::raw(text)
                });
            }
        };
        section("Left out", &p.excluded, "-", false);
        section("Not exported, no reference", &p.no_reference, "-", false);
        section("Duplicates (still written)", &p.duplicates, "=", false);
        section("Warnings (none blocks)", &p.warnings, "!", true);
        out
    }

    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        let height = area.height.saturating_sub(2).clamp(10, 28);
        let inner = Modal::new("Export", 90, height).render(frame, area);
        let foot_h = 2.min(inner.height);
        let body = Rect::new(inner.x, inner.y, inner.width, inner.height - foot_h);
        let foot = Rect::new(inner.x, inner.y + body.height, inner.width, foot_h);
        match &self.stage {
            Stage::Help => {
                let mut lines = vec![Line::styled(
                    "Unverified, check on the first real upload:",
                    yellow(),
                )];
                lines.extend(UNVERIFIED.iter().map(|u| Line::raw(format!("  ? {u}"))));
                lines.push(Line::raw(String::new()));
                lines.push(Line::styled(
                    "Files go only to the export directory; nothing is uploaded.",
                    dim(),
                ));
                frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), body);
                frame.render_widget(Paragraph::new(Line::styled("Any key returns", dim())), foot);
            }
            _ => {
                let lines = self.lines();
                let max = lines.len().saturating_sub(usize::from(body.height));
                let skip = self.scroll.min(max);
                let shown: Vec<Line> = lines.into_iter().skip(skip).collect();
                frame.render_widget(Paragraph::new(shown), body);
                let first = match (&self.stage, &self.notice) {
                    (Stage::ConfirmOverwrite(names), _) => Line::styled(
                        format!(
                            "Overwrite {}? y overwrites, any other key cancels",
                            names.join(", ")
                        ),
                        yellow(),
                    ),
                    (_, Some((text, true))) => Line::styled(text.clone(), red()),
                    (_, Some((text, false))) => Line::raw(text.clone()),
                    _ => Line::raw(String::new()),
                };
                let mut parts = vec!["Enter write"];
                if !self.states.is_empty() {
                    parts.push("s state");
                }
                parts.extend([
                    "d other dates",
                    "? unverified",
                    "Up/Down scroll",
                    "Esc close",
                ]);
                let keys = Line::styled(parts.join("   "), dim());
                frame.render_widget(Paragraph::new(vec![first, keys]), foot);
            }
        }
    }
}
