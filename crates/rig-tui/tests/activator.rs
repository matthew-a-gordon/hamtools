//! The SOTA and POTA Activator screens and the Activation tracker, through the App: keys in,
//! rendered buffers and Log files out. Contacts go to a temp-dir Log; nothing touches the network
//! or a rig.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

use common::*;
use logbook::{Callsign, Log};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rig_tui::{Activator, App, Command, RigEvent};

/// 2026-11-05 00:00:00 UTC.
const DAY: u64 = 1_793_836_800;

struct Fx {
    dir: PathBuf,
}

impl Fx {
    fn new(tag: &str, plans: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rig-tui-activator-{}-{}-{tag}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("plans.toml"), plans).unwrap();
        Self { dir }
    }

    fn log(&self) -> PathBuf {
        self.dir.join("log.adi")
    }

    fn records(&self) -> Vec<logbook::Record> {
        Log::new(self.log(), Callsign::new("K9TST").unwrap())
            .read()
            .unwrap()
    }

    /// A connected app on 14.074 MHz CW with both Activator screens hosted, General in front.
    fn app(&self) -> App {
        let mut app = App::with_callsign(Some("K9TST".to_string()))
            .with_soft_keys([(1, "CQ DE {MYCALL}".to_string())].into())
            .with_log_path(Some(self.log()))
            .with_locator(Some("EN71DQ".to_string()))
            .with_config_path(Some(self.dir.join("config.toml")));
        app.host_activity(Box::new(Activator::sota()));
        app.host_activity(Box::new(Activator::pota()));
        at(&mut app, 12 * 3600);
        let mut app = connected_app_in(app, "CW");
        app.on_rig(RigEvent::Split {
            enabled: false,
            tx_vfo: "VFOA".to_string(),
        });
        app
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Set the wall clock to `secs` after midnight UTC on the plan day, and let the app notice.
fn at(app: &mut App, secs: u64) {
    app.set_utc(UNIX_EPOCH + Duration::from_secs(DAY + secs));
    app.tick(Instant::now());
}

const POTA_PLAN: &str = r#"
[plan.pota-us-4571-20261105]
activity = "pota"
references = ["US-4571"]
date = "2026-11-05"
[plan.pota-us-4571-20261105.snapshot]
reference = "US-4571"
name = "Potato Creek State Park"
location = "US-IN"
grid = "EN61"
lat = 41.55
lon = -86.37
"#;

const NFER_PLAN: &str = r#"
[plan.nfer]
activity = "pota"
references = ["US-1", "US-2"]
date = "2026-11-05"
"#;

const SOTA_PLAN: &str = r#"
[plan.sota]
activity = "sota"
references = ["W7W/KG-001"]
date = "2026-11-05"
[plan.sota.snapshot]
reference = "W7W/KG-001"
name = "Mount Test"
alt_m = 1500
points = 6
"#;

fn launch(app: &mut App, name: &str) {
    app.on_key(ctrl('g'));
    type_str(app, name);
    app.on_key(key(KeyCode::Enter));
    assert!(app.active_plan().is_some(), "{:?}", app.status());
}

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

fn enter(app: &mut App) {
    app.on_key(key(KeyCode::Enter));
}

/// Type `call` into the form and press Enter.
fn log_call(app: &mut App, call: &str) {
    type_str(app, call);
    enter(app);
}

fn field<'a>(r: &'a logbook::Record, name: &str) -> Option<&'a str> {
    r.get(name)
}

mod plan_and_banner {
    use super::*;

