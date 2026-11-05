//! POTA Hunter: a spot list as the main body, a one-line detail row, the Mode panel and a one-line
//! session count.
//!
//! Spots come from the shared [`ReferenceClient`], which owns every limit; this Activity only asks
//! (on entry, then no faster than the client allows) and reads the latest snapshot. Selecting a
//! spot never tunes: `Enter` asks the host to open the Contact form on the Call and Their ref, and
//! `t` is the one deliberate tune. Filters and sort are session-only.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::DateTime;
use hamtools_config::bandplan::BandPlan;
use logbook::band_for_hz;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::activity::{
    Activity, ActivityCtx, ActivityOutput, HostRequest, LogPrefill, TuneRequest, Worked,
};
use crate::cw::CwPanel;
use crate::keys::{Action, Key, KeyEntry, Scope};
use crate::logging::default_log_mode;
use crate::panel::{LogDefaults, LogMode, ModePanel, PanelCommand, PanelCtx, Role};
use crate::reference::{Feed, Pace, ReferenceClient, SPOTS_EVERY, Submit};
use crate::rig::{Command, RigMode};
use crate::spots::{ModeGroup, Spot, distance_km, mode_group, parse_spots, rig_mode_for};
use crate::style::dim;

/// The cache key of the spot feed (shared by both POTA Roles).
pub const SPOTS_KEY: &str = "pota_spots";
/// The public activator spot list. Its shape is taken from a third-party client.
pub const SPOTS_URL: &str = "https://api.pota.app/spot/activator";
/// A spot older than this is dimmed (and marked), and dropped at its expiry.
pub const STALE_AFTER_SECS: u64 = 120;
const SNAPSHOT_EVERY: Duration = Duration::from_millis(500);
/// Rows of the body the Mode panel takes when there is room for it.
const PANEL_HEIGHT: u16 = 7;
const PANEL_MIN_BODY: u16 = 16;

/// The one feed both POTA Roles read.
pub fn spots_feed() -> Feed {
    Feed::new(SPOTS_KEY, SPOTS_URL, Pace::Spots)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Newest,
    Nearest,
}

#[derive(Default)]
struct Filters {
    band: Option<&'static str>,
    group: Option<ModeGroup>,
    hide_worked: bool,
    /// `/`: every word must appear in the call, reference, park, location or comment.
    text: String,
}

/// One visible spot with everything the list derives for it.
struct Row<'a> {
    spot: &'a Spot,
    age: u64,
    distance_km: Option<f64>,
    band: Option<&'static str>,
    worked: bool,
    stale: bool,
    park_to_park: bool,
    outside: bool,
}

pub struct PotaHunter {
    client: Arc<ReferenceClient>,
    online: bool,
    spots: Vec<Spot>,
    parsed_at: Option<u64>,
    parse_error: Option<String>,
    last_now: Option<Instant>,
    next_try: Option<Instant>,
    last_check: Option<Instant>,
    filters: Filters,
    sort: Sort,
    selected: Option<String>,
    /// My references, when I am activating too: a spot in another park is then a park-to-park
    /// candidate.
    my_refs: Vec<String>,
    request: Option<HostRequest>,
    panels: Vec<Box<dyn ModePanel>>,
    panel_focus: bool,
    /// The `/` search line has the keyboard.
    searching: bool,
}

impl PotaHunter {
    /// `online` false never asks the network (the cached list, if any, is still shown).
    pub fn new(client: Arc<ReferenceClient>, online: bool) -> Self {
        Self {
            client,
            online,
            spots: Vec::new(),
            parsed_at: None,
            parse_error: None,
            last_now: None,
            next_try: None,
            last_check: None,
            filters: Filters::default(),
            sort: Sort::Newest,
            selected: None,
            my_refs: Vec::new(),
            request: None,
            panels: vec![Box::new(CwPanel::new())],
            panel_focus: false,
            searching: false,
        }
    }

