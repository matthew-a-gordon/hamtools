//! The Contact form per Role, driven through the App with a stand-in Activity that supplies the
//! Role. Contacts go to a temp-dir Log, never a real one.

mod common;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

use common::*;
use logbook::{Callsign, Log};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use rig_tui::{
    Activity, ActivityCtx, ActivityOutput, App, Command, LogDefaults, LogMode, RigEvent, RigMode,
    Role,
};

fn temp_log(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rig-tui-contact-form-{}-{}-{tag}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("log.adi")
}

fn utc(secs: u64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(86_400 * 19_000 + secs)
}

const ROLES: [Role; 6] = [
    Role::General,
    Role::Satellite,
    Role::SotaActivator,
    Role::SotaChaser,
    Role::PotaActivator,
    Role::PotaHunter,
];

/// What the stand-in saw of the form through its context.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
struct Seen {
    their_call: String,
    their_ref: String,
    rst_sent: String,
    rst_rcvd: String,
}

struct RoleActivity {
    defaults: LogDefaults,
    seen: Rc<RefCell<Option<Seen>>>,
}

impl Activity for RoleActivity {
    fn name(&self) -> &'static str {
        "Role"
    }
    fn on_key(&mut self, key: KeyEvent, ctx: &ActivityCtx) -> Option<ActivityOutput> {
        if let KeyCode::F(n) = key.code {
            *self.seen.borrow_mut() = ctx.form.map(|f| Seen {
                their_call: f.their_call.to_string(),
                their_ref: f.their_ref.to_string(),
                rst_sent: f.rst_sent.to_string(),
                rst_rcvd: f.rst_rcvd.to_string(),
            });
            return Some(ActivityOutput {
                commands: vec![Command::Send(format!("F{n}"))],
                status: None,
            });
        }
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
    fn log_hints(&self, mode: &RigMode) -> (Option<LogMode>, LogDefaults) {
        let rst = match mode {
            RigMode::Cw | RigMode::Cwr => "599",
            _ => "59",
        };
        (
            Some(LogMode {
                mode: if rst == "599" { "CW" } else { "SSB" }.to_string(),
                submode: (rst == "59").then(|| "USB".to_string()),
                default_rst: Some(rst.to_string()),
            }),
            self.defaults.clone(),
        )
    }
    fn hint(&self, _ctx: &ActivityCtx) -> &'static str {
        "role hint"
    }
    fn render(&self, frame: &mut Frame, area: Rect, _ctx: &ActivityCtx) {
        frame.render_widget(Paragraph::new("ROLE BODY"), area);
    }
}

fn defaults(role: Role, my_refs: &[&str], their_ref: Option<&str>) -> LogDefaults {
    LogDefaults {
        role,
        my_refs: my_refs.iter().map(|s| s.to_string()).collect(),
        their_ref: their_ref.map(String::from),
        ..LogDefaults::default()
    }
}

/// A connected app on 14.074 MHz in `mode`, hosting a stand-in with `defaults` in the foreground.
fn app_as(defaults: LogDefaults, mode: &str, log: &Path) -> (App, Rc<RefCell<Option<Seen>>>) {
    let mut app = App::with_callsign(Some("K9TST".to_string()))
        .with_log_path(Some(log.to_path_buf()))
        .with_locator(Some("EN71DQ".to_string()));
    app.set_utc(utc(45_296));
    let seen = Rc::new(RefCell::new(None));
    let index = app.host_activity(Box::new(RoleActivity {
        defaults,
        seen: Rc::clone(&seen),
    }));
    app.set_foreground(index);
    let mut app = connected_app_in(app, mode);
    app.on_rig(RigEvent::Split {
        enabled: false,
        tx_vfo: "VFOA".to_string(),
    });
    (app, seen)
}

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

fn open(app: &mut App) {
    app.on_key(ctrl('l'));
    assert!(
        has(&screen(app, 80, 24), "Log contact"),
        "form did not open: {:?}",
        app.status()
    );
}

fn records(log: &Path) -> Vec<logbook::Record> {
    Log::new(log, Callsign::new("K9TST").unwrap())
        .read()
        .unwrap()
}

mod fields {
    use super::*;

