//! The Ctrl-G go-to palette and the Ctrl-K keybindings modal, driven through the App.

mod common;

use std::time::Instant;

use common::*;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use rig_tui::keybindings;
use rig_tui::keys::{Action, Key, KeyEntry, Scope};
use rig_tui::{Activity, ActivityCtx, ActivityOutput, App, Command, LogDefaults, LogMode, RigMode};

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

/// An Activity with its own keys, state line and (optionally) a reason it cannot run.
struct Probe {
    unavailable: Option<&'static str>,
}

fn probe_keys() -> Vec<KeyEntry> {
    vec![
        KeyEntry {
            scope: Scope::Plain,
            key: Key::plain(KeyCode::Char('t')),
            action: Action::Activity("track"),
            label: "tracking",
            blocked: Some("not while idle"),
        },
        KeyEntry {
            scope: Scope::Plain,
            key: Key::plain(KeyCode::Char('[')),
            action: Action::Activity("prev"),
            label: "previous transmitter",
            blocked: None,
        },
    ]
}

impl Activity for Probe {
    fn name(&self) -> &'static str {
        "Probe"
    }
    fn on_key(&mut self, _key: KeyEvent, _ctx: &ActivityCtx) -> Option<ActivityOutput> {
        None
    }
    fn on_tick(&mut self, _now: Instant, _ctx: &ActivityCtx) -> Vec<Command> {
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
    fn keys(&self) -> Vec<KeyEntry> {
        probe_keys()
    }
    fn state_line(&self, _ctx: &ActivityCtx) -> String {
        "next pass in 12 min".to_string()
    }
    fn unavailable(&self) -> Option<String> {
        self.unavailable.map(String::from)
    }
}

fn app_with_probe(unavailable: Option<&'static str>) -> App {
    let mut app = App::new();
    app.host_activity(Box::new(Probe { unavailable }));
    app
}

/// Go to Probe through the palette (the second row).
fn enter_probe(app: &mut App) {
    app.on_key(ctrl('g'));
    app.on_key(key(KeyCode::Down));
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.activity_name(), "Probe");
}

mod palette {
    use super::*;

    #[test]
    fn ctrl_g_lists_every_activity_with_its_live_state() {
        let mut app = connected_app_in(app_with_probe(None), "CW");
        app.on_key(ctrl('g'));
        let rows = screen(&app, 80, 24);

        assert!(has(&rows, "Go to"), "{rows:#?}");
        assert!(has(&rows, "Probe"), "{rows:#?}");
        assert!(has(&rows, "next pass in 12 min"), "{rows:#?}");
        // General's state line is the rig mode.
        assert!(
            rows.iter()
                .any(|r| r.contains("General") && r.contains("CW") && r.contains('│')),
            "{rows:#?}"
        );
    }

    #[test]
    fn typing_filters_the_rows() {
        let mut app = app_with_probe(None);
        app.on_key(ctrl('g'));
        type_str(&mut app, "pro");
        let rows = screen(&app, 80, 24);

        assert!(has(&rows, "> pro"), "{rows:#?}");
        assert!(has(&rows, "next pass in 12 min"), "{rows:#?}");
        // The only General text left is the strip title.
        let general = rows.iter().filter(|r| r.contains("General")).count();
        assert_eq!(general, 1, "{rows:#?}");
    }

    #[test]
    fn enter_switches_activity_and_the_left_one_keeps_its_state() {
        let mut app = focus_input(connected_app_in(app_with_probe(None), "CW"));
        type_str(&mut app, "cq");

        app.on_key(ctrl('g'));
        type_str(&mut app, "probe");
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.activity_name(), "Probe");
        assert!(!app.has_overlay());

        app.on_key(ctrl('g'));
        type_str(&mut app, "gen");
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.activity_name(), "General");
        assert!(has(&screen(&app, 80, 24), "> CQ"));
    }

    #[test]
    fn an_unavailable_activity_shows_its_reason_and_cannot_be_entered() {
        let mut app = app_with_probe(Some("needs FT-847"));
        app.on_key(ctrl('g'));
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "needs FT-847"), "{rows:#?}");

        type_str(&mut app, "probe");
        app.on_key(key(KeyCode::Enter));

        assert_eq!(app.activity_name(), "General");
        assert!(app.has_overlay(), "the palette stays open");
    }

    #[test]
    fn esc_and_ctrl_g_close_it_without_switching() {
        let mut app = app_with_probe(None);
        app.on_key(ctrl('g'));
        app.on_key(key(KeyCode::Esc));
        assert!(!app.has_overlay());
        app.on_key(ctrl('g'));
        app.on_key(ctrl('g'));
        assert!(!app.has_overlay());
        assert_eq!(app.activity_name(), "General");
    }

    #[test]
    fn ctrl_c_still_asks_to_quit_over_it() {
        let mut app = app_with_probe(None);
        app.on_key(ctrl('g'));
        app.on_key(ctrl('c'));
        assert!(has(&screen(&app, 80, 24), "Quit rig-tui?"));
    }
}