    #[test]
    fn launching_a_pota_plan_shows_the_banner_tracker_and_form_at_80x24() {
        let fx = Fx::new("banner", POTA_PLAN);
        let mut app = fx.app();
        launch(&mut app, "US-4571");

        assert_eq!(app.activity_name(), "POTA");
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "US-4571  Potato Creek State Park"), "{rows:#?}");
        assert!(has(&rows, "EN61"), "{rows:#?}");
        assert!(has(&rows, " km"), "{rows:#?}");
        assert!(has(&rows, "2026-11-05"), "{rows:#?}");
        assert!(has(&rows, "POTA US-4571: 0 of 10"), "{rows:#?}");
        assert!(has(&rows, "Call:"), "{rows:#?}");
        assert!(has(&rows, "Their ref:"), "{rows:#?}");
        assert!(has(&rows, "Send (Enter)"), "{rows:#?}");
        assert!(has(&rows, "CQ DE K9TST"), "{rows:#?}");
    }

    #[test]
    fn a_sota_plan_shows_altitude_points_and_the_distinct_count() {
        let fx = Fx::new("sota", SOTA_PLAN);
        let mut app = fx.app();
        launch(&mut app, "W7W/KG-001");

        assert_eq!(app.activity_name(), "SOTA");
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "1500 m"), "{rows:#?}");
        assert!(has(&rows, "6 pts"), "{rows:#?}");
        assert!(has(&rows, "SOTA W7W/KG-001: 0 of 4 distinct"), "{rows:#?}");
    }

    #[test]
    fn no_active_plan_says_so_offers_the_plans_and_still_logs() {
        let fx = Fx::new("noplan", POTA_PLAN);
        let mut app = fx.app();
        app.set_foreground(2); // POTA
        assert_eq!(app.activity_name(), "POTA");

        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "no active plan"), "{rows:#?}");
        assert!(has(&rows, "Alt-1 US-4571"), "{rows:#?}");

        log_call(&mut app, "W1AW");
        let records = fx.records();
        assert_eq!(records.len(), 1, "{:?}", app.status());
        assert_eq!(field(&records[0], "CALL"), Some("W1AW"));
        assert_eq!(field(&records[0], "MY_SIG"), None);
        assert_eq!(field(&records[0], "MY_SIG_INFO"), None);
    }

    #[test]
    fn alt_digit_launches_the_listed_plan() {
        let fx = Fx::new("alt", POTA_PLAN);
        let mut app = fx.app();
        app.set_foreground(2);
        app.on_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::ALT));
        assert_eq!(app.active_plan().and_then(|p| p.primary()), Some("US-4571"));
        assert!(has(&screen(&app, 80, 24), "POTA US-4571: 0 of 10"));
    }

    #[test]
    fn picking_a_plan_later_does_not_change_earlier_contacts() {
        let fx = Fx::new("later", POTA_PLAN);
        let mut app = fx.app();
        app.set_foreground(2);
        log_call(&mut app, "W1AW");
        let before = std::fs::read(fx.log()).unwrap();

        launch(&mut app, "US-4571");
        log_call(&mut app, "K9XX");

        let after = std::fs::read(fx.log()).unwrap();
        assert!(after.starts_with(&before), "the earlier bytes changed");
        let records = fx.records();
        assert_eq!(field(&records[0], "MY_SIG_INFO"), None);
        assert_eq!(field(&records[1], "MY_SIG_INFO"), Some("US-4571"));
        // The tracker counts only the Contact made under the plan.
        assert!(has(&screen(&app, 80, 24), "POTA US-4571: 1 of 10"));
    }
}

mod tracker {
    use super::*;