    fn rows_for(role: Role, their_ref: Option<&str>) -> Vec<String> {
        let (mut app, _) = app_as(defaults(role, &[], their_ref), "USB", &temp_log("fields"));
        open(&mut app);
        screen(&app, 80, 24)
    }

    #[test]
    fn general_and_satellite_keep_grid_and_name_and_have_no_their_ref() {
        for role in [Role::General, Role::Satellite] {
            let rows = rows_for(role, None);
            for label in [
                "Call:",
                "Grid:",
                "Name:",
                "RST sent:",
                "RST rcvd:",
                "Comment:",
            ] {
                assert!(has(&rows, label), "{role:?} lacks {label}: {rows:#?}");
            }
            assert!(!has(&rows, "Their ref"), "{role:?}: {rows:#?}");
        }
    }

    #[test]
    fn activators_have_their_ref_and_no_grid_or_name() {
        for role in [Role::SotaActivator, Role::PotaActivator] {
            let rows = rows_for(role, None);
            for label in ["Call:", "RST sent:", "RST rcvd:", "Their ref:", "Comment:"] {
                assert!(has(&rows, label), "{role:?} lacks {label}: {rows:#?}");
            }
            assert!(!has(&rows, "Grid:"), "{role:?}: {rows:#?}");
            assert!(!has(&rows, "Name:"), "{role:?}: {rows:#?}");
        }
    }

    #[test]
    fn hunters_have_their_ref_prefilled_and_no_name_or_grid() {
        for (role, their_ref) in [
            (Role::SotaChaser, "W7W/SN-001"),
            (Role::PotaHunter, "US-0001"),
        ] {
            let rows = rows_for(role, Some(their_ref));
            assert!(has(&rows, &format!("Their ref: {their_ref}")), "{rows:#?}");
            assert!(!has(&rows, "Name:"), "{role:?}: {rows:#?}");
            assert!(!has(&rows, "Grid:"), "{role:?}: {rows:#?}");
        }
    }

    #[test]
    fn rst_defaults_follow_the_mode_in_every_role() {
        for role in ROLES {
            let (mut app, _) = app_as(defaults(role, &[], None), "CW", &temp_log("rst"));
            open(&mut app);
            let rows = screen(&app, 80, 24);
            assert!(has(&rows, "RST sent: 599"), "{role:?}: {rows:#?}");
            assert!(has(&rows, "RST rcvd: 599"), "{role:?}: {rows:#?}");
        }
    }
}

mod extras {
    use super::*;

    fn log_one(defaults: LogDefaults, call: &str, their_ref: &str) -> Vec<logbook::Record> {
        let log = temp_log("extras");
        let (mut app, _) = app_as(defaults, "CW", &log);
        open(&mut app);
        type_str(&mut app, call);
        if !their_ref.is_empty() {
            app.on_key(key(KeyCode::Tab)); // RST sent
            app.on_key(key(KeyCode::Tab)); // RST rcvd
            app.on_key(key(KeyCode::Tab)); // Their ref
            type_str(&mut app, their_ref);
        }
        app.on_key(key(KeyCode::Enter));
        records(&log)
    }

    #[test]
    fn pota_activator_writes_my_sig_and_their_sig_as_standard_extras() {
        let recs = log_one(
            defaults(Role::PotaActivator, &["US-1234"], None),
            "w1abc",
            "us-0001",
        );
        assert_eq!(recs.len(), 1);
        assert_eq!(
            recs[0].extras(),
            vec![
                ("MY_SIG", "POTA"),
                ("MY_SIG_INFO", "US-1234"),
                ("SIG", "POTA"),
                ("SIG_INFO", "US-0001"),
            ]
        );
    }

    #[test]
    fn an_nfer_is_one_contact_listing_its_parks() {
        let recs = log_one(
            defaults(Role::PotaActivator, &["US-1234", "US-5678"], None),
            "w1abc",
            "",
        );
        assert_eq!(recs.len(), 1);
        assert_eq!(
            recs[0].extras(),
            vec![("MY_SIG", "POTA"), ("MY_SIG_INFO", "US-1234,US-5678")]
        );
    }