    /// The references of the Activation in progress, if any (marks park-to-park candidates).
    pub fn set_my_refs(&mut self, refs: Vec<String>) {
        self.my_refs = refs;
    }

    fn active_panel(&self, ctx: &ActivityCtx) -> Option<usize> {
        let mode = ctx.rig_mode?;
        self.panels.iter().position(|p| p.applies(mode))
    }

    fn panel_ctx<'a>(&self, ctx: &ActivityCtx<'a>, focused: bool) -> PanelCtx<'a> {
        PanelCtx {
            connected: ctx.connected,
            live: ctx.live,
            soft_keys: ctx.soft_keys,
            focused,
        }
    }

    fn refresh_snapshot(&mut self) {
        let Some(snap) = self.client.snapshot(SPOTS_KEY) else {
            return;
        };
        if self.parsed_at == Some(snap.fetched_at) {
            return;
        }
        self.parsed_at = Some(snap.fetched_at);
        match parse_spots(&snap.body, snap.fetched_at) {
            Ok(spots) => {
                self.spots = spots;
                self.parse_error = None;
            }
            Err(e) => self.parse_error = Some(e),
        }
    }

    /// The spots to show: not expired, past the filters, in order.
    fn rows<'a>(&'a self, ctx: &ActivityCtx) -> Vec<Row<'a>> {
        let now = ctx.utc.timestamp();
        let mut rows: Vec<Row> = self
            .live_spots(now)
            .map(|spot| make_row(spot, now, ctx, &self.my_refs))
            .filter(|r| self.filters.band.is_none_or(|b| r.band == Some(b)))
            .filter(|r| {
                self.filters
                    .group
                    .is_none_or(|g| mode_group(&r.spot.mode) == g)
            })
            .filter(|r| !(self.filters.hide_worked && r.worked))
            .filter(|r| self.matches_text(r.spot))
            .collect();
        match self.sort {
            Sort::Newest => {
                rows.sort_by(|a, b| a.age.cmp(&b.age).then(a.spot.call.cmp(&b.spot.call)))
            }
            Sort::Nearest => rows.sort_by(|a, b| {
                let key = |r: &Row| r.distance_km.unwrap_or(f64::MAX);
                key(a)
                    .total_cmp(&key(b))
                    .then(a.age.cmp(&b.age))
                    .then(a.spot.call.cmp(&b.spot.call))
            }),
        }
        rows
    }

    fn matches_text(&self, spot: &Spot) -> bool {
        let hay = format!(
            "{} {} {} {} {}",
            spot.call, spot.reference, spot.park_name, spot.location, spot.comment
        )
        .to_lowercase();
        self.filters
            .text
            .split_whitespace()
            .all(|w| hay.contains(&w.to_lowercase()))
    }

    fn live_spots(&self, now: i64) -> impl Iterator<Item = &Spot> {
        self.spots.iter().filter(move |s| s.expires_at > now)
    }

    /// The bands that have a spot right now, lowest first (what `b` cycles through).
    fn present_bands(&self, now: i64) -> Vec<&'static str> {
        let mut bands: Vec<_> = self
            .live_spots(now)
            .filter_map(|s| band_for_hz(s.hz))
            .collect();
        bands.sort_by_key(|b| b.lower_hz);
        bands.dedup_by_key(|b| b.name);
        bands.into_iter().map(|b| b.name).collect()
    }

    fn selected_index(&self, rows: &[Row]) -> usize {
        self.selected
            .as_ref()
            .and_then(|key| rows.iter().position(|r| &r.spot.key() == key))
            .unwrap_or(0)
    }

    fn selected_spot(&self, ctx: &ActivityCtx) -> Option<Spot> {
        let rows = self.rows(ctx);
        rows.get(self.selected_index(&rows)).map(|r| r.spot.clone())
    }

    fn move_selection(&mut self, ctx: &ActivityCtx, delta: isize) {
        let rows = self.rows(ctx);
        if rows.is_empty() {
            return;
        }
        let at = self.selected_index(&rows) as isize + delta;
        let to = at.clamp(0, rows.len() as isize - 1) as usize;
        self.selected = Some(rows[to].spot.key());
    }

    fn cycle_band(&mut self, ctx: &ActivityCtx) {
        let bands = self.present_bands(ctx.utc.timestamp());
        self.filters.band = match self.filters.band {
            None => bands.first().copied(),
            Some(current) => bands
                .iter()
                .position(|b| *b == current)
                .and_then(|i| bands.get(i + 1).copied()),
        };
    }

    fn cycle_group(&mut self) {
        self.filters.group = match self.filters.group {
            None => Some(ModeGroup::Cw),
            Some(ModeGroup::Cw) => Some(ModeGroup::Phone),
            Some(ModeGroup::Phone) => Some(ModeGroup::Digital),
            Some(ModeGroup::Digital) => None,
        };
    }

    /// `r`: ask now; the client refuses if it is too soon, and says how long to wait.
    fn refresh_now(&mut self) -> String {
        if !self.online {
            return "spots are off (started with --offline)".to_string();
        }
        match self.client.request(&spots_feed()) {
            Submit::Queued => {
                if let Some(now) = self.last_now {
                    self.next_try = Some(now + SPOTS_EVERY);
                }
                "refreshing spots".to_string()
            }
            Submit::TooSoon(wait) => format!("next in {}s", ceil_secs(wait)),
        }
    }

    fn title(&self) -> String {
        let mut title = "POTA Hunter".to_string();
        if let Some(band) = self.filters.band {
            title.push_str(&format!(" [band {band}]"));
        }
        if let Some(group) = self.filters.group {
            title.push_str(&format!(" [mode {}]", group.label()));
        }
        if self.filters.hide_worked {
            title.push_str(" [hide worked]");
        }
        if self.sort == Sort::Nearest {
            title.push_str(" [nearest first]");
        }
        if !self.filters.text.trim().is_empty() {
            title.push_str(&format!(" [search {}]", self.filters.text.trim()));
        }
        title
    }

    /// The right-hand title: the offline indicator, or when the next poll may happen.
    fn feed_state(&self) -> String {
        if !self.online {
            return "offline mode".to_string();
        }
        let status = self.client.status(&spots_feed());
        if status.consecutive_failures > 0 {
            let last = status
                .last_success
                .or(self.parsed_at)
                .and_then(|t| DateTime::from_timestamp(t as i64, 0))
                .map(|t| t.format("%H:%M:%SZ").to_string());
            return match last {
                Some(t) => format!("offline, last ok {t}"),
                None => "offline, no spots yet".to_string(),
            };
        }
        if status.next_in.is_zero() {
            String::new()
        } else {
            format!("next in {}s", ceil_secs(status.next_in))
        }
    }
}