    #[test]
    fn each_saved_contact_moves_the_count_and_the_form_reopens_on_call() {
        let fx = Fx::new("count", POTA_PLAN);
        let mut app = fx.app();
        launch(&mut app, "US-4571");

        log_call(&mut app, "W1AW");
        log_call(&mut app, "K9XX");

        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "POTA US-4571: 2 of 10"), "{rows:#?}");
        let records = fx.records();
        assert_eq!(records.len(), 2);
        assert_eq!(field(&records[1], "MY_SIG"), Some("POTA"));
        assert_eq!(field(&records[1], "MY_SIG_INFO"), Some("US-4571"));
        // The next call lands in a fresh Call field.
        type_str(&mut app, "N0AA");
        assert!(has(&screen(&app, 80, 24), "Call: N0AA"));
    }

    #[test]
    fn duplicates_warn_softly_never_block_and_count_once() {
        let fx = Fx::new("dupe", POTA_PLAN);
        let mut app = fx.app();
        launch(&mut app, "US-4571");
        log_call(&mut app, "W1AW");

        type_str(&mut app, "W1AW");
        let rows = screen(&app, 80, 24);
        assert!(
            has(&rows, "duplicate: W1AW already logged on 20m CW"),
            "{rows:#?}"
        );

        enter(&mut app);
        assert_eq!(fx.records().len(), 2, "a duplicate must still save");
        assert!(has(&screen(&app, 80, 24), "POTA US-4571: 1 of 10"));
    }

    #[test]
    fn a_different_band_or_call_is_not_a_duplicate() {
        let fx = Fx::new("nodupe", POTA_PLAN);
        let mut app = fx.app();
        launch(&mut app, "US-4571");
        log_call(&mut app, "W1AW");
        type_str(&mut app, "W1AX");
        assert!(!has(&screen(&app, 80, 24), "duplicate"));
    }

    #[test]
    fn an_nfer_counts_toward_each_park_with_one_contact() {
        let fx = Fx::new("nfer", NFER_PLAN);
        let mut app = fx.app();
        launch(&mut app, "US-1");

        log_call(&mut app, "W1AW");

        assert_eq!(fx.records().len(), 1);
        assert_eq!(field(&fx.records()[0], "MY_SIG_INFO"), Some("US-1,US-2"));
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "POTA US-1: 1 of 10, US-2: 1 of 10"), "{rows:#?}");
    }

    #[test]
    fn ten_contacts_qualify_and_say_so() {
        let fx = Fx::new("ten", POTA_PLAN);
        let mut app = fx.app();
        launch(&mut app, "US-4571");
        for i in 0..10 {
            log_call(&mut app, &format!("W{i}AW"));
        }
        assert!(has(
            &screen(&app, 80, 24),
            "POTA US-4571: 10 of 10 (qualified)"
        ));
    }

    #[test]
    fn sota_counts_distinct_stations_not_bands() {
        let fx = Fx::new("sotadistinct", SOTA_PLAN);
        let mut app = fx.app();
        launch(&mut app, "W7W/KG-001");
        log_call(&mut app, "W1AW");
        log_call(&mut app, "W1AW");
        log_call(&mut app, "K9XX");
        assert!(has(
            &screen(&app, 80, 24),
            "SOTA W7W/KG-001: 2 of 4 distinct"
        ));
    }

    #[test]
    fn the_tracker_is_read_from_the_log_each_time() {
        let fx = Fx::new("fromlog", POTA_PLAN);
        let mut app = fx.app();
        launch(&mut app, "US-4571");
        log_call(&mut app, "W1AW");
        // A second program appends to the same Log; the screen follows it on the next refresh.
        let other = Log::new(fx.log(), Callsign::new("K9TST").unwrap());
        let contact = logbook::Contact::new(logbook::ContactInput {
            call: "N0ZZ".into(),
            grid: None,
            name: None,
            comment: None,
            rst_sent: None,
            rst_rcvd: None,
            start: chrono::DateTime::from_timestamp((DAY + 43_000) as i64, 0).unwrap(),
            end: chrono::DateTime::from_timestamp((DAY + 43_060) as i64, 0).unwrap(),
            freq_hz: 7_030_000,
            mode: logbook::Mode::Cw,
            own_grid: None,
            station_callsign: None,
            extras: vec![
                ("MY_SIG".into(), "POTA".into()),
                ("MY_SIG_INFO".into(), "US-4571".into()),
            ],
        })
        .unwrap();
        other.append(&contact).unwrap();
        // Ticks pass; the view is rebuilt at most every few seconds.
        app.tick(Instant::now() + Duration::from_secs(5));
        assert!(has(&screen(&app, 80, 24), "POTA US-4571: 2 of 10"));
    }

    #[test]
    fn a_utc_rollover_warns_and_counts_the_new_day() {
        let fx = Fx::new("rollover", POTA_PLAN);
        let mut app = fx.app();
        launch(&mut app, "US-4571");
        log_call(&mut app, "W1AW");
        assert!(!has(&screen(&app, 80, 24), "rolled over"));

        at(&mut app, 24 * 3600 + 600); // 00:10Z the next day
        app.tick(Instant::now() + Duration::from_secs(5));

        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "UTC date rolled over"), "{rows:#?}");
        assert!(has(&rows, "POTA US-4571: 0 of 10"), "{rows:#?}");
        // Logging goes on regardless.
        log_call(&mut app, "K9XX");
        assert_eq!(fx.records().len(), 2);
    }

    #[test]
    fn a_plan_for_another_day_warns_without_calling_it_a_rollover() {
        let fx = Fx::new("wrongday", POTA_PLAN);
        let mut app = fx.app();
        at(&mut app, 3 * 86_400);
        launch(&mut app, "US-4571");
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "plan date 2026-11-05 is not today"), "{rows:#?}");
    }
}

