//! The SOTA Chaser screen: a reference box that looks summits up in the offline summit list.
//!
//! There is no spot feed: the SOTA spot API needs the SOTA owner's approval of this software, so
//! the screen says so and makes no SOTA API call. Lookups read the cached summit list only; the
//! one request this screen can make is `r`, an explicit refresh of that downloaded data file
//! through the shared [`ReferenceData`]. `Enter` asks the host to open the Contact form with
//! Their ref prefilled; it never tunes.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::activity::{Activity, ActivityCtx, ActivityOutput, HostRequest, LogPrefill};
use crate::general::General;
use crate::keys::{Action, Key, KeyEntry, Scope};
use crate::panel::{LogDefaults, LogMode, Role};
use crate::refdata::{Hit, Kind, ReferenceData};
use crate::rig::{Command, RigMode};
use crate::style::{dim, yellow};

/// What the screen says in place of a spot list.
pub const NOTICE: &str = "SOTA spots off: needs the SOTA owner's approval";
/// Rows of the body the Mode panel takes when there is room for it.
const PANEL_HEIGHT: u16 = 7;
const PANEL_MIN_BODY: u16 = 16;
const MAX_RESULTS: usize = 200;
/// How often the results are searched again while a query is typed (the cache may have changed).
const RESEARCH_EVERY: Duration = Duration::from_secs(5);

pub struct SotaChaser {
    data: Arc<ReferenceData>,
    online: bool,
    /// The CW panel and its per-rig state, shared with General.
    inner: General,
    /// The reference box has the keyboard.
    boxed: bool,
    query: String,
    results: Vec<Hit>,
    searched_for: Option<String>,
    searched_at: Option<Instant>,
    selected: usize,
    request: Option<HostRequest>,
}

impl SotaChaser {
    /// `online` false never asks the network (the cached list, if any, is still searched).
    pub fn new(data: Arc<ReferenceData>, online: bool) -> Self {
        Self {
            data,
            online,
            inner: General::new(),
            boxed: false,
            query: String::new(),
            results: Vec::new(),
            searched_for: None,
            searched_at: None,
            selected: 0,
            request: None,
        }
    }

    fn research(&mut self, now_unix: u64) {
        self.searched_for = Some(self.query.clone());
        self.results = if self.query.trim().is_empty() {
            Vec::new()
        } else {
            self.data
                .search(&self.query, now_unix, MAX_RESULTS)
                .into_iter()
                .filter(|h| h.entry.kind == Kind::Sota)
                .collect()
        };
        self.selected = self.selected.min(self.results.len().saturating_sub(1));
    }

    fn open_log(&mut self) -> Option<String> {
        let typed = self.query.trim().to_ascii_uppercase();
        let reference = match self.results.get(self.selected) {
            Some(hit) => hit.entry.reference.clone(),
            None if typed.contains('/') => typed,
            None => return Some("type a summit (W4C/CM-001) or part of its name first".into()),
        };
        self.request = Some(HostRequest::OpenLog(LogPrefill {
            call: String::new(),
            their_ref: Some(reference),
        }));
        None
    }

    fn list_state(&self, now_unix: u64) -> String {
        if let Some(why) = self.data.summit_list_error() {
            return why;
        }
        match self.data.summit_list_age(now_unix) {
            None => "summit list not cached".to_string(),
            Some(age) => format!("summit list {}d old", age / 86_400),
        }
    }
}

fn unix(ctx: &ActivityCtx) -> u64 {
    u64::try_from(ctx.utc.timestamp()).unwrap_or(0)
}

fn row_text(hit: &Hit, width: usize) -> String {
    let e = &hit.entry;
    let alt = e.alt_m.map_or(String::new(), |a| format!("{a} m"));
    let pts = e.points.map_or(String::new(), |p| format!("{p} pts"));
    format!("{:<11} {:>7} {:>6}  {}", e.reference, alt, pts, e.name)
        .chars()
        .take(width)
        .collect()
}