fn ceil_secs(d: Duration) -> u64 {
    d.as_secs() + u64::from(d.subsec_nanos() > 0)
}

fn make_row<'a>(spot: &'a Spot, now: i64, ctx: &ActivityCtx, my_refs: &[String]) -> Row<'a> {
    let band = band_for_hz(spot.hz).map(|b| b.name);
    let age = spot.age_secs(now);
    Row {
        spot,
        age,
        distance_km: ctx
            .observer
            .zip(spot.position)
            .map(|(a, b)| distance_km(a, b)),
        band,
        worked: is_worked(spot, band, ctx.worked),
        stale: age > STALE_AFTER_SECS,
        park_to_park: !my_refs.is_empty() && !my_refs.contains(&spot.reference),
        outside: ctx
            .license_class
            .is_some_and(|class| !BandPlan::embedded().in_privilege(spot.hz, class)),
    }
}

/// Worked today: the same call, band and mode group (POTA counts a repeat only on a new band or
/// mode).
fn is_worked(spot: &Spot, band: Option<&str>, worked: &[Worked]) -> bool {
    let group = mode_group(&spot.mode);
    worked.iter().any(|w| {
        w.call == spot.call && Some(w.band.as_str()) == band && mode_group(&w.mode) == group
    })
}

fn age_text(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        _ => format!("{}h", secs / 3600),
    }
}