    #[test]
    fn sota_activator_writes_my_and_their_summit() {
        let recs = log_one(
            defaults(Role::SotaActivator, &["W9/SE-001"], None),
            "w7xyz",
            "w7w/sn-001",
        );
        assert_eq!(
            recs[0].extras(),
            vec![("MY_SOTA_REF", "W9/SE-001"), ("SOTA_REF", "W7W/SN-001")]
        );
    }

    #[test]
    fn hunters_log_the_spot_reference_as_theirs_and_write_no_my_ref() {
        let recs = log_one(
            defaults(Role::PotaHunter, &[], Some("US-0001")),
            "w1abc",
            "",
        );
        assert_eq!(
            recs[0].extras(),
            vec![("SIG", "POTA"), ("SIG_INFO", "US-0001")]
        );
        let recs = log_one(
            defaults(Role::SotaChaser, &[], Some("W7W/SN-001")),
            "w7xyz",
            "",
        );
        assert_eq!(recs[0].extras(), vec![("SOTA_REF", "W7W/SN-001")]);
    }

    #[test]
    fn general_writes_no_reference_extras() {
        let recs = log_one(defaults(Role::General, &[], None), "w1abc", "");
        assert!(recs[0].extras().is_empty());
    }
}

mod quick_entry {
    use super::*;

    #[test]
    fn enter_on_call_saves_and_reopens_on_a_cleared_call() {
        let log = temp_log("quick");
        let (mut app, _) = app_as(
            defaults(Role::PotaActivator, &["US-1234"], None),
            "CW",
            &log,
        );
        open(&mut app);
        type_str(&mut app, "w1abc");
        app.on_key(key(KeyCode::Enter));

        assert_eq!(records(&log).len(), 1);
        assert_eq!(app.logged_count(), 1);
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "Log contact"), "form should reopen: {rows:#?}");
        assert!(has(&rows, "Call: "), "{rows:#?}");
        assert!(
            !has(&rows, "Call: W1ABC"),
            "call should be cleared: {rows:#?}"
        );

        type_str(&mut app, "k2def");
        app.on_key(key(KeyCode::Enter));
        let recs = records(&log);
        assert_eq!(
            recs.iter().filter_map(|r| r.call()).collect::<Vec<_>>(),
            vec!["W1ABC", "K2DEF"]
        );
    }

    #[test]
    fn the_start_time_is_when_the_first_call_character_is_typed() {
        let log = temp_log("start");
        let (mut app, _) = app_as(
            defaults(Role::PotaActivator, &["US-1234"], None),
            "CW",
            &log,
        );
        open(&mut app); // 12:34:56
        app.set_utc(utc(12 * 3600 + 40 * 60));
        type_str(&mut app, "w1abc");
        app.set_utc(utc(12 * 3600 + 41 * 60));
        app.on_key(key(KeyCode::Enter));
        let recs = records(&log);
        assert_eq!(recs[0].get("TIME_ON"), Some("124000"));
        assert_eq!(recs[0].get("TIME_OFF"), Some("124100"));
    }

    #[test]
    fn a_general_form_does_not_reopen() {
        let log = temp_log("noreopen");
        let (mut app, _) = app_as(defaults(Role::General, &[], None), "CW", &log);
        open(&mut app);
        type_str(&mut app, "w1abc");
        app.on_key(key(KeyCode::Enter));
        assert!(!has(&screen(&app, 80, 24), "Log contact"));
    }

    #[test]
    fn esc_discards_a_half_typed_contact() {
        let log = temp_log("esc");
        let (mut app, _) = app_as(
            defaults(Role::PotaActivator, &["US-1234"], None),
            "CW",
            &log,
        );
        open(&mut app);
        type_str(&mut app, "w1a");
        app.on_key(key(KeyCode::Esc));
        assert!(!has(&screen(&app, 80, 24), "Log contact"));
        assert!(records(&log).is_empty());
        open(&mut app);
        assert!(!has(&screen(&app, 80, 24), "W1A"));
    }

    #[test]
    fn the_reopened_form_is_refused_when_the_rig_has_gone_stale() {
        let log = temp_log("stale-reopen");
        let (mut app, _) = app_as(
            defaults(Role::PotaActivator, &["US-1234"], None),
            "CW",
            &log,
        );
        open(&mut app);
        type_str(&mut app, "w1abc");
        app.tick(Instant::now() + Duration::from_secs(3));
        app.on_key(key(KeyCode::Enter));
        // The Contact is saved from the open-time snapshot; the next one needs a fresh rig.
        assert_eq!(records(&log).len(), 1);
        assert!(!has(&screen(&app, 80, 24), "Log contact"));
        let status = app.status().unwrap_or_default();
        assert!(status.contains("logged W1ABC"), "{status}");
        assert!(status.contains("frequency and mode"), "{status}");
    }
}

