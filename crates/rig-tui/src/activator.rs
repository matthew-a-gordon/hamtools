//! The SOTA and POTA Activator screens: a banner and the Activation tracker, the always-visible
//! Contact form, the CW panel with its Send line and Soft key bar, and a recent-contacts list.
//!
//! One implementation serves both programs; it is registered once as "SOTA" and once as "POTA"
//! and takes the Activator Role of the program. It does no I/O: the host keeps the form, the
//! plan and the Log, and passes what the screen shows through [`ActivityCtx`]. Logging never
//! waits on a plan: with no active plan the banner says so and Contacts carry no reference.

use std::time::Instant;

use logbook::band_for_hz;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::activity::{Activity, ActivityCtx, ActivityOutput, FormRoute};
use crate::general::General;
use crate::keys::{self, Action, Key, KeyEntry, Scope};
use crate::panel::{LogDefaults, LogMode, Role};
use crate::rig::{Command, RigMode};
use crate::style::{dim, yellow};
use crate::tracker::Program;
use crate::ui::format_hz;

/// Where the keyboard is: the Contact form (printable keys type into it) or the rig strip (the
/// shared strip keys tune). The CW Send line is the panel's own focus on top of either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Form,
    Strip,
}

pub struct Activator {
    program: Program,
    /// The CW panel and its per-rig state, shared with General.
    inner: General,
    focus: Focus,
    role: Role,
    /// My references of the active plan, when it is this program's.
    my_refs: Vec<String>,
}

/// The Ctrl key that moves to the CW Send line.
const SEND_KEY: char = 't';

/// Rows the Contact form takes (four field rows inside a border).
const FORM_H: u16 = 6;
/// The least the CW panel needs: one transcript row, the Send line and the Soft key bar.
const CW_MIN_H: u16 = 7;
const SIDE_W: u16 = 44;

impl Activator {
    pub fn pota() -> Self {
        Self::new(Program::Pota, Role::PotaActivator)
    }

    pub fn sota() -> Self {
        Self::new(Program::Sota, Role::SotaActivator)
    }

    fn new(program: Program, role: Role) -> Self {
        Self {
            program,
            inner: General::new(),
            focus: Focus::Form,
            role,
            my_refs: Vec::new(),
        }
    }

    fn name_of(&self) -> &'static str {
        self.program.label()
    }

    /// The banner block's lines: the plan, the tracker (or the plans to pick from), then any
    /// warnings (a soft duplicate while a call is typed, the plan's date).
    fn banner_lines(&self, ctx: &ActivityCtx) -> Vec<Line<'static>> {
        let view = ctx.activation;
        let first = match view.and_then(|v| v.banner.as_ref()) {
            Some(b) => Line::raw(b.line()),
            None => Line::styled("no active plan", yellow()),
        };
        let second = match view.and_then(|v| v.tracker.as_ref()) {
            Some(t) => Line::raw(t.row()),
            None if ctx.plans.is_empty() => Line::styled(
                "logging works without a plan; Ctrl-G, Plans makes one",
                dim(),
            ),
            None => {
                let picks: Vec<String> = ctx
                    .plans
                    .iter()
                    .take(9)
                    .enumerate()
                    .map(|(n, c)| format!("Alt-{} {}", n + 1, c.label))
                    .collect();
                Line::styled(format!("pick a plan: {}", picks.join("  ")), dim())
            }
        };
        let mut lines = vec![first, second];
        let call = ctx.form.call();
        let duplicate = view.and_then(|v| v.tracker.as_ref()).and_then(|t| {
            let band = ctx.form.hz().and_then(band_for_hz).map(|b| b.name);
            t.duplicate_warning(&call, band, &ctx.form.mode())
        });
        let warnings: Vec<String> = duplicate
            .into_iter()
            .chain(view.into_iter().flat_map(|v| v.warnings.iter().cloned()))
            .collect();
        if !warnings.is_empty() {
            lines.push(Line::styled(
                format!("! {}", warnings.join("  ! ")),
                yellow(),
            ));
        }
        lines
    }

    fn render_banner(&self, frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
        let title = format!("{} Activator", self.name_of());
        frame.render_widget(
            Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title)),
            area,
        );
    }

    /// The plan picker, shown in place of the recent list's space when no plan of this program
    /// is active. Alt-1..9 launches one.
    fn render_picker(&self, frame: &mut Frame, area: Rect, ctx: &ActivityCtx) {
        let mut lines: Vec<Line> = Vec::new();
        if ctx.plans.is_empty() {
            lines.push(Line::styled(
                "no plans for this program; Ctrl-G, Plans to make one",
                dim(),
            ));
        }
        for (n, choice) in ctx.plans.iter().take(9).enumerate() {
            lines.push(Line::raw(format!("Alt-{} {}", n + 1, choice.label)));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::default().borders(Borders::ALL).title("Pick a plan")),
            area,
        );
    }

    fn render_recent(&self, frame: &mut Frame, area: Rect, ctx: &ActivityCtx) {
        let rows = usize::from(area.height.saturating_sub(2));
        let text = if ctx.recent.is_empty() {
            "none yet".to_string()
        } else {
            ctx.recent
                .iter()
                .take(rows)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        };
        frame.render_widget(
            Paragraph::new(text).style(dim()).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Recent contacts"),
            ),
            area,
        );
    }

    fn render_cw(&self, frame: &mut Frame, area: Rect, ctx: &ActivityCtx) {
        if self.inner.has_panel(ctx) {
            self.inner.render_panel(frame, area, ctx);
        } else {
            let text = match ctx.fresh_mode {
                Some(mode) => {
                    format!("No CW panel for {mode}; log from the form, tune with the strip keys")
                }
                None => "No mode reported yet".to_string(),
            };
            frame.render_widget(
                Paragraph::new(text)
                    .style(dim())
                    .block(Block::default().borders(Borders::ALL).title("CW")),
                area,
            );
        }
    }

    /// One line of the band the receiver is in, with the frequency marked: `20m 14.000 [--|--]
    /// 14.350`. Shown from 30 rows.
    fn render_bandplan(&self, frame: &mut Frame, area: Rect, ctx: &ActivityCtx) {
        let text = match ctx.rx_hz.and_then(|hz| band_for_hz(hz).map(|b| (hz, b))) {
            None => "bandplan: no frequency".to_string(),
            Some((hz, band)) => {
                let width = usize::from(area.width).saturating_sub(40).clamp(10, 60);
                let span = (band.upper_hz - band.lower_hz).max(1);
                let at = ((hz - band.lower_hz) * (width as u64 - 1) / span) as usize;
                let bar: String = (0..width)
                    .map(|i| if i == at { '|' } else { '-' })
                    .collect();
                format!(
                    "{} {} [{bar}] {}  rx {}",
                    band.name,
                    mhz(band.lower_hz),
                    mhz(band.upper_hz),
                    format_hz(hz)
                )
            }
        };
        frame.render_widget(Paragraph::new(text).style(dim()), area);
    }

    fn render_hints(&self, frame: &mut Frame, area: Rect) {
        let text = "Form: Enter saves and reopens on Call, Tab next field, Esc clears a half-typed Contact\n\
                    Strip: Esc from the form; f tune, Left/Right step; Tab or any other key returns\n\
                    CW: F1-F6 soft keys, Ctrl-T Send line; Alt-1..9 picks a plan; Ctrl-G go to";
        frame.render_widget(
            Paragraph::new(text)
                .style(dim())
                .block(Block::default().borders(Borders::ALL).title("Keys")),
            area,
        );
    }
}