mod keybindings_modal {
    use super::*;

    fn open(app: &mut App) {
        app.on_key(ctrl('k'));
    }

    #[test]
    fn it_shows_three_groups_with_the_chord_before_its_alias() {
        let mut app = App::new();
        open(&mut app);
        let rows = screen(&app, 80, 24);

        assert!(has(&rows, "Keybindings"), "{rows:#?}");
        assert!(has(&rows, "General"), "{rows:#?}");
        assert!(has(&rows, "Shared strip keys"), "{rows:#?}");

        let table = keybindings::rows("General", &[], false);
        let pos = |key: &str, action: &str| {
            table
                .iter()
                .position(|r| r.key == key && r.action == action)
                .unwrap()
        };
        assert!(pos("Ctrl-L", "log form") < pos("l", "log form"));
        assert!(table.iter().any(|r| r.group == "App-level Chords"));
    }

    #[test]
    fn it_fits_80x24_without_wrapping() {
        let mut app = App::new();
        open(&mut app);
        let rows = screen(&app, 80, 24);
        let top = rows.iter().position(|r| r.contains("Keybindings")).unwrap();
        // The box is 78 wide, centred: borders at columns 1 and 78, and nothing spills past.
        for r in &rows[top + 1..top + 21] {
            let c: Vec<char> = r.chars().collect();
            assert_eq!((c[1], c[78]), ('┃', '┃'), "{r}");
        }
        assert!(has(&rows, "Ctrl-G"), "{rows:#?}");
    }

    #[test]
    fn slash_filters_the_rows() {
        let mut app = App::new();
        open(&mut app);
        app.on_key(key(KeyCode::Char('/')));
        type_str(&mut app, "split");
        let rows = screen(&app, 80, 24);

        assert!(has(&rows, "/ split"), "{rows:#?}");
        assert!(has(&rows, "split transmit frequency"), "{rows:#?}");
        assert!(!has(&rows, "next mode"), "{rows:#?}");
    }

    #[test]
    fn blocked_keys_show_their_reason() {
        let mut app = app_with_probe(None);
        enter_probe(&mut app);
        open(&mut app);
        app.on_key(key(KeyCode::Char('/')));
        type_str(&mut app, "quit");
        let rows = screen(&app, 80, 24);
        assert!(
            has(&rows, "q quits only in General; use Ctrl-C"),
            "{rows:#?}"
        );

        let mut app = App::new();
        open(&mut app);
        app.on_key(key(KeyCode::Char('/')));
        type_str(&mut app, "quit");
        assert!(!has(&screen(&app, 80, 24), "q quits only in General"));
    }

    #[test]
    fn plain_keys_are_greyed_while_a_text_area_has_focus() {
        let table = keybindings::rows("General", &[], true);
        let split = table.iter().find(|r| r.action == "split").unwrap();
        assert_eq!(split.blocked.as_deref(), Some("text area has focus"));
        let chord = table.iter().find(|r| r.key == "Ctrl-G").unwrap();
        assert_eq!(chord.blocked, None);
        let table = keybindings::rows("General", &[], false);
        let split = table.iter().find(|r| r.action == "split").unwrap();
        assert_eq!(split.blocked, None);
    }

    #[test]
    fn every_key_of_each_activity_table_appears() {
        let table = keybindings::rows("Probe", &probe_keys(), false);
        for entry in probe_keys() {
            let name = keybindings::key_name(&entry.key);
            assert!(
                table
                    .iter()
                    .any(|r| r.key == name && r.action == entry.label),
                "{name} missing"
            );
        }
        let t = table.iter().find(|r| r.key == "t").unwrap();
        assert_eq!(t.blocked.as_deref(), Some("not while idle"));
    }

    #[test]
    fn every_shared_key_appears() {
        let table = keybindings::rows("General", &[], false);
        for entry in rig_tui::keys::shared() {
            let name = keybindings::key_name(&entry.key);
            let shown = table.iter().any(|r| {
                r.key == name
                    || (r.key == "0-9" && matches!(entry.action, Action::FreqDigit(_)))
                    || (r.key == "F1-F6" && matches!(entry.action, Action::SoftKey(_)))
            });
            assert!(shown, "{name} ({}) missing", entry.label);
        }
    }

    #[test]
    fn the_foreground_activity_keys_are_in_the_open_modal() {
        let mut app = app_with_probe(None);
        enter_probe(&mut app);
        open(&mut app);
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "previous transmitter"), "{rows:#?}");
        assert!(has(&rows, "not while idle"), "{rows:#?}");
    }

    #[test]
    fn scrolling_reaches_the_chords_and_esc_closes() {
        let mut app = App::new();
        open(&mut app);
        for _ in 0..40 {
            app.on_key(key(KeyCode::Down));
        }
        assert!(has(&screen(&app, 80, 24), "session list"));
        app.on_key(key(KeyCode::Esc));
        assert!(!app.has_overlay());
    }
}