mod soft_keys {
    use super::*;

    #[test]
    fn f_keys_reach_the_activity_while_the_form_is_focused() {
        let log = temp_log("fkey");
        let (mut app, _) = app_as(
            defaults(Role::PotaActivator, &["US-1234"], None),
            "CW",
            &log,
        );
        open(&mut app);
        type_str(&mut app, "w1");
        let commands = app.on_key(key(KeyCode::F(2)));
        assert_eq!(commands, vec![Command::Send("F2".to_string())]);
        // The form is still open with what was typed.
        assert!(has(&screen(&app, 80, 24), "W1"));
    }

    #[test]
    fn the_form_exposes_reports_their_call_and_their_ref() {
        let log = temp_log("expose");
        let (mut app, seen) = app_as(defaults(Role::PotaHunter, &[], Some("US-0001")), "CW", &log);
        open(&mut app);
        type_str(&mut app, "k2def");
        app.on_key(key(KeyCode::Tab)); // RST sent
        app.on_key(key(KeyCode::Backspace));
        type_str(&mut app, "5");
        app.on_key(key(KeyCode::F(1)));
        assert_eq!(
            seen.borrow().clone(),
            Some(Seen {
                their_call: "K2DEF".to_string(),
                their_ref: "US-0001".to_string(),
                rst_sent: "595".to_string(),
                rst_rcvd: "599".to_string(),
            })
        );
    }

    #[test]
    fn with_no_form_open_the_activity_sees_none() {
        let log = temp_log("noform");
        let (mut app, seen) = app_as(defaults(Role::PotaActivator, &[], None), "CW", &log);
        app.on_key(key(KeyCode::F(1)));
        assert_eq!(seen.borrow().clone(), None);
    }
}

mod gate_in_every_role {
    use super::*;

    #[test]
    fn freshness_refusal_holds() {
        for role in ROLES {
            let (mut app, _) = app_as(defaults(role, &[], None), "CW", &temp_log("fresh"));
            app.tick(Instant::now() + Duration::from_secs(3));
            app.on_key(ctrl('l'));
            assert!(
                app.status()
                    .unwrap_or_default()
                    .contains("frequency and mode"),
                "{role:?}: {:?}",
                app.status()
            );
            assert!(!has(&screen(&app, 80, 24), "Log contact"), "{role:?}");
        }
    }

    #[test]
    fn the_open_time_snapshot_holds() {
        for role in ROLES {
            let log = temp_log("snap");
            let (mut app, _) = app_as(defaults(role, &[], None), "CW", &log);
            open(&mut app);
            app.on_rig(RigEvent::Freq(7_030_000));
            type_str(&mut app, "w1abc");
            app.on_key(key(KeyCode::Enter));
            let recs = records(&log);
            assert_eq!(recs[0].get("FREQ"), Some("14.074000"), "{role:?}");
        }
    }

    #[test]
    fn the_transmit_frequency_is_logged_under_split() {
        for role in ROLES {
            let log = temp_log("split");
            let (mut app, _) = app_as(defaults(role, &[], None), "CW", &log);
            app.on_rig(RigEvent::Split {
                enabled: true,
                tx_vfo: "VFOB".to_string(),
            });
            app.on_rig(RigEvent::SplitFreq(14_076_000));
            open(&mut app);
            type_str(&mut app, "w1abc");
            app.on_key(key(KeyCode::Enter));
            let recs = records(&log);
            assert_eq!(recs[0].get("FREQ"), Some("14.076000"), "{role:?}");
        }
    }
}
