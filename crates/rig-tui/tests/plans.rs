//! Activation plans through the App: the plan manager modal, the palette rows, saving a plan's
//! reference into the cache and a snapshot, and launching. No network: a scripted transport.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use common::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use ratatui::style::Color;
use rig_tui::refdata::ReferenceData;
use rig_tui::reference::*;
use rig_tui::softkeys::Layer;
use rig_tui::{App, render};

const DETAIL: &str = r#"{"reference":"US-4571","name":"Potato Creek State Park","latitude":41.5546,"longitude":-86.3697,"grid6":"EN61uo","locationDesc":"US-IN"}"#;

const DETAIL2: &str = r#"{"reference":"US-4572","name":"Second Creek State Park","latitude":41.6,"longitude":-86.1,"grid6":"EN61vo","locationDesc":"US-MI"}"#;

/// 2027-01-15 08:00 UTC.
const NOW: u64 = 1_800_000_000;

struct FakeClock(AtomicU64);
impl Clock for FakeClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.load(Ordering::SeqCst))
    }
    fn unix(&self) -> u64 {
        NOW + self.0.load(Ordering::SeqCst) / 1000
    }
    fn sleep(&self, d: Duration) {
        self.0.fetch_add(d.as_millis() as u64, Ordering::SeqCst);
    }
}

struct Routes {
    urls: Mutex<Vec<String>>,
    calls: AtomicUsize,
    offline: bool,
}
impl Transport for Routes {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.urls.lock().unwrap().push(req.url.clone());
        let body = if req.url.contains("/park/US-4571") {
            DETAIL
        } else if req.url.contains("/park/US-4572") {
            DETAIL2
        } else {
            return Err("offline".into());
        };
        if self.offline {
            return Err("offline".into());
        }
        Ok(HttpResponse {
            status: 200,
            body: body.as_bytes().to_vec(),
            ..Default::default()
        })
    }
}

struct Fx {
    dir: PathBuf,
    cache: PathBuf,
    client: Arc<ReferenceClient>,
    data: Arc<ReferenceData>,
    routes: Arc<Routes>,
}

impl Fx {
    fn new(name: &str, offline: bool) -> Self {
        let dir =
            std::env::temp_dir().join(format!("hamtools-plans-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let routes = Arc::new(Routes {
            urls: Mutex::new(vec![]),
            calls: AtomicUsize::new(0),
            offline,
        });
        let client = Arc::new(ReferenceClient::start(
            routes.clone(),
            Arc::new(FakeClock(AtomicU64::new(0))),
            Limits::default(),
            user_agent(None),
            cache.clone(),
        ));
        let data = Arc::new(ReferenceData::new(client.clone(), cache.clone()));
        Self {
            dir,
            cache,
            client,
            data,
            routes,
        }
    }

    fn plans_file(&self) -> PathBuf {
        self.dir.join("plans.toml")
    }

    fn write_plans(&self, text: &str) {
        std::fs::write(self.plans_file(), text).unwrap();
    }

    fn stored(&self) -> String {
        std::fs::read_to_string(self.plans_file()).unwrap_or_default()
    }

