//! The Activity host seam, driven through the App with no terminal, rig or network.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use common::*;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use rig_tui::{
    Activity, ActivityCtx, ActivityOutput, App, Command, LogDefaults, LogMode, RigEvent, RigMode,
};

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

/// A trivial Activity that counts its ticks and answers every key with a status.
struct Probe {
    ticks: Rc<Cell<u32>>,
}

impl Activity for Probe {
    fn name(&self) -> &'static str {
        "Probe"
    }
    fn on_key(&mut self, _key: KeyEvent, _ctx: &ActivityCtx) -> Option<ActivityOutput> {
        Some(ActivityOutput {
            commands: Vec::new(),
            status: Some("probe key".to_string()),
        })
    }
    fn on_tick(&mut self, _now: Instant, _ctx: &ActivityCtx) -> Vec<Command> {
        self.ticks.set(self.ticks.get() + 1);
        Vec::new()
    }
    fn on_reply(&mut self, _request: &str, _reply: Result<String, String>) -> Result<(), String> {
        Ok(())
    }
    fn on_mode(&mut self, _ctx: &ActivityCtx) {}
    fn text_focus(&self, _ctx: &ActivityCtx) -> bool {
        false
    }
    fn has_panel(&self, _ctx: &ActivityCtx) -> bool {
        false
    }
    fn log_hints(&self, _mode: &RigMode) -> (Option<LogMode>, LogDefaults) {
        (None, LogDefaults::default())
    }
    fn hint(&self, _ctx: &ActivityCtx) -> &'static str {
        "probe hint"
    }
    fn render(&self, frame: &mut Frame, area: Rect, _ctx: &ActivityCtx) {
        frame.render_widget(Paragraph::new("PROBE BODY"), area);
    }
}

#[test]
fn the_app_starts_in_general() {
    assert_eq!(App::new().activity_name(), "General");
}

#[test]
fn the_shared_top_strip_names_the_activity_and_the_rig_once() {
    let app = connected_app_in(App::new().with_rig_addr("127.0.0.1:4533"), "CW");
    let rows = screen(&app, 80, 24);

    assert!(has(&rows, "General"), "{rows:#?}");
    // One title, one clock: the embedded screen adds neither.
    let count = |needle: &str| rows.iter().filter(|r| r.contains(needle)).count();
    assert_eq!(count("General"), 1, "{rows:#?}");
    assert_eq!(count("--:--:--Z"), 1, "{rows:#?}");
    assert!(!has(&rows, "rig-tui"), "{rows:#?}");
}

#[test]
fn only_the_foreground_activity_is_ticked() {
    let ticks = Rc::new(Cell::new(0));
    let mut app = App::new();
    let probe = app.host_activity(Box::new(Probe {
        ticks: ticks.clone(),
    }));
    let t0 = Instant::now();

    app.tick(t0);
    assert_eq!(ticks.get(), 0, "background Activity must not be ticked");

    app.set_foreground(probe);
    app.tick(t0 + Duration::from_millis(10));
    assert_eq!(ticks.get(), 1);
    assert_eq!(app.activity_name(), "Probe");

    app.set_foreground(0);
    app.tick(t0 + Duration::from_millis(20));
    assert_eq!(ticks.get(), 1);
}

#[test]
fn the_foreground_activity_fills_the_body_and_gets_the_keys() {
    let mut app = App::new();
    let probe = app.host_activity(Box::new(Probe {
        ticks: Rc::new(Cell::new(0)),
    }));
    app.set_foreground(probe);

    assert_eq!(app.on_key(key(KeyCode::Char('x'))), vec![]);

    assert_eq!(app.status(), Some("probe key"));
    let rows = screen(&app, 80, 24);
    assert!(has(&rows, "PROBE BODY"), "{rows:#?}");
    assert!(has(&rows, "Probe"), "{rows:#?}");
}

#[test]
fn leaving_an_activity_keeps_its_state() {
    let mut app = cw_app();
    type_str(&mut app, "cq");
    let other = app.host_activity(Box::new(Probe {
        ticks: Rc::new(Cell::new(0)),
    }));

    app.set_foreground(other);
    app.set_foreground(0);

    assert!(has(&screen(&app, 80, 24), "> CQ"));
}

#[test]
fn only_a_cw_send_from_a_key_press_can_reach_the_transmitter() {
    let mut app = cw_app();
    let mut keys: Vec<KeyEvent> = Vec::new();
    for c in ('!'..='~').chain(['é']) {
        for modifiers in [
            KeyModifiers::NONE,
            KeyModifiers::SHIFT,
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
        ] {
            // Ctrl-C quits the program; it never transmits.
            keys.push(KeyEvent::new(KeyCode::Char(c), modifiers));
        }
    }
    for code in [
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Esc,
        KeyCode::Tab,
        KeyCode::Backspace,
        KeyCode::Enter,
    ] {
        keys.push(key(code));
    }
    for n in 1..=12 {
        keys.push(key(KeyCode::F(n)));
    }

    // With empty input and no Soft keys, nothing at all may transmit, in either focus.
    for focus_input in [false, true] {
        let mut a = app;
        if focus_input {
            a.on_key(key(KeyCode::Tab));
        } else {
            a.on_key(key(KeyCode::Esc));
        }
        for k in &keys {
            for command in a.on_key(*k) {
                assert!(!command.transmits(), "{k:?} produced {command:?}");
            }
            // Dismiss anything the key opened and return to a known state.
            a.on_key(key(KeyCode::Esc));
            a.on_key(key(KeyCode::Esc));
        }
        for n in 0..20 {
            for command in a.tick(Instant::now() + Duration::from_secs(n)) {
                assert!(!command.transmits(), "tick produced {command:?}");
            }
        }
        app = a;
    }

    // And the send itself is the one command that transmits, and only from Enter on typed text.
    let mut app = cw_app();
    type_str(&mut app, "cq");
    let sent = app.on_key(key(KeyCode::Enter));
    assert_eq!(sent, vec![Command::Send("CQ".to_string())]);
    assert!(sent[0].transmits());
    for other in [
        Command::Quit,
        Command::Connect,
        Command::PollState,
        Command::PollSMeter,
        Command::SetFreq(7_000_000),
        Command::SetVfo("VFOA".to_string()),
        Command::SetSplitFreq(7_000_000),
        Command::Panel("TB;".to_string()),
    ] {
        assert!(!other.transmits(), "{other:?}");
    }
}

#[test]
fn rig_events_still_reach_the_hosted_activity_state() {
    let mut app = connected_app_in(App::new(), "CW");
    app.on_rig(RigEvent::PanelReply {
        request: "TB;".to_string(),
        reply: Ok("TB006F C E ;".to_string()),
    });
    assert!(has(&screen(&app, 80, 24), "F C E"));
}