fn mhz(hz: u64) -> String {
    format!("{}.{:03}", hz / 1_000_000, hz / 1_000 % 1_000)
}

fn ctrl_or_alt(key: &KeyEvent) -> bool {
    key.modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

impl Activity for Activator {
    fn name(&self) -> &'static str {
        self.name_of()
    }

    fn sending(&self) -> bool {
        self.inner.sending()
    }

    fn switch_rig(&mut self, from: usize, to: usize) {
        self.inner.switch_rig(from, to);
    }

    fn embeds_contact_form(&self) -> bool {
        true
    }

    fn form_key(&mut self, key: KeyEvent, ctx: &ActivityCtx) -> FormRoute {
        // The Send line has the keyboard: the panel gets every key.
        if self.inner.text_focus(ctx) {
            return FormRoute::Pass;
        }
        let plain = !ctrl_or_alt(&key);
        if matches!(key.code, KeyCode::F(_)) {
            return FormRoute::Pass;
        }
        if key.code == KeyCode::Char(SEND_KEY) && key.modifiers.contains(KeyModifiers::CONTROL) {
            // The panel focuses its input line on Tab.
            let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
            self.inner.on_key(tab, ctx);
            return FormRoute::Handled;
        }
        if !plain {
            return FormRoute::Pass;
        }
        match self.focus {
            Focus::Form => {
                if key.code == KeyCode::Esc {
                    if ctx.form.dirty() {
                        FormRoute::Discard
                    } else {
                        self.focus = Focus::Strip;
                        FormRoute::Handled
                    }
                } else {
                    FormRoute::Form
                }
            }
            Focus::Strip => match key.code {
                KeyCode::Tab => {
                    self.focus = Focus::Form;
                    FormRoute::Handled
                }
                KeyCode::Esc => FormRoute::Handled,
                // A strip key tunes; any other printable key starts typing.
                KeyCode::Char(_) => {
                    if keys::lookup(&key, &[Scope::Plain, Scope::Alias]).is_some() {
                        FormRoute::Pass
                    } else {
                        self.focus = Focus::Form;
                        FormRoute::Form
                    }
                }
                _ => FormRoute::Pass,
            },
        }
    }

    fn on_key(&mut self, key: KeyEvent, ctx: &ActivityCtx) -> Option<ActivityOutput> {
        let was_send = self.inner.text_focus(ctx);
        let out = self.inner.on_key(key, ctx);
        // Leaving the Send line returns to the form.
        if was_send && !self.inner.text_focus(ctx) {
            self.focus = Focus::Form;
        }
        out
    }

    fn on_tick(&mut self, now: Instant, ctx: &ActivityCtx) -> Vec<Command> {
        self.inner.on_tick(now, ctx)
    }

    fn on_reply(&mut self, request: &str, reply: Result<String, String>) -> Result<(), String> {
        self.inner.on_reply(request, reply)
    }

    fn on_mode(&mut self, ctx: &ActivityCtx) {
        self.inner.on_mode(ctx);
    }

    fn text_focus(&self, ctx: &ActivityCtx) -> bool {
        self.focus == Focus::Form || self.inner.text_focus(ctx)
    }

    fn has_panel(&self, ctx: &ActivityCtx) -> bool {
        self.inner.has_panel(ctx)
    }

    fn log_hints(&self, mode: &RigMode) -> (Option<LogMode>, LogDefaults) {
        let (log_mode, mut defaults) = self.inner.log_hints(mode);
        defaults.role = self.role;
        defaults.my_refs.clone_from(&self.my_refs);
        (log_mode, defaults)
    }

    fn set_plan(&mut self, role: Role, my_refs: &[String]) {
        // Only this program's plan gives the screen a reference; any other leaves none.
        self.my_refs = if role == self.role {
            my_refs.to_vec()
        } else {
            Vec::new()
        };
    }

    fn state_line(&self, ctx: &ActivityCtx) -> String {
        match ctx.activation.and_then(|a| a.tracker.as_ref()) {
            Some(t) => t.row(),
            None if self.my_refs.is_empty() => "no active plan".to_string(),
            None => self.my_refs.join("+"),
        }
    }

    fn quit_warnings(&self) -> Vec<String> {
        self.inner.quit_warnings()
    }

    fn keys(&self) -> Vec<KeyEntry> {
        vec![KeyEntry {
            scope: Scope::Chord,
            key: Key::ctrl(SEND_KEY),
            action: Action::Activity("send"),
            label: "CW Send line",
            blocked: None,
        }]
    }

    fn hint(&self, ctx: &ActivityCtx) -> &'static str {
        if self.inner.text_focus(ctx) {
            "Enter send  F1-F6 soft keys  Tab/Esc back to the form\nCtrl-G go to  Ctrl-C quit"
        } else if self.focus == Focus::Form {
            "Enter save  Tab field  Esc strip keys  F1-F6 soft keys\nCtrl-T send line  Ctrl-G go to  Ctrl-K keys  Ctrl-C quit"
        } else {
            "f freq  </> tune  ^/v step  m/M mode  v VFO  s/S split\nTab or any key types  F1-F6 soft keys  Ctrl-C quit"
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, ctx: &ActivityCtx) {
        let (main, side) = if ctx.panes.columns >= 2 {
            let [main, side] =
                Layout::horizontal([Constraint::Min(60), Constraint::Length(SIDE_W)]).areas(area);
            (main, Some(side))
        } else {
            (area, None)
        };
        let banner_lines = self.banner_lines(ctx);
        let banner_h = banner_lines.len() as u16 + 2;
        let hints_h = if ctx.panes.key_hints { 5 } else { 0 };
        let band_h = u16::from(ctx.panes.bandplan_strip);
        let fixed = banner_h + FORM_H + band_h + hints_h;
        let spare = main.height.saturating_sub(fixed + CW_MIN_H);
        // The CW panel grows a little, the recent list takes the rest (beside it at 128 columns).
        let cw_extra = if side.is_some() { spare } else { spare.min(3) };
        let recent_h = if side.is_some() { 0 } else { spare - cw_extra };
        let recent_h = if recent_h >= 3 { recent_h } else { 0 };
        let cw_h = main.height.saturating_sub(fixed + recent_h);
        let [banner, form, band, cw, recent, hints] = Layout::vertical([
            Constraint::Length(banner_h),
            Constraint::Length(FORM_H),
            Constraint::Length(band_h),
            Constraint::Length(cw_h),
            Constraint::Length(recent_h),
            Constraint::Length(hints_h),
        ])
        .areas(main);

        self.render_banner(frame, banner, banner_lines);
        ctx.form.render(
            frame,
            form,
            self.focus == Focus::Form && !self.inner.text_focus(ctx),
        );
        if band_h > 0 {
            self.render_bandplan(frame, band, ctx);
        }
        self.render_cw(frame, cw, ctx);
        let planless = ctx.activation.is_none_or(|a| a.banner.is_none());
        if recent_h > 0 {
            if planless {
                self.render_picker(frame, recent, ctx);
            } else {
                self.render_recent(frame, recent, ctx);
            }
        }
        if hints_h > 0 {
            self.render_hints(frame, hints);
        }
        if let Some(side) = side {
            // The wide screen's second column: the picker while there is no plan, then the list.
            if planless {
                let [top, rest] = Layout::vertical([
                    Constraint::Length(ctx.plans.len().min(9) as u16 + 3),
                    Constraint::Min(0),
                ])
                .areas(side);
                self.render_picker(frame, top, ctx);
                self.render_recent(frame, rest, ctx);
            } else {
                self.render_recent(frame, side, ctx);
            }
        }
    }
}
