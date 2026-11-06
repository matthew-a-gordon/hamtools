//! The SOTA Chaser Activity through the App, with a fake clock and a fake transport: no network,
//! no rig. The summit list is cached through the one shared client; the screen itself reads the
//! cache only and never asks for the SOTA API.

mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

use common::*;
use ratatui::crossterm::event::KeyCode;
use rig_tui::refdata::{ReferenceData, SUMMIT_LIST_URL};
use rig_tui::reference::{
    Clock, HttpRequest, HttpResponse, Limits, ReferenceClient, Transport, user_agent,
};
use rig_tui::{App, RigEvent, SOTA_SPOTS_NOTICE, SotaChaser};

/// 2026-11-04T12:03:20Z.
const NOW: u64 = 1_793_793_800;

const SUMMITS: &str = "SOTA Summits List (Date=04/11/2026)\n\
SummitCode,AssociationName,RegionName,SummitName,AltM,AltFt,GridRef1,GridRef2,Longitude,Latitude,Points,BonusPoints,ValidFrom,ValidTo,ActivationCount,ActivationDate,ActivationCall\n\
W4C/CM-001,\"USA - North Carolina\",\"Central Mountains\",\"Mount Mitchell\",2037,6685,,,-82.2652,35.7648,10,3,01/05/2002,31/12/2099,100,01/01/2025,K4XYZ\n\
W4C/CM-002,\"USA - North Carolina\",\"Central Mountains\",\"Clingmans Dome, North\",2025,6643,,,-83.4985,35.5628,10,3,01/05/2002,31/12/2099,0,,\n";

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

#[derive(Default)]
struct FakeTransport {
    urls: Mutex<Vec<String>>,
}
impl Transport for FakeTransport {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, String> {
        self.urls.lock().unwrap().push(req.url.clone());
        Ok(HttpResponse {
            status: 200,
            body: SUMMITS.as_bytes().to_vec(),
            ..Default::default()
        })
    }
}

struct Setup {
    app: App,
    client: Arc<ReferenceClient>,
    data: Arc<ReferenceData>,
    transport: Arc<FakeTransport>,
    chaser: usize,
}