    /// A fresh App as after a restart, over the same files.
    fn app(&self) -> App {
        let mut sets = BTreeMap::new();
        sets.insert(
            "pota".to_string(),
            BTreeMap::from([
                (1, "CQ POTA DE {MYCALL}".to_string()),
                (2, "POTA 2".to_string()),
            ]),
        );
        let mut app = App::with_callsign(Some("W1AW".into()))
            .with_softkey_sets(sets)
            .with_config_path(Some(self.dir.join("config.toml")))
            .with_reference_data(self.data.clone());
        app.set_utc(SystemTime::UNIX_EPOCH + Duration::from_secs(NOW));
        app
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const PLANS: &str = r#"
[plan.pota-us-4571-20270201]
activity = "pota"
references = ["US-4571"]
date = "2027-02-01"
softkey_set = "pota"
[plan.pota-us-4571-20270201.softkeys]
F1 = "CQ PARK DE {MYCALL}"
[plan.pota-us-4571-20270201.snapshot]
reference = "US-4571"
name = "Potato Creek State Park"

[plan.sota-old]
activity = "sota"
references = ["W4C/CM-001"]
date = "2027-01-10"

[plan.pota-finished]
activity = "pota"
references = ["US-9999"]
date = "2026-12-01"
status = "done"
"#;

fn press(app: &mut App, codes: &[KeyCode]) {
    for c in codes {
        app.on_key(key(*c));
    }
}

fn open_manager(app: &mut App) {
    app.on_key(ctrl('g'));
    type_str(app, "plans");
    press(app, &[KeyCode::Enter]);
    assert!(
        screen(app, 100, 30)
            .iter()
            .any(|r| r.contains("Activation plans")),
        "the manager should be open"
    );
}

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

fn row_with(rows: &[String], needle: &str) -> usize {
    rows.iter()
        .position(|r| r.contains(needle))
        .unwrap_or_else(|| panic!("no row with {needle:?} in {rows:#?}"))
}

/// Fill the new-plan form: references, date, set, keys.
fn new_plan(app: &mut App, refs: &str, date: &str, set: &str, keys: &str) {
    press(app, &[KeyCode::Char('n'), KeyCode::Tab]);
    type_str(app, refs);
    press(app, &[KeyCode::Tab]);
    for _ in 0..10 {
        press(app, &[KeyCode::Backspace]);
    }
    type_str(app, date);
    press(app, &[KeyCode::Tab]);
    type_str(app, "40m");
    press(app, &[KeyCode::Tab]);
    type_str(app, "cw");
    press(app, &[KeyCode::Tab]);
    type_str(app, set);
    press(app, &[KeyCode::Tab]);
    type_str(app, keys);
    press(app, &[KeyCode::Enter]);
}

mod storage {
    use super::*;

    #[test]
    fn a_new_plan_is_stored_and_listed() {
        let fx = Fx::new("create", true);
        let mut app = fx.app();
        open_manager(&mut app);
        assert!(has(&screen(&app, 100, 30), "No plans yet"));
        new_plan(
            &mut app,
            "us-4571",
            "2027-02-01",
            "pota",
            "F1=cq park de {mycall}",
        );

        let text = fx.stored();
        assert!(text.contains("[plan.pota-us-4571-20270201]"), "{text}");
        assert!(text.contains("references = [\"US-4571\"]"), "{text}");
        assert!(text.contains("bands = [\"40M\"]"), "{text}");
        assert!(text.contains("F1 = \"CQ PARK DE {MYCALL}\""), "{text}");
        let rows = screen(&app, 100, 30);
        assert!(has(&rows, "POTA US-4571"), "{rows:#?}");
        assert!(has(&rows, "2027-02-01"), "{rows:#?}");
    }

    #[test]
    fn hand_edits_survive_an_app_save() {
        let fx = Fx::new("handedit", true);
        fx.write_plans(&format!(
            "# my trips\n{PLANS}\n[plan.pota-finished.extra]\nx = 1\n"
        ));
        let mut app = fx.app();
        open_manager(&mut app);
        // The soonest not-done plan is the overdue one; mark it done.
        press(&mut app, &[KeyCode::Char('m')]);
        let text = fx.stored();
        assert!(text.starts_with("# my trips"), "{text}");
        assert!(text.contains("[plan.pota-finished.extra]"), "{text}");
        assert!(text.contains("[plan.sota-old]"), "{text}");
        assert!(text.contains("status = \"done\""), "{text}");
    }

    #[test]
    fn a_form_error_names_the_field_and_saves_nothing() {
        let fx = Fx::new("formerr", true);
        let mut app = fx.app();
        open_manager(&mut app);
        new_plan(&mut app, "us-4571", "2027-13-45", "", "");
        let rows = screen(&app, 100, 30);
        assert!(has(&rows, "use YYYY-MM-DD"), "{rows:#?}");
        assert_eq!(fx.stored(), "");

        // A SOTA plan holds exactly one summit.
        press(&mut app, &[KeyCode::Esc]);
        press(&mut app, &[KeyCode::Char('n')]);
        press(&mut app, &[KeyCode::Backspace; 4]);
        type_str(&mut app, "sota");
        press(&mut app, &[KeyCode::Tab]);
        type_str(&mut app, "W4C/CM-001, W4C/CM-002");
        press(&mut app, &[KeyCode::Enter]);
        assert!(has(&screen(&app, 100, 30), "exactly one summit"));
    }

    #[test]
    fn duplicate_makes_a_new_plan_and_delete_asks_first() {
        let fx = Fx::new("dupdel", true);
        fx.write_plans(PLANS);
        let mut app = fx.app();
        open_manager(&mut app);
        // Rows: sota-old, pota 2027-02-01, then the done one. Move to the second.
        press(&mut app, &[KeyCode::Down, KeyCode::Char('d')]);
        press(&mut app, &[KeyCode::Enter]);
        let text = fx.stored();
        assert!(text.contains("[plan.pota-us-4571-20270201]"), "{text}");
        assert!(text.contains("[plan.pota-us-4571-20270115]"), "{text}");
        assert!(text.contains("F1 = \"CQ PARK DE {MYCALL}\""), "{text}");

        press(&mut app, &[KeyCode::Char('x')]);
        assert!(has(&screen(&app, 100, 30), "Delete this plan?"));
        press(&mut app, &[KeyCode::Char('n')]);
        assert!(fx.stored().contains("[plan.pota-us-4571-20270115]"));
        press(&mut app, &[KeyCode::Char('x'), KeyCode::Char('y')]);
        assert!(!fx.stored().contains("[plan.pota-us-4571-20270115]"));
        assert!(fx.stored().contains("[plan.pota-us-4571-20270201]"));
    }

    #[test]
    fn a_change_on_disk_to_the_same_field_is_reported_not_overwritten() {
        let fx = Fx::new("conflict", true);
        fx.write_plans(PLANS);
        let mut app = fx.app();
        open_manager(&mut app);
        let on_disk = PLANS.replace("2027-01-10", "2027-01-11");
        fx.write_plans(&on_disk);
        // Edit the first plan's date.
        press(&mut app, &[KeyCode::Char('e'), KeyCode::Tab, KeyCode::Tab]);
        for _ in 0..10 {
            press(&mut app, &[KeyCode::Backspace]);
        }
        type_str(&mut app, "2027-01-12");
        press(&mut app, &[KeyCode::Enter]);
        assert_eq!(fx.stored(), on_disk);
        let rows = screen(&app, 100, 30);
        assert!(has(&rows, "changed on disk"), "{rows:#?}");
    }

    #[test]
    fn an_unparseable_file_is_shown_and_never_saved_over() {
        let fx = Fx::new("broken", true);
        fx.write_plans("[plan");
        let mut app = fx.app();
        assert!(app.plans_error().is_some());
        open_manager(&mut app);
        let rows = screen(&app, 100, 30);
        assert!(has(&rows, "line 1"), "{rows:#?}");
        new_plan(&mut app, "us-4571", "2027-02-01", "", "");
        assert_eq!(fx.stored(), "[plan");
    }
}

mod references {
    use super::*;

    #[test]
    fn saving_fetches_the_one_reference_and_stores_a_snapshot() {
        let fx = Fx::new("fetch", false);
        let mut app = fx.app();
        open_manager(&mut app);
        new_plan(&mut app, "us-4571", "2027-02-01", "", "");
        // Nothing is cached yet, so the plan is saved unresolved and one request goes out.
        assert!(!fx.stored().contains("snapshot"));
        fx.client.sync();
        assert_eq!(fx.routes.calls.load(Ordering::SeqCst), 1);
        assert!(fx.routes.urls.lock().unwrap()[0].ends_with("/park/US-4571"));

        // A later tick picks the answer out of the cache, without another request.
        app.tick(Instant::now() + Duration::from_secs(10));
        let rows = screen(&app, 100, 30);
        assert!(has(&rows, "Potato Creek State Park"), "{rows:#?}");
        assert!(fx.stored().contains("name = \"Potato Creek State Park\""));
        assert_eq!(fx.routes.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn saving_an_nfer_plan_stores_a_snapshot_of_every_reference() {
        let fx = Fx::new("nfer", false);
        let mut app = fx.app();
        open_manager(&mut app);
        new_plan(&mut app, "us-4571, us-4572", "2027-02-01", "", "");
        // One request per reference, through the client.
        fx.client.sync();
        assert_eq!(fx.routes.calls.load(Ordering::SeqCst), 2);
        app.tick(Instant::now() + Duration::from_secs(10));
        let stored = fx.stored();
        assert!(stored.contains("Potato Creek State Park"), "{stored}");
        assert!(stored.contains("Second Creek State Park"), "{stored}");
        assert!(stored.contains("US-MI"), "{stored}");

        // Wipe the cache: a fresh App still has both from the file, and asks for nothing.
        std::fs::remove_dir_all(&fx.cache).unwrap();
        let calls = fx.routes.calls.load(Ordering::SeqCst);
        let mut app = fx.app();
        app.tick(Instant::now() + Duration::from_secs(10));
        fx.client.sync();
        assert_eq!(fx.routes.calls.load(Ordering::SeqCst), calls);
    }

    #[test]
    fn a_failed_lookup_still_saves_the_plan_as_unresolved() {
        let fx = Fx::new("offline", true);
        let mut app = fx.app();
        open_manager(&mut app);
        new_plan(&mut app, "us-4571", "2027-02-01", "", "");
        fx.client.sync();
        app.tick(Instant::now() + Duration::from_secs(10));
        assert!(fx.stored().contains("[plan.pota-us-4571-20270201]"));
        assert!(!fx.stored().contains("snapshot"));
        let rows = screen(&app, 100, 30);
        assert!(has(&rows, "unresolved"), "{rows:#?}");
    }

    #[test]
    fn a_plan_works_from_its_snapshot_after_the_cache_is_wiped() {
        let fx = Fx::new("wiped", true);
        fx.write_plans(PLANS);
        std::fs::remove_dir_all(&fx.cache).unwrap();
        let mut app = fx.app();
        open_manager(&mut app);
        let rows = screen(&app, 100, 30);
        assert!(has(&rows, "Potato Creek State Park"), "{rows:#?}");
        press(&mut app, &[KeyCode::Esc]);
        // The palette and a launch need no cache and make no request.
        app.on_key(ctrl('g'));
        let rows = screen(&app, 100, 30);
        assert!(has(&rows, "Potato Creek State Park"), "{rows:#?}");
        assert_eq!(fx.routes.calls.load(Ordering::SeqCst), 0);
    }
}

mod lifecycle {
    use super::*;

    fn palette_rows(app: &mut App) -> Vec<String> {
        app.on_key(ctrl('g'));
        screen(app, 100, 30)
    }

    #[test]
    fn plans_are_the_first_palette_rows_soonest_first_and_done_ones_are_left_out() {
        let fx = Fx::new("palette", true);
        fx.write_plans(PLANS);
        let mut app = fx.app();
        let rows = palette_rows(&mut app);
        let old = row_with(&rows, "SOTA W4C/CM-001");
        let pota = row_with(&rows, "POTA US-4571");
        let general = row_with(&rows, "rig not connected");
        assert!(old < pota && pota < general, "{rows:#?}");
        assert!(!has(&rows, "US-9999"), "{rows:#?}");
        assert!(rows[old].contains("overdue"), "{rows:#?}");
        assert!(rows[pota].contains("2027-02-01 planned"), "{rows:#?}");
    }

    #[test]
    fn the_list_flags_overdue_and_greys_done_plans_last() {
        let fx = Fx::new("list", true);
        fx.write_plans(PLANS);
        let mut app = fx.app();
        open_manager(&mut app);
        let rows = screen(&app, 100, 30);
        let (old, pota, done) = (
            row_with(&rows, "SOTA"),
            row_with(&rows, "US-4571"),
            row_with(&rows, "US-9999"),
        );
        assert!(old < pota && pota < done, "{rows:#?}");
        assert!(rows[old].contains("overdue") && rows[done].contains("done"));
        assert!(rows[pota].contains("planned"));

        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let buffer = terminal.backend().buffer();
        let x = rows[done].find("US-9999").unwrap() as u16;
        assert_eq!(buffer[(x, done as u16)].fg, Color::DarkGray);
        let x = rows[old].find("overdue").unwrap() as u16;
        assert_eq!(buffer[(x, old as u16)].fg, Color::Red);
    }

    #[test]
    fn launching_sets_my_reference_loads_the_soft_key_set_and_activates() {
        let fx = Fx::new("launch", true);
        fx.write_plans(PLANS);
        let mut app = fx.app();
        app.on_key(ctrl('g'));
        press(&mut app, &[KeyCode::Down, KeyCode::Enter]); // the POTA plan
        let plan = app.active_plan().expect("active");
        assert_eq!(plan.primary(), Some("US-4571"));
        let keys = app.resolved_soft_keys();
        assert_eq!(keys.keys[&1].text, "CQ PARK DE {MYCALL}");
        assert_eq!(keys.keys[&1].layer, Layer::PlanOverride);
        assert_eq!(keys.keys[&2].text, "POTA 2");
        assert_eq!(keys.keys[&2].layer, Layer::PlanSet);
        // No POTA Activity is hosted yet: the plan is active and the status says so.
        assert!(app.status().unwrap().contains("POTA"), "{:?}", app.status());

        let rows = {
            app.on_key(ctrl('g'));
            screen(&app, 100, 30)
        };
        assert!(has(&rows, "active"), "{rows:#?}");
    }

    #[test]
    fn a_restart_returns_an_active_plan_to_planned() {
        let fx = Fx::new("restart", true);
        fx.write_plans(PLANS);
        let mut app = fx.app();
        app.on_key(ctrl('g'));
        press(&mut app, &[KeyCode::Down, KeyCode::Enter]);
        assert!(app.active_plan().is_some());
        assert!(!fx.stored().contains("active"), "{}", fx.stored());

        let mut again = fx.app();
        assert!(again.active_plan().is_none());
        open_manager(&mut again);
        let rows = screen(&again, 100, 30);
        assert!(
            rows[row_with(&rows, "US-4571")].contains("planned"),
            "{rows:#?}"
        );
    }

    #[test]
    fn a_missing_soft_key_set_is_flagged_and_never_blocks_launching() {
        let fx = Fx::new("missing", true);
        fx.write_plans(&PLANS.replace("softkey_set = \"pota\"", "softkey_set = \"nope\""));
        let mut app = fx.app();
        app.on_key(ctrl('g'));
        press(&mut app, &[KeyCode::Down, KeyCode::Enter]);
        assert!(app.active_plan().is_some());
        let status = app.status().unwrap();
        assert!(
            status.contains("nope") && status.contains("not found"),
            "{status}"
        );
        // The lower layers still answer: the plan's own key, then the built-in defaults.
        let keys = app.resolved_soft_keys();
        assert_eq!(keys.keys[&1].layer, Layer::PlanOverride);
        assert_eq!(keys.keys[&3].layer, Layer::BuiltIn);
    }

    #[test]
    fn finishing_the_active_plan_by_hand_stops_using_it() {
        let fx = Fx::new("finish", true);
        fx.write_plans(PLANS);
        let mut app = fx.app();
        app.on_key(ctrl('g'));
        press(&mut app, &[KeyCode::Down, KeyCode::Enter]);
        open_manager(&mut app);
        press(&mut app, &[KeyCode::Down, KeyCode::Char('m')]);
        assert!(app.active_plan().is_none());
        assert_eq!(app.resolved_soft_keys().keys[&1].layer, Layer::BuiltIn);
        assert!(fx.stored().contains("status = \"done\""));
    }
}

mod hosted_activity {
    use super::*;
    use ratatui::Frame;
    use ratatui::crossterm::event::KeyEvent;
    use ratatui::layout::Rect;
    use rig_tui::{
        Activity, ActivityCtx, ActivityOutput, Command, LogDefaults, LogMode, RigMode, Role,
    };

    type Told = Arc<Mutex<Vec<(Role, Vec<String>)>>>;

    /// A stand-in POTA Activity that records what it was told about the plan.
    struct Pota(Told);

    impl Activity for Pota {
        fn name(&self) -> &'static str {
            "POTA"
        }
        fn on_key(&mut self, _: KeyEvent, _: &ActivityCtx) -> Option<ActivityOutput> {
            None
        }
        fn on_tick(&mut self, _: Instant, _: &ActivityCtx) -> Vec<Command> {
            Vec::new()
        }
        fn on_reply(&mut self, _: &str, _: Result<String, String>) -> Result<(), String> {
            Ok(())
        }
        fn on_mode(&mut self, _: &ActivityCtx) {}
        fn text_focus(&self, _: &ActivityCtx) -> bool {
            false
        }
        fn has_panel(&self, _: &ActivityCtx) -> bool {
            false
        }
        fn log_hints(&self, _: &RigMode) -> (Option<LogMode>, LogDefaults) {
            (None, LogDefaults::default())
        }
        fn hint(&self, _: &ActivityCtx) -> &'static str {
            ""
        }
        fn render(&self, _: &mut Frame, _: Rect, _: &ActivityCtx) {}
        fn set_plan(&mut self, role: Role, my_refs: &[String]) {
            self.0.lock().unwrap().push((role, my_refs.to_vec()));
        }
    }

    #[test]
    fn launching_brings_the_plans_activity_forward_in_the_activator_role() {
        let fx = Fx::new("host", true);
        fx.write_plans(PLANS);
        let told: Told = Arc::new(Mutex::new(Vec::new()));
        let mut app = fx.app();
        app.host_activity(Box::new(Pota(told.clone())));
        app.on_key(ctrl('g'));
        type_str(&mut app, "US-4571");
        press(&mut app, &[KeyCode::Enter]);
        assert_eq!(app.activity_name(), "POTA");
        assert_eq!(
            told.lock().unwrap().last().unwrap(),
            &(Role::PotaActivator, vec!["US-4571".to_string()])
        );
    }

    #[test]
    fn the_form_and_list_fit_the_80_by_24_floor() {
        let fx = Fx::new("floor", true);
        fx.write_plans(PLANS);
        let mut app = fx.app();
        app.on_key(ctrl('g'));
        type_str(&mut app, "plans");
        press(&mut app, &[KeyCode::Enter]);
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "n new") && has(&rows, "Esc close"), "{rows:#?}");
        press(&mut app, &[KeyCode::Char('n')]);
        let rows = screen(&app, 80, 24);
        assert!(
            has(&rows, "Notes:") && has(&rows, "Enter save"),
            "{rows:#?}"
        );
    }
}