mod focus {
    use super::*;

    fn pota(fx: &Fx) -> App {
        let mut app = fx.app();
        launch(&mut app, "US-4571");
        app
    }

    #[test]
    fn the_form_has_focus_and_printable_keys_type_into_it() {
        let fx = Fx::new("typing", POTA_PLAN);
        let mut app = pota(&fx);
        type_str(&mut app, "f1");
        assert!(has(&screen(&app, 80, 24), "Call: F1"));
    }

    #[test]
    fn esc_moves_to_the_strip_where_arrows_tune_and_tab_returns() {
        let fx = Fx::new("strip", POTA_PLAN);
        let mut app = pota(&fx);

        app.on_key(key(KeyCode::Esc));
        let commands = app.on_key(key(KeyCode::Right));
        assert!(
            commands.iter().any(|c| matches!(c, Command::SetFreq(_))),
            "{commands:?}"
        );
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "strip keys, Tab to type"), "{rows:#?}");

        app.on_key(key(KeyCode::Tab));
        type_str(&mut app, "ab");
        assert!(has(&screen(&app, 80, 24), "Call: AB"));
    }

    #[test]
    fn from_the_strip_a_non_strip_printable_key_returns_to_the_form_and_types() {
        let fx = Fx::new("anykey", POTA_PLAN);
        let mut app = pota(&fx);
        app.on_key(key(KeyCode::Esc));
        // `w` is no strip key, so it starts typing.
        app.on_key(key(KeyCode::Char('w')));
        assert!(has(&screen(&app, 80, 24), "Call: W"));
    }

    #[test]
    fn esc_on_a_half_typed_contact_discards_it_and_stays_in_the_form() {
        let fx = Fx::new("discard", POTA_PLAN);
        let mut app = pota(&fx);
        type_str(&mut app, "W1");
        app.on_key(key(KeyCode::Esc));

        let rows = screen(&app, 80, 24);
        assert!(!has(&rows, "Call: W1"), "{rows:#?}");
        assert!(has(&rows, "| typing"), "{rows:#?}");
        assert!(fx.records().is_empty());
    }

    #[test]
    fn the_title_says_whether_the_form_or_the_strip_has_the_keys() {
        let fx = Fx::new("title", POTA_PLAN);
        let mut app = pota(&fx);
        assert!(has(&screen(&app, 80, 24), "| typing"));
        app.on_key(key(KeyCode::Esc));
        assert!(has(&screen(&app, 80, 24), "Tab to type"));
    }

    #[test]
    fn soft_keys_work_from_the_form_with_the_typed_call() {
        let fx = Fx::new("soft", POTA_PLAN);
        let mut app = pota(&fx);
        type_str(&mut app, "W1AW");
        let commands = app.on_key(key(KeyCode::F(1)));
        assert_eq!(commands, vec![Command::Send("CQ DE K9TST".to_string())]);
        // The form keeps what was typed.
        assert!(has(&screen(&app, 80, 24), "Call: W1AW"));
    }

    #[test]
    fn ctrl_t_reaches_the_send_line_and_esc_comes_back_to_the_form() {
        let fx = Fx::new("sendline", POTA_PLAN);
        let mut app = pota(&fx);
        app.on_key(ctrl('t'));
        type_str(&mut app, "tu");
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "> TU"), "{rows:#?}");
        assert!(fx.records().is_empty());
        let commands = app.on_key(key(KeyCode::Enter));
        assert_eq!(commands, vec![Command::Send("TU".to_string())]);
        app.on_key(key(KeyCode::Esc));
        type_str(&mut app, "k9");
        assert!(has(&screen(&app, 80, 24), "Call: K9"));
    }

    #[test]
    fn leaving_the_screen_drops_the_form_so_no_modal_follows() {
        let fx = Fx::new("leave", POTA_PLAN);
        let mut app = pota(&fx);
        type_str(&mut app, "W1");
        app.set_foreground(0);
        let rows = screen(&app, 80, 24);
        assert!(!has(&rows, "Log contact"), "{rows:#?}");
        assert_eq!(app.status(), Some("half-typed Contact discarded"));
    }
}