impl Activity for SotaChaser {
    fn name(&self) -> &'static str {
        "SOTA Chaser"
    }

    fn sending(&self) -> bool {
        self.inner.sending()
    }

    fn switch_rig(&mut self, from: usize, to: usize) {
        self.inner.switch_rig(from, to);
    }

    fn take_request(&mut self) -> Option<HostRequest> {
        self.request.take()
    }

    fn keys(&self) -> Vec<KeyEntry> {
        let plain = |c: char, label: &'static str| KeyEntry {
            scope: Scope::Plain,
            key: Key::plain(KeyCode::Char(c)),
            action: Action::Activity(label),
            label,
            blocked: None,
        };
        vec![
            plain('/', "search summits"),
            plain('j', "next summit"),
            plain('k', "previous summit"),
            plain('r', "refresh summit list"),
            KeyEntry {
                scope: Scope::Plain,
                key: Key::plain(KeyCode::Enter),
                action: Action::Activity("log summit"),
                label: "log summit",
                blocked: None,
            },
        ]
    }

    fn on_key(&mut self, key: KeyEvent, ctx: &ActivityCtx) -> Option<ActivityOutput> {
        if self.inner.text_focus(ctx) || matches!(key.code, KeyCode::F(1..=6)) {
            return self.inner.on_key(key, ctx);
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        let mut out = ActivityOutput::default();
        if self.boxed {
            match key.code {
                KeyCode::Esc | KeyCode::Tab => self.boxed = false,
                KeyCode::Enter => out.status = self.open_log(),
                KeyCode::Backspace => {
                    self.query.pop();
                }
                KeyCode::Up => self.selected = self.selected.saturating_sub(1),
                KeyCode::Down => {
                    self.selected = (self.selected + 1).min(self.results.len().saturating_sub(1));
                }
                KeyCode::Char(c) => self.query.push(c),
                _ => return None,
            }
            if self.searched_for.as_deref() != Some(self.query.as_str()) {
                self.selected = 0;
                self.research(unix(ctx));
            }
            return Some(out);
        }
        match key.code {
            KeyCode::Char('/') => self.boxed = true,
            KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.results.len().saturating_sub(1));
            }
            KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Enter => out.status = self.open_log(),
            KeyCode::Char('r') => {
                out.status = Some(if self.online {
                    self.data.refresh_summits();
                    "refreshing the summit list (at most once a day)".to_string()
                } else {
                    "offline mode: the summit list is not fetched".to_string()
                });
            }
            KeyCode::Tab => {
                if !self.inner.has_panel(ctx) {
                    return None;
                }
                return self.inner.on_key(key, ctx);
            }
            _ => return None,
        }
        Some(out)
    }

    fn on_tick(&mut self, now: Instant, ctx: &ActivityCtx) -> Vec<Command> {
        let due = self
            .searched_at
            .is_none_or(|t| now.duration_since(t) >= RESEARCH_EVERY);
        if due && !self.query.trim().is_empty() {
            self.searched_at = Some(now);
            self.research(unix(ctx));
        }
        self.inner.on_tick(now, ctx)
    }

    fn on_reply(&mut self, request: &str, reply: Result<String, String>) -> Result<(), String> {
        self.inner.on_reply(request, reply)
    }

    fn on_mode(&mut self, ctx: &ActivityCtx) {
        self.inner.on_mode(ctx);
    }

    fn text_focus(&self, ctx: &ActivityCtx) -> bool {
        self.boxed || self.inner.text_focus(ctx)
    }

    fn has_panel(&self, ctx: &ActivityCtx) -> bool {
        self.inner.has_panel(ctx)
    }

    fn log_hints(&self, mode: &RigMode) -> (Option<LogMode>, LogDefaults) {
        let (log_mode, mut defaults) = self.inner.log_hints(mode);
        defaults.role = Role::SotaChaser;
        (log_mode, defaults)
    }

    fn state_line(&self, _ctx: &ActivityCtx) -> String {
        "reference lookup, spots off".to_string()
    }

    fn quit_warnings(&self) -> Vec<String> {
        self.inner.quit_warnings()
    }

    fn hint(&self, ctx: &ActivityCtx) -> &'static str {
        if self.inner.text_focus(ctx) {
            "Enter send  F1-F6 soft keys  Tab/Esc summits\nCtrl-L log  Ctrl-C quit"
        } else if self.boxed {
            "type a summit or name  Up/Down select  Enter log  Esc/Tab leave\nCtrl-G go to  Ctrl-C quit"
        } else {
            "/ search  j/k select  Enter log  r refresh list  Tab input\nf freq  </> tune  v VFO  s/S split  l log  L list  c config  ^C quit"
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, ctx: &ActivityCtx) {
        let panel = self.inner.has_panel(ctx) && area.height >= PANEL_MIN_BODY;
        let [list_area, detail_area, panel_area, count_area] = Layout::vertical([
            Constraint::Min(4),
            Constraint::Length(1),
            Constraint::Length(if panel { PANEL_HEIGHT } else { 0 }),
            Constraint::Length(1),
        ])
        .areas(area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title("SOTA Chaser")
            .title(Line::from(self.list_state(unix(ctx))).right_aligned());
        let inner = block.inner(list_area);
        let width = usize::from(inner.width);
        let cursor = if self.boxed { "_" } else { "" };
        let mut lines = vec![
            Line::styled(NOTICE, yellow()),
            Line::raw(format!("Summit: {}{cursor}", self.query)),
        ];
        let room = usize::from(inner.height).saturating_sub(lines.len());
        if self.results.is_empty() {
            let text = if self.query.trim().is_empty() {
                "Press / and type a summit reference or part of its name"
            } else if self.query.contains('/') {
                "not cached"
            } else {
                "no summit matches"
            };
            lines.push(Line::styled(text, dim()));
        } else {
            let first = (self.selected + 1).saturating_sub(room.max(1));
            for (i, hit) in self.results.iter().enumerate().skip(first).take(room) {
                let style = if i == self.selected {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else if hit.stale {
                    yellow()
                } else {
                    Style::default()
                };
                lines.push(Line::styled(row_text(hit, width), style));
            }
        }
        frame.render_widget(Paragraph::new(lines).block(block), list_area);

        let detail = self.results.get(self.selected).map_or(String::new(), |h| {
            let e = &h.entry;
            let mut text = format!("{} {}", e.reference, e.name);
            if let Some(l) = &e.location {
                text.push_str(&format!(" ({l})"));
            }
            if h.stale {
                text.push_str("  list older than 30 days");
            }
            text.chars().take(usize::from(area.width)).collect()
        });
        frame.render_widget(Paragraph::new(detail), detail_area);

        if panel {
            self.inner.render_panel(frame, panel_area, ctx);
        }
        frame.render_widget(
            Paragraph::new(format!(
                "{} summits  {} logged",
                self.results.len(),
                ctx.logged
            ))
            .style(dim()),
            count_area,
        );
    }
}
