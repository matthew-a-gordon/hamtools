//! The POTA Activator's "you are spotted" line and nearby spots, through the App, with a fake
//! clock and a fake transport: no network, no rig.

mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

use common::*;
use ratatui::crossterm::event::KeyCode;
use rig_tui::reference::{
    Clock, HttpRequest, HttpResponse, Limits, ReferenceClient, Transport, user_agent,
};
use rig_tui::{Activator, App, RigEvent};

/// 2026-11-05T12:00:00Z.
const NOW: u64 = 1_793_836_800 + 12 * 3600;

const PLANS: &str = r#"
[plan.pota]
activity = "pota"
references = ["US-4571"]
date = "2026-11-05"

[plan.sota]
activity = "sota"
references = ["W7W/KG-001"]
date = "2026-11-05"
"#;

/// K9TST spotted 40 s ago by W9XYZ; two other activators, one near EN71DQ and one far.
const SPOTS: &str = r#"[
 {"activator":"K9TST","frequency":"14062.5","mode":"CW","reference":"US-4571","spotter":"W9XYZ",
  "spotTime":"2026-11-05T11:59:20","expire":600,"latitude":41.55,"longitude":-86.37},
 {"activator":"K1FAR","frequency":"7030","mode":"CW","reference":"US-0002",
  "spotTime":"2026-11-05T11:59:00","expire":600,"latitude":35.0,"longitude":-100.0},
 {"activator":"N9NEAR","frequency":"7200","mode":"SSB","reference":"US-0001",
  "spotTime":"2026-11-05T11:58:00","expire":600,"latitude":41.7,"longitude":-86.0}
]"#;

const OTHERS_ONLY: &str = r#"[
 {"activator":"N9NEAR","frequency":"7200","mode":"SSB","reference":"US-0001",
  "spotTime":"2026-11-05T11:58:00","expire":600,"latitude":41.7,"longitude":-86.0}
]"#;

struct FakeClock(AtomicU64);
impl FakeClock {
    fn advance(&self, d: Duration) {
        self.0.fetch_add(d.as_millis() as u64, Ordering::SeqCst);
    }
}
impl Clock for FakeClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.load(Ordering::SeqCst))
    }
    fn unix(&self) -> u64 {
        NOW + self.0.load(Ordering::SeqCst) / 1000
    }
    fn sleep(&self, d: Duration) {
        self.advance(d);
    }
}

struct FakeTransport {
    body: &'static str,
    urls: Mutex<Vec<String>>,
}
impl Transport for FakeTransport {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, String> {
        self.urls.lock().unwrap().push(req.url.clone());
        Ok(HttpResponse {
            status: 200,
            body: self.body.as_bytes().to_vec(),
            ..Default::default()
        })
    }
}

struct Rig {
    app: App,
    client: Arc<ReferenceClient>,
    clock: Arc<FakeClock>,
    transport: Arc<FakeTransport>,
    now: Instant,
    dir: PathBuf,
}

impl Rig {
    fn new(body: &'static str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rig-tui-spotted-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("plans.toml"), PLANS).unwrap();
        let clock = Arc::new(FakeClock(AtomicU64::new(0)));
        let transport = Arc::new(FakeTransport {
            body,
            urls: Mutex::new(Vec::new()),
        });
        let client = Arc::new(ReferenceClient::start(
            Arc::clone(&transport) as Arc<dyn Transport>,
            Arc::clone(&clock) as Arc<dyn Clock>,
            Limits::default(),
            user_agent(Some("K9TST")),
            dir.join("cache"),
        ));
        let mut app = App::with_callsign(Some("K9TST".to_string()))
            .with_log_path(Some(dir.join("log.adi")))
            .with_locator(Some("EN71DQ".to_string()))
            .with_config_path(Some(dir.join("config.toml")));
        let watch = |a: Activator| a.with_spots(Arc::clone(&client), true, Some("K9TST"));
        app.host_activity(Box::new(watch(Activator::sota())));
        app.host_activity(Box::new(watch(Activator::pota())));
        app.set_utc(UNIX_EPOCH + Duration::from_secs(NOW));
        let mut app = connected_app_in(app, "CW");
        app.on_rig(RigEvent::Split {
            enabled: false,
            tx_vfo: "VFOA".to_string(),
        });
        Self {
            app,
            client,
            clock,
            transport,
            now: Instant::now(),
            dir,
        }
    }

    fn go_to(&mut self, name: &str) {
        self.app.on_key(ctrl('g'));
        type_str(&mut self.app, name);
        self.app.on_key(key(KeyCode::Enter));
    }

    /// Tick once a second for `secs`, letting the network thread finish each time.
    fn run(&mut self, secs: u64) {
        for _ in 0..secs {
            self.app.tick(self.now);
            self.client.sync();
            self.now += Duration::from_secs(1);
            self.clock.advance(Duration::from_secs(1));
            let elapsed = self.clock.now().as_secs();
            self.app
                .set_utc(UNIX_EPOCH + Duration::from_secs(NOW + elapsed));
        }
        self.app.tick(self.now);
    }

    fn requests(&self) -> usize {
        self.transport.urls.lock().unwrap().len()
    }

    fn screen(&self, w: u16, h: u16) -> Vec<String> {
        screen(&self.app, w, h)
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

#[test]
fn the_line_appears_only_when_the_call_is_in_the_spots() {
    let mut r = Rig::new(SPOTS);
    r.go_to("US-4571");
    r.run(3);
    let rows = r.screen(100, 30);
    assert!(
        has(&rows, "spotted: K9TST 14.062.500 CW by W9XYZ"),
        "{rows:#?}"
    );
    assert!(has(&rows, "40s ago") || has(&rows, "ago"), "{rows:#?}");

    let mut r = Rig::new(OTHERS_ONLY);
    r.go_to("US-4571");
    r.run(3);
    assert!(!has(&r.screen(100, 30), "spotted:"));
}

#[test]
fn sota_activators_ask_for_nothing() {
    let mut r = Rig::new(SPOTS);
    r.go_to("W7W/KG-001");
    r.run(120);
    assert_eq!(r.requests(), 0);
    assert!(!has(&r.screen(140, 32), "spotted:"));
}

#[test]
fn nearby_spots_come_with_30_rows_in_either_layout_and_list_nearest_first() {
    let mut r = Rig::new(SPOTS);
    r.go_to("US-4571");
    r.run(3);
    let wide = r.screen(140, 32);
    let near = wide
        .iter()
        .position(|l| l.contains("N9NEAR"))
        .expect("near");
    let far = wide.iter().position(|l| l.contains("K1FAR")).expect("far");
    assert!(near < far, "{wide:#?}");
    assert!(
        has(&r.screen(100, 32), "N9NEAR"),
        "one column stacks it under the band strip"
    );
    assert!(!has(&r.screen(140, 28), "N9NEAR"), "short");
}

#[test]
fn polling_keeps_the_60s_floor_and_stops_in_the_background() {
    let mut r = Rig::new(SPOTS);
    r.go_to("US-4571");
    r.run(59);
    assert_eq!(r.requests(), 1);
    r.run(3);
    assert_eq!(r.requests(), 2);
    r.go_to("General");
    r.run(400);
    assert_eq!(r.requests(), 2);
}
