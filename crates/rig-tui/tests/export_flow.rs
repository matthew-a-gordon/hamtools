//! Export through the App: `E` in the plan manager opens a preview, Enter writes into the export
//! directory only, an existing file asks first, and neither the Log nor the plan changes.

mod common;

use std::path::PathBuf;

use common::*;
use ratatui::crossterm::event::KeyCode;
use rig_tui::App;

const PLANS: &str = r#"
[plan.pota-us-1234]
activity = "pota"
references = ["US-1234"]
date = "2026-11-05"
[plan.pota-us-1234.snapshot]
reference = "US-1234"
name = "Test Park"
location = "US-IN"
"#;

const LOG: &str = "test\n<EOH>\n\
<CALL:5>W1ABC<QSO_DATE:8>20261105<TIME_ON:6>150000<BAND:3>40m<MODE:2>CW<MY_SIG:4>POTA<MY_SIG_INFO:7>US-1234<EOR>\n\
<CALL:5>K1XYZ<QSO_DATE:8>20261105<TIME_ON:6>151000<BAND:3>40m<MODE:2>CW<EOR>\n";

struct Fx {
    dir: PathBuf,
}

impl Fx {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("hamtools-exportflow-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("plans.toml"), PLANS).unwrap();
        std::fs::write(dir.join("log.adi"), LOG).unwrap();
        Self { dir }
    }

    fn app(&self) -> App {
        App::with_callsign(Some("K9TST".into()))
            .with_config_path(Some(self.dir.join("config.toml")))
            .with_log_path(Some(self.dir.join("log.adi")))
            .with_export_dir(Some(self.dir.join("out")))
    }