mod sizes {
    use super::*;

    fn logged_app(fx: &Fx) -> App {
        let mut app = fx.app();
        launch(&mut app, "US-4571");
        log_call(&mut app, "W1AW");
        app
    }

    #[test]
    fn the_floor_fits_without_the_optional_panes() {
        let fx = Fx::new("floor", POTA_PLAN);
        let app = logged_app(&fx);
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "POTA Activator"), "{rows:#?}");
        assert!(has(&rows, "Contact 14.074.000 CW"), "{rows:#?}");
        assert!(has(&rows, "Send (Enter)"), "{rows:#?}");
        // No bandplan strip, key hints or second column at this size.
        assert!(!has(&rows, "rx 14.074.000"), "{rows:#?}");
        assert!(!has(&rows, "Keys"), "{rows:#?}");
    }

    #[test]
    fn the_operators_tall_narrow_window_adds_the_strip_hints_and_the_recent_list() {
        let fx = Fx::new("tall", POTA_PLAN);
        let app = logged_app(&fx);
        let rows = screen(&app, 83, 47);
        assert!(has(&rows, "20m 14.000"), "{rows:#?}");
        assert!(has(&rows, "rx 14.074.000"), "{rows:#?}");
        assert!(has(&rows, "Keys"), "{rows:#?}");
        let recent = rows
            .iter()
            .position(|r| r.contains("Recent contacts"))
            .unwrap_or_else(|| panic!("{rows:#?}"));
        let row = rows[recent + 1..]
            .iter()
            .find(|r| r.contains("W1AW"))
            .unwrap_or_else(|| panic!("{rows:#?}"));
        assert!(row.contains("599/599"), "{row}");
    }

    #[test]
    fn the_wide_window_adds_a_second_column_with_the_recent_list() {
        let fx = Fx::new("wide", POTA_PLAN);
        let app = logged_app(&fx);
        let rows = screen(&app, 172, 48);
        let row = rows
            .iter()
            .find(|r| r.contains("Recent contacts"))
            .unwrap_or_else(|| panic!("{rows:#?}"));
        assert!(row.find("Recent contacts").unwrap() >= 120, "{row}");
        assert!(has(&rows, "POTA US-4571: 1 of 10"), "{rows:#?}");
        assert!(has(&rows, "rx 14.074.000"), "{rows:#?}");
    }

    #[test]
    fn a_park_to_park_contact_is_marked_in_the_recent_list() {
        let fx = Fx::new("p2p", POTA_PLAN);
        let mut app = fx.app();
        launch(&mut app, "US-4571");
        type_str(&mut app, "W1AW");
        app.on_key(key(KeyCode::Tab)); // RST sent
        app.on_key(key(KeyCode::Tab)); // RST rcvd
        app.on_key(key(KeyCode::Tab)); // Their ref
        type_str(&mut app, "us-1234");
        enter(&mut app);

        let records = fx.records();
        assert_eq!(field(&records[0], "SIG"), Some("POTA"));
        assert_eq!(field(&records[0], "SIG_INFO"), Some("US-1234"));
        let rows = screen(&app, 83, 47);
        assert!(
            rows.iter()
                .any(|r| r.contains("US-1234") && r.contains('P')),
            "{rows:#?}"
        );
    }
}

mod safety {
    use super::*;

    #[test]
    fn logging_never_waits_on_the_network_or_a_plan() {
        // No reference data is configured at all; the screen, the form and the Log work.
        let fx = Fx::new("offline", POTA_PLAN);
        let mut app = fx.app();
        app.set_foreground(2);
        log_call(&mut app, "W1AW");
        assert_eq!(fx.records().len(), 1);
    }

    #[test]
    fn an_unconnected_rig_shows_why_the_form_is_not_open() {
        let fx = Fx::new("down", POTA_PLAN);
        let mut app = App::with_callsign(Some("K9TST".to_string()))
            .with_log_path(Some(Path::new(&fx.log()).to_path_buf()));
        app.host_activity(Box::new(Activator::pota()));
        app.set_foreground(1);
        app.tick(Instant::now());
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "not connected"), "{rows:#?}");
        // A key that would have typed does nothing and does not crash.
        assert_eq!(app.on_key(key(KeyCode::Char('w'))), vec![]);
    }
}
