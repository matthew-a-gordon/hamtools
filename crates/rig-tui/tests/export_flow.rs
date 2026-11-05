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
fn the_preview_shows_files_exclusions_and_help_before_anything_is_written() {
    let fx = Fx::new("preview");
    let mut app = fx.app();
    open_preview(&mut app);
    let rows = screen(&app, 100, 30);
    assert!(has(&rows, "K9TST@US-1234-20261105.adi"), "{rows:#?}");
    assert!(has(&rows, "1 Contact(s) selected"), "{rows:#?}");
    assert!(
        has(&rows, "K1XYZ"),
        "the contact with no reference is named: {rows:#?}"
    );
    assert!(!fx.out().exists(), "opening the preview writes nothing");
    app.on_key(key(KeyCode::Char('?')));
    assert!(has(&screen(&app, 100, 30), "multi-state"));
}

#[test]
fn enter_writes_and_an_existing_file_asks_before_overwriting() {
    let fx = Fx::new("write");
    let mut app = fx.app();
    open_preview(&mut app);
    app.on_key(key(KeyCode::Enter));
    assert!(has(&screen(&app, 100, 30), "Wrote 1 file(s)"));
    let first = std::fs::read_to_string(fx.out()).unwrap();
    assert!(first.contains("<MY_STATE:2>IN"));
    assert!(!first.contains("K1XYZ"));

    std::fs::write(fx.out(), "mine").unwrap();
    app.on_key(key(KeyCode::Enter));
    assert!(has(&screen(&app, 100, 30), "Overwrite"));
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
    assert!(has(&screen(&app, 100, 30), "Activation plans"));
}

#[test]
fn an_nfer_export_takes_every_park_state_from_the_plan_with_no_cache() {
    let fx = Fx::new("nfer");
    let nfer = PLANS
        .replace(
            "references = [\"US-1234\"]",
            "references = [\"US-1234\", \"US-5678\"]\n\
             other_snapshots = [{ reference = \"US-5678\", name = \"Other Park\", location = \"US-MI\" }]",
        );
    std::fs::write(fx.dir.join("plans.toml"), nfer).unwrap();
    std::fs::write(
        fx.dir.join("log.adi"),
        "test\n<EOH>\n<CALL:5>W1ABC<QSO_DATE:8>20261105<TIME_ON:6>150000<BAND:3>40m<MODE:2>CW<MY_SIG:4>POTA<MY_SIG_INFO:15>US-1234,US-5678<EOR>\n",
    )
    .unwrap();
    let mut app = fx.app();
    open_preview(&mut app);
    app.on_key(key(KeyCode::Enter));
    let mi = std::fs::read_dir(fx.dir.join("out"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .find(|c| c.contains("US-5678"))
        .expect("a file for the second park");
    assert!(mi.contains("<MY_STATE:2>MI"), "{mi}");
}

#[test]
fn with_no_export_directory_the_write_is_refused() {
    let fx = Fx::new("nodir");
    let mut app = App::with_callsign(Some("K9TST".into()))
        .with_config_path(Some(fx.dir.join("config.toml")))
        .with_log_path(Some(fx.dir.join("log.adi")));
    open_preview(&mut app);
    app.on_key(key(KeyCode::Enter));
    assert!(has(&screen(&app, 100, 30), "No export directory"));
}