    fn out(&self) -> PathBuf {
        self.dir.join("out").join("K9TST@US-1234-20261105.adi")
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Width and height of the test screen.
const SCREEN: (u16, u16) = (100, 30);

fn view(app: &App) -> Vec<String> {
    screen(app, SCREEN.0, SCREEN.1)
}

/// `src` with `from` replaced by `to`, failing the test if `from` is not there.
fn replaced(src: &str, from: &str, to: &str) -> String {
    assert!(src.contains(from), "fixture text not found: {from}");
    src.replace(from, to)
}

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

fn open_preview(app: &mut App) {
    app.on_key(ctrl('g'));
    type_str(app, "plans");
    app.on_key(key(KeyCode::Enter));
    app.on_key(key(KeyCode::Char('E')));
}

#[test]
fn chords_stay_blocked_over_the_export_modal() {
    let fx = Fx::new("blocked");
    let mut app = fx.app();
    open_preview(&mut app);
    assert!(has(&view(&app), "K9TST@US-1234-20261105.adi"));
    for c in ['o', 'r', 'g', 'k', 'j'] {
        app.on_key(ctrl(c));
        let rows = view(&app);
        assert!(
            has(&rows, "K9TST@US-1234-20261105.adi"),
            "Ctrl-{c} closed the export modal"
        );
        assert!(!has(&rows, "Config") && !has(&rows, "Rigs"), "Ctrl-{c}");
    }
}

#[test]
fn the_preview_shows_files_exclusions_and_help_before_anything_is_written() {
    let fx = Fx::new("preview");
    let mut app = fx.app();
    open_preview(&mut app);
    let rows = view(&app);
    assert!(has(&rows, "K9TST@US-1234-20261105.adi"), "{rows:#?}");
    assert!(has(&rows, "1 Contact(s) selected"), "{rows:#?}");
    assert!(
        has(&rows, "K1XYZ"),
        "the contact with no reference is named: {rows:#?}"
    );
    assert!(!fx.out().exists(), "opening the preview writes nothing");
    app.on_key(key(KeyCode::Char('?')));
    assert!(has(&view(&app), "multi-state"));
}

#[test]
fn enter_writes_and_an_existing_file_asks_before_overwriting() {
    let fx = Fx::new("write");
    let mut app = fx.app();
    open_preview(&mut app);
    app.on_key(key(KeyCode::Enter));
    assert!(has(&view(&app), "Wrote 1 file(s)"));
    let first = std::fs::read_to_string(fx.out()).unwrap();
    assert!(first.contains("<MY_STATE:2>IN"));
    assert!(!first.contains("K1XYZ"));

    std::fs::write(fx.out(), "mine").unwrap();
    app.on_key(key(KeyCode::Enter));
    assert!(has(&view(&app), "Overwrite"));
    app.on_key(key(KeyCode::Char('n')));
    assert_eq!(std::fs::read_to_string(fx.out()).unwrap(), "mine");
    app.on_key(key(KeyCode::Enter));
    app.on_key(key(KeyCode::Char('y')));
    assert_eq!(std::fs::read_to_string(fx.out()).unwrap(), first);
}

#[test]
fn export_changes_neither_the_log_nor_the_plan() {
    let fx = Fx::new("untouched");
    let mut app = fx.app();
    open_preview(&mut app);
    app.on_key(key(KeyCode::Enter));
    app.on_key(key(KeyCode::Esc));
    assert_eq!(
        std::fs::read_to_string(fx.dir.join("log.adi")).unwrap(),
        LOG
    );
    assert_eq!(
        std::fs::read_to_string(fx.dir.join("plans.toml")).unwrap(),
        PLANS
    );
    // Esc closed the preview; the plan manager is still there under it.
    assert!(has(&view(&app), "Activation plans"));
}

/// Two parks (US-1234, US-5678) in the plan, each with the given snapshot location, and one
/// Contact credited to both.
fn two_parks(fx: &Fx, first: &str, second: &str) {
    let plans = replaced(
        PLANS,
        "references = [\"US-1234\"]",
        &format!(
            "references = [\"US-1234\", \"US-5678\"]\n\
             other_snapshots = [{{ reference = \"US-5678\", name = \"Other Park\", location = \"{second}\" }}]"
        ),
    );
    let plans = replaced(
        &plans,
        "location = \"US-IN\"",
        &format!("location = \"{first}\""),
    );
    std::fs::write(fx.dir.join("plans.toml"), plans).unwrap();
    std::fs::write(
        fx.dir.join("log.adi"),
        "test\n<EOH>\n<CALL:5>W1ABC<QSO_DATE:8>20261105<TIME_ON:6>150000<BAND:3>40m<MODE:2>CW<MY_SIG:4>POTA<MY_SIG_INFO:15>US-1234,US-5678<EOR>\n",
    )
    .unwrap();
}

/// A fixture whose plan and Log are as `setup` leaves them, with the export preview open.
fn preview_of(name: &str, setup: impl FnOnce(&Fx)) -> (Fx, App) {
    let fx = Fx::new(name);
    setup(&fx);
    let mut app = fx.app();
    open_preview(&mut app);
    (fx, app)
}

fn multi_state(fx: &Fx) {
    let plans = replaced(PLANS, "location = \"US-IN\"", "location = \"US-IN,US-MI\"");
    std::fs::write(fx.dir.join("plans.toml"), plans).unwrap();
}

#[test]
fn an_nfer_export_takes_every_park_state_from_the_plan_with_no_cache() {
    let (fx, mut app) = preview_of("nfer", |fx| two_parks(fx, "US-IN", "US-MI"));
    app.on_key(key(KeyCode::Enter));
    let mi = std::fs::read_dir(fx.dir.join("out"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .find(|c| c.contains("US-5678"))
        .expect("a file for the second park");
    assert!(mi.contains("<MY_STATE:2>MI"), "{mi}");
}

#[test]
fn a_single_state_primary_park_gives_the_default_when_a_later_park_spans_it() {
    let (_fx, app) = preview_of("ms-primary-single", |fx| {
        two_parks(fx, "US-MI", "US-IN,US-MI")
    });
    let rows = view(&app);
    assert!(has(&rows, "State: US-MI"), "{rows:#?}");
    assert!(has(&rows, "K9TST@US-5678-20261105-US-MI.adi"), "{rows:#?}");
}

#[test]
fn a_single_state_primary_park_outside_the_later_park_falls_back_to_its_first_state() {
    let (_fx, app) = preview_of("ms-primary-outside", |fx| {
        two_parks(fx, "US-OH", "US-IN,US-MI")
    });
    let rows = view(&app);
    assert!(has(&rows, "State: US-IN"), "{rows:#?}");
}

#[test]
fn a_multi_state_primary_park_leads_even_when_a_later_park_lists_other_states_first() {
    let (_fx, app) = preview_of("ms-two-multi", |fx| {
        two_parks(fx, "US-IN,US-MI", "US-OH,US-MI")
    });
    let rows = view(&app);
    assert!(has(&rows, "State: US-IN"), "{rows:#?}");
}

#[test]
fn a_state_chosen_that_another_multi_state_park_lacks_warns_and_leaves_it_unset() {
    let (_fx, mut app) = preview_of("ms-lacks", |fx| two_parks(fx, "US-IN,US-MI", "US-MI,US-OH"));
    // Pool is IN, MI, OH with MI shared; step to OH, which US-1234 lacks.
    app.on_key(key(KeyCode::Char('s')));
    app.on_key(key(KeyCode::Char('s')));
    let rows = view(&app);
    assert!(has(&rows, "State: US-OH"), "{rows:#?}");
    assert!(
        has(&rows, "US-1234: US-OH is not one of its locations"),
        "{rows:#?}"
    );
    // Shared states appear once in the pool: three presses return to the start.
    app.on_key(key(KeyCode::Char('s')));
    assert!(has(&view(&app), "State: US-IN"));
}

#[test]
fn the_keys_footer_keeps_scroll_whether_or_not_states_are_offered() {
    let (_fx, app) = preview_of("footer-single", |_| {});
    let rows = view(&app);
    assert!(has(&rows, "Up/Down scroll"), "{rows:#?}");
    assert!(!has(&rows, "s state"), "{rows:#?}");
    let (_fx, app) = preview_of("footer-multi", multi_state);
    let rows = view(&app);
    assert!(has(&rows, "s state"), "{rows:#?}");
    assert!(has(&rows, "Up/Down scroll"), "{rows:#?}");
}

fn out_files(fx: &Fx) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fx.dir.join("out"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_multi_state_park_defaults_to_its_primary_state() {
    let fx = Fx::new("ms-default");
    multi_state(&fx);
    let mut app = fx.app();
    open_preview(&mut app);
    let rows = view(&app);
    assert!(has(&rows, "K9TST@US-1234-20261105-US-IN.adi"), "{rows:#?}");
    assert!(has(&rows, "State: US-IN"), "{rows:#?}");
    app.on_key(key(KeyCode::Enter));
    assert_eq!(out_files(&fx), ["K9TST@US-1234-20261105-US-IN.adi"]);
}

#[test]
fn s_picks_another_state_and_the_file_gets_it() {
    let fx = Fx::new("ms-pick");
    multi_state(&fx);
    let mut app = fx.app();
    open_preview(&mut app);
    app.on_key(key(KeyCode::Char('s')));
    let rows = view(&app);
    assert!(has(&rows, "State: US-MI"), "{rows:#?}");
    app.on_key(key(KeyCode::Enter));
    let names = out_files(&fx);
    assert_eq!(names, ["K9TST@US-1234-20261105-US-MI.adi"]);
    let body = std::fs::read_to_string(fx.dir.join("out").join(&names[0])).unwrap();
    assert!(body.contains("<MY_STATE:2>MI"), "{body}");
}

#[test]
fn a_single_state_park_asks_nothing() {
    let fx = Fx::new("ss-none");
    let mut app = fx.app();
    open_preview(&mut app);
    let before = view(&app);
    assert!(!has(&before, "State:"), "{before:#?}");
    app.on_key(key(KeyCode::Char('s')));
    assert_eq!(view(&app), before);
}

#[test]
fn with_no_export_directory_the_write_is_refused() {
    let fx = Fx::new("nodir");
    let mut app = App::with_callsign(Some("K9TST".into()))
        .with_config_path(Some(fx.dir.join("config.toml")))
        .with_log_path(Some(fx.dir.join("log.adi")));
    open_preview(&mut app);
    app.on_key(key(KeyCode::Enter));
    assert!(has(&view(&app), "No export directory"));
}