impl Setup {
    fn new(online: bool) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir: PathBuf = std::env::temp_dir().join(format!(
            "rig-tui-sota-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let transport = Arc::new(FakeTransport::default());
        let client = Arc::new(ReferenceClient::start(
            Arc::clone(&transport) as Arc<dyn Transport>,
            Arc::new(FakeClock(AtomicU64::new(0))),
            Limits::default(),
            user_agent(Some("K9TST")),
            dir.join("cache"),
        ));
        let data = Arc::new(ReferenceData::new(Arc::clone(&client), dir.join("cache")));
        let mut app = App::with_callsign(Some("K9TST".to_string()))
            .with_log_path(Some(dir.join("log.adi")))
            .with_reference_data(Arc::clone(&data));
        app.set_utc(UNIX_EPOCH + Duration::from_secs(NOW));
        let chaser = app.host_activity(Box::new(SotaChaser::new(Arc::clone(&data), online)));
        let mut app = connected_app_in(app, "CW");
        app.on_rig(RigEvent::Split {
            enabled: false,
            tx_vfo: "VFOA".to_string(),
        });
        app.set_foreground(chaser);
        app.tick(Instant::now());
        Self {
            app,
            client,
            data,
            transport,
            chaser,
        }
    }

    /// Cache the summit list the way `r` does, and wait for the network thread.
    fn cached() -> Self {
        let s = Self::new(true);
        s.data.refresh_summits();
        s.client.sync();
        s
    }

    fn press(&mut self, c: char) {
        self.app.on_key(key(KeyCode::Char(c)));
    }

    fn rows(&self, w: u16, h: u16) -> Vec<String> {
        screen(&self.app, w, h)
    }
}

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

#[test]
fn the_screen_works_with_no_feed_and_says_why() {
    let s = Setup::new(true);
    let rows = s.rows(100, 30);
    assert!(has(&rows, "SOTA Chaser"), "{rows:#?}");
    assert!(has(&rows, SOTA_SPOTS_NOTICE), "{rows:#?}");
    assert!(has(&rows, "summit list not cached"), "{rows:#?}");
    // Entering the screen asked for nothing.
    s.client.sync();
    assert!(s.transport.urls.lock().unwrap().is_empty());
}

#[test]
fn the_notice_and_reference_box_fit_80x24() {
    let rows = Setup::new(true).rows(80, 24);
    assert!(has(&rows, SOTA_SPOTS_NOTICE), "{rows:#?}");
    assert!(has(&rows, "Summit:"), "{rows:#?}");
}

#[test]
fn typing_a_reference_looks_it_up_in_the_cached_list_only() {
    let mut s = Setup::cached();
    let before = s.transport.urls.lock().unwrap().len();
    s.press('/');
    for c in "w4c/cm-001".chars() {
        s.press(c);
    }
    let rows = s.rows(100, 30);
    assert!(has(&rows, "W4C/CM-001"), "{rows:#?}");
    assert!(has(&rows, "Mount Mitchell"), "{rows:#?}");
    assert!(has(&rows, "2037 m"), "{rows:#?}");
    assert!(has(&rows, "10 pts"), "{rows:#?}");
    assert!(has(&rows, "summit list 0d old"), "{rows:#?}");
    s.client.sync();
    assert_eq!(s.transport.urls.lock().unwrap().len(), before);
}

#[test]
fn any_word_of_the_name_finds_a_summit() {
    let mut s = Setup::cached();
    s.press('/');
    for c in "dome".chars() {
        s.press(c);
    }
    assert!(has(&s.rows(100, 30), "Clingmans Dome, North"));
}

#[test]
fn a_reference_in_no_cache_is_dim_not_cached_and_fetches_nothing() {
    let mut s = Setup::cached();
    let before = s.transport.urls.lock().unwrap().len();
    s.press('/');
    for c in "W9X/ZZ-999".chars() {
        s.press(c);
    }
    assert!(has(&s.rows(100, 30), "not cached"));
    s.client.sync();
    assert_eq!(s.transport.urls.lock().unwrap().len(), before);
}

#[test]
fn the_box_owns_printable_keys_and_esc_leaves_it() {
    let mut s = Setup::cached();
    s.press('/');
    // `f` would open frequency entry if the box did not own it.
    s.press('f');
    assert!(has(&s.rows(100, 30), "Summit: f_"));
    s.app.on_key(key(KeyCode::Esc));
    s.press('j');
    assert!(has(&s.rows(100, 30), "Summit: f"));
    assert!(!has(&s.rows(100, 30), "Summit: f_"));
}

#[test]
fn enter_opens_the_form_with_their_ref_and_never_tunes() {
    let mut s = Setup::cached();
    s.press('/');
    for c in "mitchell".chars() {
        s.press(c);
    }
    let commands = s.app.on_key(key(KeyCode::Enter));
    assert!(commands.is_empty(), "{commands:?}");
    let rows = s.rows(100, 30);
    assert!(has(&rows, "W4C/CM-001"), "{rows:#?}");
    assert!(has(&rows, "Their ref"), "{rows:#?}");
    // The Call is left empty for the operator to type.
    assert!(
        rows.iter().any(|r| r.contains("Call:")
            && r.trim_matches(|c: char| c == '│' || c == '┃' || c == ' ')
                .trim_end()
                == "Call:"),
        "{rows:#?}"
    );
}

#[test]
fn enter_with_nothing_typed_says_so() {
    let mut s = Setup::cached();
    s.app.on_key(key(KeyCode::Enter));
    assert!(s.app.status().is_some_and(|t| t.contains("summit")));
}

#[test]
fn r_refreshes_only_the_summit_list_data_file() {
    let mut s = Setup::new(true);
    s.press('r');
    s.client.sync();
    let urls = s.transport.urls.lock().unwrap().clone();
    assert_eq!(urls, vec![SUMMIT_LIST_URL.to_string()]);
    assert!(
        urls.iter().all(|u| !u.contains("api2.sota.org.uk")),
        "{urls:?}"
    );
}

#[test]
fn offline_mode_never_asks_the_network() {
    let mut s = Setup::new(false);
    s.press('r');
    s.client.sync();
    assert!(s.transport.urls.lock().unwrap().is_empty());
    assert!(s.app.status().is_some_and(|t| t.contains("offline")));
    let _ = s.chaser;
}

#[test]
fn only_a_soft_key_press_can_transmit_from_the_chaser() {
    let mut s = Setup::cached();
    assert_only_soft_keys_can_transmit(&mut s.app);
}

#[test]
fn the_chaser_shows_todays_contacts_with_the_width_or_height() {
    let s = Setup::new(true);
    assert!(has(&s.rows(128, 24), "Today's contacts"));
    assert!(has(&s.rows(83, 47), "Today's contacts"));
    assert!(!has(&s.rows(80, 24), "Today's contacts"));
}