fn fit(text: String, width: usize) -> String {
    text.chars().take(width).collect()
}

/// `✓` worked today, `~` stale, `P` park-to-park candidate, `!` outside privileges: a character
/// each, so none depends on color.
fn markers(r: &Row) -> String {
    [
        if r.worked { '✓' } else { ' ' },
        if r.stale { '~' } else { ' ' },
        if r.park_to_park { 'P' } else { ' ' },
        if r.outside { '!' } else { ' ' },
    ]
    .iter()
    .collect()
}

fn header(has_distance: bool, width: usize) -> String {
    let dist = if has_distance { "  dist" } else { "" };
    fit(
        format!(
            "     {:>4} {:<10} {:>10} {:<5} {:<8}{} comment",
            "age", "call", "freq", "mode", "ref", dist
        ),
        width,
    )
}

fn row_text(r: &Row, has_distance: bool, width: usize) -> String {
    let dist = if has_distance {
        match r.distance_km {
            Some(km) => format!(" {:>5}", format!("{km:.0}km")),
            None => format!(" {:>5}", "-"),
        }
    } else {
        String::new()
    };
    fit(
        format!(
            "{} {:>4} {:<10} {:>10} {:<5} {:<8}{} {}",
            markers(r),
            age_text(r.age),
            r.spot.call,
            crate::ui::format_hz(r.spot.hz),
            r.spot.mode,
            r.spot.reference,
            dist,
            r.spot.comment
        ),
        width,
    )
}

