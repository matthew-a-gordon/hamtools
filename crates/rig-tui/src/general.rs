//! General: today's `rig-tui` body. The base Activity and the startup default.

use std::time::Instant;

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use crate::activity::{Activity, ActivityCtx, ActivityOutput};
use crate::cw::CwPanel;
use crate::logging::default_log_mode;
use crate::panel::{LogDefaults, LogMode, ModePanel, PanelCommand, PanelCtx};
use crate::rig::{Command, RigMode};

pub struct General {
    panels: Vec<Box<dyn ModePanel>>,
    /// The active panel's input line has the keyboard.
    panel_focus: bool,
}

impl General {
    pub fn new() -> Self {
        Self {
            panels: vec![Box::new(CwPanel::new())],
            panel_focus: false,
        }
    }

    fn active_panel(&self, ctx: &ActivityCtx) -> Option<usize> {
        let mode = ctx.rig_mode?;
        self.panels.iter().position(|p| p.applies(mode))
    }

    fn panel_ctx<'a>(&self, ctx: &ActivityCtx<'a>, focused: bool) -> PanelCtx<'a> {
        PanelCtx {
            connected: ctx.connected,
            callsign: ctx.callsign,
            soft_keys: ctx.soft_keys,
            focused,
        }
    }
}

impl Default for General {
    fn default() -> Self {
        Self::new()
    }
}

impl Activity for General {
    fn name(&self) -> &'static str {
        "General"
    }

    fn on_key(&mut self, key: KeyEvent, ctx: &ActivityCtx) -> Option<ActivityOutput> {
        let index = self.active_panel(ctx)?;
        let focused = self.text_focus(ctx);
        if matches!(key.code, KeyCode::Esc | KeyCode::Tab) && focused {
            self.panel_focus = false;
            return Some(ActivityOutput::default());
        }
        if key.code == KeyCode::Tab
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            && self.panels[index].captures_text()
        {
            self.panel_focus = true;
            return Some(ActivityOutput::default());
        }
        if !(matches!(key.code, KeyCode::F(1..=6)) || focused) {
            return None;
        }
        let pctx = self.panel_ctx(ctx, self.panel_focus);
        let mut out = ActivityOutput::default();
        for command in self.panels[index].handle_key(key, &pctx) {
            match command {
                PanelCommand::Send(text) => out.commands.push(Command::Send(text)),
                PanelCommand::Status(text) => out.status = Some(text),
            }
        }
        Some(out)
    }

    fn on_tick(&mut self, _now: Instant, ctx: &ActivityCtx) -> Vec<Command> {
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
        // Panels ignore requests they did not make.
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
        match self.active_panel(ctx) {
            Some(i) => self.panel_focus && self.panels[i].captures_text(),
            None => false,
        }
    }

    fn has_panel(&self, ctx: &ActivityCtx) -> bool {
        self.active_panel(ctx).is_some()
    }

    fn log_hints(&self, mode: &RigMode) -> (Option<LogMode>, LogDefaults) {
        match self.panels.iter().find(|p| p.applies(mode)) {
            Some(panel) => (
                panel.logbook_mode(mode).or_else(|| default_log_mode(mode)),
                panel.log_defaults(),
            ),
            None => (default_log_mode(mode), LogDefaults::default()),
        }
    }

    fn quit_warnings(&self) -> Vec<String> {
        if self.panels.iter().any(|p| p.has_unsent_input()) {
            vec!["Unsent CW input will be lost.".to_string()]
        } else {
            Vec::new()
        }
    }

    fn hint(&self, ctx: &ActivityCtx) -> &'static str {
        if self.text_focus(ctx) {
            "Enter send  F1-F6 soft keys  Tab/Esc strip\nCtrl-L log  Ctrl-C quit"
        } else if self.has_panel(ctx) {
            "f freq  </> tune  ^/v step  m/M mode  v VFO  s/S split\nl log  L list  c config  Tab input  q quit"
        } else {
            "f freq  </> tune  ^/v step  m/M mode  v VFO  s/S split\nl log  L list  c config  q quit"
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, ctx: &ActivityCtx) {
        match self.active_panel(ctx) {
            Some(index) => {
                let pctx = self.panel_ctx(ctx, self.text_focus(ctx));
                self.panels[index].render(frame, area, &pctx);
            }
            None => crate::ui::render_no_panel(ctx, frame, area),
        }
    }
}