impl Activity for PotaHunter {
    fn name(&self) -> &'static str {
        "POTA Hunter"
    }

    fn on_enter(&mut self) {
        // An immediate poll on entry (the client still enforces its own floor).
        self.next_try = None;
        self.last_check = None;
    }

    fn uses_log(&self) -> bool {
        true
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
            plain('j', "next spot"),
            plain('k', "previous spot"),
            plain('d', "nearest first"),
            plain('r', "refresh spots"),
            plain('b', "band filter"),
            plain('o', "mode filter"),
            plain('w', "hide worked"),
            plain('/', "search spots"),
            plain('t', "tune to spot"),
            KeyEntry {
                scope: Scope::Plain,
                key: Key::plain(KeyCode::Enter),
                action: Action::Activity("log spot"),
                label: "log spot",
                blocked: None,
            },
        ]
    }

    fn on_key(&mut self, key: KeyEvent, ctx: &ActivityCtx) -> Option<ActivityOutput> {
        if self.searching {
            match key.code {
                KeyCode::Enter => self.searching = false,
                KeyCode::Esc => {
                    self.searching = false;
                    self.filters.text.clear();
                }
                KeyCode::Backspace => {
                    self.filters.text.pop();
                }
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.filters.text.push(c);
                }
                _ => return None,
            }
            return Some(ActivityOutput::default());
        }
        let panel = self.active_panel(ctx);
        let focused = self.text_focus(ctx);
        if matches!(key.code, KeyCode::Esc | KeyCode::Tab) && focused {
            self.panel_focus = false;
            return Some(ActivityOutput::default());
        }
        if let Some(index) = panel
            && (focused || matches!(key.code, KeyCode::F(1..=6)))
        {
            let pctx = self.panel_ctx(ctx, self.panel_focus);
            let mut out = ActivityOutput::default();
            for command in self.panels[index].handle_key(key, &pctx) {
                match command {
                    PanelCommand::Send(text) => out.commands.push(Command::Send(text)),
                    PanelCommand::Status(text) => out.status = Some(text),
                }
            }
            return Some(out);
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        let mut out = ActivityOutput::default();
        match key.code {
            KeyCode::Tab => {
                let index = panel?;
                if !self.panels[index].captures_text() {
                    return None;
                }
                self.panel_focus = true;
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(ctx, 1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(ctx, -1),
            KeyCode::Char('d') => {
                if self.sort == Sort::Nearest {
                    self.sort = Sort::Newest;
                } else if ctx.observer.is_some() {
                    self.sort = Sort::Nearest;
                } else {
                    out.status = Some("no position: set a Locator to sort by distance".into());
                }
            }
            KeyCode::Char('b') => self.cycle_band(ctx),
            KeyCode::Char('o') => self.cycle_group(),
            KeyCode::Char('w') => self.filters.hide_worked = !self.filters.hide_worked,
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Char('r') => out.status = Some(self.refresh_now()),
            KeyCode::Enter => {
                let spot = self.selected_spot(ctx);
                match spot {
                    Some(spot) => {
                        self.request = Some(HostRequest::OpenLog(LogPrefill {
                            call: spot.call,
                            their_ref: Some(spot.reference),
                        }));
                    }
                    None => out.status = Some("no spot selected".into()),
                }
            }
            KeyCode::Char('t') => {
                let spot = self.selected_spot(ctx);
                match spot {
                    Some(spot) => {
                        self.request = Some(HostRequest::Tune(TuneRequest {
                            hz: spot.hz,
                            mode: rig_mode_for(&spot.mode, spot.hz),
                            mode_name: spot.mode,
                        }));
                    }
                    None => out.status = Some("no spot selected".into()),
                }
            }
            _ => return None,
        }
        Some(out)
    }

    fn on_tick(&mut self, now: Instant, ctx: &ActivityCtx) -> Vec<Command> {
        self.last_now = Some(now);
        if self.online && self.next_try.is_none_or(|t| now >= t) {
            let wait = match self.client.request(&spots_feed()) {
                Submit::Queued => SPOTS_EVERY,
                Submit::TooSoon(wait) => wait.max(Duration::from_secs(1)),
            };
            self.next_try = Some(now + wait);
        }
        if self
            .last_check
            .is_none_or(|t| now.duration_since(t) >= SNAPSHOT_EVERY)
        {
            self.last_check = Some(now);
            self.refresh_snapshot();
        }
        if !ctx.connected {
            return Vec::new();
        }
        match self.active_panel(ctx) {
            Some(index) => self.panels[index]
                .due_requests()
                .into_iter()
                .map(Command::Panel)
                .collect(),
            None => Vec::new(),
        }
    }

    fn on_reply(&mut self, request: &str, reply: Result<String, String>) -> Result<(), String> {
        let mut result = Ok(());
        for panel in &mut self.panels {
            if let Err(why) = panel.on_reply(request, reply.clone()) {
                result = Err(why);
            }
        }
        result
    }

    fn on_mode(&mut self, ctx: &ActivityCtx) {
        if self.active_panel(ctx).is_none() {
            self.panel_focus = false;
        }
    }

    fn text_focus(&self, ctx: &ActivityCtx) -> bool {
        if self.searching {
            return true;
        }
        match self.active_panel(ctx) {
            Some(i) => self.panel_focus && self.panels[i].captures_text(),
            None => false,
        }
    }

    fn has_panel(&self, ctx: &ActivityCtx) -> bool {
        self.active_panel(ctx).is_some()
    }

    fn log_hints(&self, mode: &RigMode) -> (Option<LogMode>, LogDefaults) {
        let (log_mode, mut defaults) = match self.panels.iter().find(|p| p.applies(mode)) {
            Some(panel) => (
                panel.logbook_mode(mode).or_else(|| default_log_mode(mode)),
                panel.log_defaults(),
            ),
            None => (default_log_mode(mode), LogDefaults::default()),
        };
        defaults.role = Role::PotaHunter;
        (log_mode, defaults)
    }

    fn state_line(&self, ctx: &ActivityCtx) -> String {
        format!("{} spots", self.rows(ctx).len())
    }

    fn quit_warnings(&self) -> Vec<String> {
        if self.panels.iter().any(|p| p.has_unsent_input()) {
            vec!["Unsent CW input will be lost.".to_string()]
        } else {
            Vec::new()
        }
    }

    fn hint(&self, ctx: &ActivityCtx) -> &'static str {
        if self.searching {
            "type to filter spots  Enter keep  Esc clear\nCtrl-G go to  Ctrl-C quit"
        } else if self.text_focus(ctx) {
            "Enter send  F1-F6 soft keys  Tab/Esc spots\nCtrl-L log  Ctrl-C quit"
        } else {
            "j/k select  Enter log  t tune  d nearest  b band  o mode  w worked  / search  r refresh\nf freq  </> tune  v VFO  s/S split  l log  L list  c config  Tab input  ^C quit"
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, ctx: &ActivityCtx) {
        let (area, contacts) = crate::general::split_contacts(ctx, area);
        if let Some(pane) = contacts {
            crate::general::render_todays_contacts(ctx, frame, pane);
        }
        let panel = self
            .active_panel(ctx)
            .filter(|_| area.height >= PANEL_MIN_BODY);
        let [list_area, detail_area, panel_area, count_area] = Layout::vertical([
            Constraint::Min(4),
            Constraint::Length(1),
            Constraint::Length(if panel.is_some() { PANEL_HEIGHT } else { 0 }),
            Constraint::Length(1),
        ])
        .areas(area);

        let rows = self.rows(ctx);
        let has_distance = ctx.observer.is_some();
        let selected = self.selected_index(&rows);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.title())
            .title(Line::from(self.feed_state()).right_aligned());
        let inner = block.inner(list_area);
        let width = usize::from(inner.width);
        let mut lines = vec![Line::styled(header(has_distance, width), dim())];
        if rows.is_empty() {
            lines.push(Line::styled(self.empty_notice(ctx), dim()));
        } else {
            let room = usize::from(inner.height.saturating_sub(1)).max(1);
            let first = (selected + 1).saturating_sub(room);
            for (i, row) in rows.iter().enumerate().skip(first).take(room) {
                let style = if i == selected {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else if row.stale {
                    dim()
                } else {
                    Style::default()
                };
                lines.push(Line::styled(row_text(row, has_distance, width), style));
            }
        }
        frame.render_widget(Paragraph::new(lines).block(block), list_area);

        let detail = rows.get(selected).map_or(String::new(), |r| {
            let s = r.spot;
            let mut text = format!("{} {}", s.reference, s.park_name);
            if !s.location.is_empty() {
                text.push_str(&format!(" ({})", s.location));
            }
            if !s.spotter.is_empty() {
                text.push_str(&format!("  spotted by {}", s.spotter));
            }
            text
        });
        frame.render_widget(
            Paragraph::new(Line::from(Span::raw(fit(detail, usize::from(area.width))))),
            detail_area,
        );

        if let Some(index) = panel {
            let pctx = self.panel_ctx(ctx, self.text_focus(ctx));
            self.panels[index].render(frame, panel_area, &pctx);
        }

        let total = self.live_spots(ctx.utc.timestamp()).count();
        frame.render_widget(
            Paragraph::new(format!(
                "{} of {} spots  {} logged",
                rows.len(),
                total,
                ctx.logged
            ))
            .style(dim()),
            count_area,
        );
    }
}

impl PotaHunter {
    fn empty_notice(&self, ctx: &ActivityCtx) -> String {
        let total = self.live_spots(ctx.utc.timestamp()).count();
        if let Some(why) = &self.parse_error {
            format!("The spot list could not be read: {why}")
        } else if total > 0 {
            "No spots match the filters (b band, o mode, w worked)".to_string()
        } else if self.parsed_at.is_none() {
            if self.online {
                "Waiting for spots...".to_string()
            } else {
                "Spots are off and none are cached".to_string()
            }
        } else {
            "No spots right now".to_string()
        }
    }
}
