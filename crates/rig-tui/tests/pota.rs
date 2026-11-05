//! The POTA Hunter Activity through the App, with a fake clock and a fake transport: no network,
//! no rig, no real Log.

mod common;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

use chrono::{TimeZone, Utc};
use common::*;
use hamtools_config::LicenseClass;
use logbook::{Callsign, Contact, ContactInput, Log, Mode};
use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use rig_tui::reference::{
    Clock, HttpRequest, HttpResponse, Limits, ReferenceClient, Transport, user_agent,
};
use rig_tui::{Activity, ActivityCtx, ActivityOutput, App, Command, PotaHunter, RigEvent, RigMode};

/// 2026-11-04T12:03:20Z.
const NOW: u64 = 1_793_793_800;

/// Four spots. Ages at NOW: K1AAA 20 s (CW, 20 m, ~83 km west of EN71DQ), N3CCC 80 s (FT8),
/// AA4DDD 140 s (CW, just above 20 m, so outside every privilege) and W2BBB 200 s (SSB on 40 m).
const SPOTS: &str = r#"[
 {"activator":"K1AAA","frequency":"14062.5","mode":"CW","reference":"US-0001","parkName":"Alpha SP",
  "locationDesc":"US-IN","spotter":"W9XYZ","comments":"QRP","spotTime":"2026-11-04T12:03:00",
  "expire":600,"latitude":41.6875,"longitude":-86.7083},
 {"activator":"N3CCC","frequency":"14074","mode":"FT8","reference":"US-0003","parkName":"Gamma NP",
  "spotTime":"2026-11-04T12:02:00","expire":600,"latitude":35.0,"longitude":-100.0},
 {"activator":"AA4DDD","frequency":"14400","mode":"CW","reference":"US-0004","parkName":"Delta",
  "spotTime":"2026-11-04T12:01:00","expire":600,"latitude":45.0,"longitude":-90.0},
 {"activator":"W2BBB","frequency":"7200","mode":"SSB","reference":"US-0002","parkName":"Beta SF",
  "spotTime":"2026-11-04T12:00:00","expire":600,"latitude":40.0,"longitude":-80.0}
]"#;

struct FakeClock {
    millis: AtomicU64,
}
impl FakeClock {
    fn advance(&self, d: Duration) {
        self.millis
            .fetch_add(d.as_millis() as u64, Ordering::SeqCst);
    }
}
impl Clock for FakeClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.millis.load(Ordering::SeqCst))
    }
    fn unix(&self) -> u64 {
        NOW + self.millis.load(Ordering::SeqCst) / 1000
    }
    fn sleep(&self, d: Duration) {
        self.advance(d);
    }
}

#[derive(Default)]
struct FakeTransport {
    replies: Mutex<VecDeque<Result<HttpResponse, String>>>,
    urls: Mutex<Vec<String>>,
}
impl FakeTransport {
    fn requests(&self) -> usize {
        self.urls.lock().unwrap().len()
    }
}
impl Transport for FakeTransport {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, String> {
        self.urls.lock().unwrap().push(req.url.clone());
        self.replies.lock().unwrap().pop_front().unwrap_or_else(|| {
            Ok(HttpResponse {
                status: 200,
                body: SPOTS.as_bytes().to_vec(),
                ..Default::default()
            })
        })
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rig-tui-pota-{}-{}-{tag}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A tracking Activity kept in the background: only its tuning gate matters.
struct Tracking;
impl Activity for Tracking {
    fn name(&self) -> &'static str {
        "Tracking"
    }
    fn on_key(
        &mut self,
        _key: ratatui::crossterm::event::KeyEvent,
        _c: &ActivityCtx,
    ) -> Option<ActivityOutput> {
        None
    }
    fn on_tick(&mut self, _now: Instant, _ctx: &ActivityCtx) -> Vec<Command> {
        Vec::new()
    }
    fn on_reply(&mut self, _r: &str, _reply: Result<String, String>) -> Result<(), String> {
        Ok(())
    }
    fn on_mode(&mut self, _ctx: &ActivityCtx) {}
    fn text_focus(&self, _ctx: &ActivityCtx) -> bool {
        false
    }
    fn has_panel(&self, _ctx: &ActivityCtx) -> bool {
        false
    }
    fn log_hints(&self, _m: &RigMode) -> (Option<rig_tui::LogMode>, rig_tui::LogDefaults) {
        (None, rig_tui::LogDefaults::default())
    }
    fn hint(&self, _ctx: &ActivityCtx) -> &'static str {
        ""
    }
    fn render(&self, _f: &mut Frame, _a: Rect, _c: &ActivityCtx) {}
    fn tuning_blocked(&self) -> Option<&'static str> {
        Some("Doppler tracking")
    }
}

struct Setup {
    app: App,
    client: Arc<ReferenceClient>,
    clock: Arc<FakeClock>,
    transport: Arc<FakeTransport>,
    log: PathBuf,
    now: Instant,
    hunter: usize,
}

impl Setup {
    fn new() -> Self {
        Self::build(true, Some("EN71DQ"), Some(LicenseClass::General), &[])
    }

    fn build(
        online: bool,
        locator: Option<&str>,
        class: Option<LicenseClass>,
        my_refs: &[&str],
    ) -> Self {
        let dir = temp_dir("run");
        let clock = Arc::new(FakeClock {
            millis: AtomicU64::new(0),
        });
        let transport = Arc::new(FakeTransport::default());
        let client = Arc::new(ReferenceClient::start(
            Arc::clone(&transport) as Arc<dyn Transport>,
            Arc::clone(&clock) as Arc<dyn Clock>,
            Limits::default(),
            user_agent(Some("K9TST")),
            dir.join("cache"),
        ));
        let log = dir.join("log.adi");
        let mut app = App::with_callsign(Some("K9TST".to_string()))
            .with_log_path(Some(log.clone()))
            .with_locator(locator.map(String::from))
            .with_license_class(class);
        app.set_utc(UNIX_EPOCH + Duration::from_secs(NOW));
        let mut activity = PotaHunter::new(Arc::clone(&client), online);
        activity.set_my_refs(my_refs.iter().map(|s| s.to_string()).collect());
        let hunter = app.host_activity(Box::new(activity));
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
            log,
            now: Instant::now(),
            hunter,
        }
    }

    fn enter(&mut self) {
        self.app.set_foreground(self.hunter);
    }

    /// Tick, let the network thread finish, and tick again so the new snapshot is read.
    fn settle(&mut self) {
        self.app.tick(self.now);
        self.client.sync();
        self.now += Duration::from_secs(1);
        self.clock.advance(Duration::from_secs(1));
        self.app
            .set_utc(UNIX_EPOCH + Duration::from_secs(NOW + self.clock.now().as_secs()));
        self.app.tick(self.now);
    }

    fn entered() -> Self {
        let mut s = Self::new();
        s.enter();
        s.settle();
        s
    }

    fn press(&mut self, c: char) -> Vec<Command> {
        self.app.on_key(key(KeyCode::Char(c)))
    }

    fn screen(&self) -> Vec<String> {
        screen(&self.app, 100, 30)
    }

    /// The row of the list that carries `needle`, as its index among the screen rows.
    fn row_of(&self, needle: &str) -> Option<usize> {
        self.screen().iter().position(|r| r.contains(needle))
    }

    fn append_contact(&self, call: &str, hz: u64, mode: Mode) {
        let at = Utc.timestamp_opt(NOW as i64 - 600, 0).unwrap();
        let contact = Contact::new(ContactInput {
            call: call.to_string(),
            grid: None,
            name: None,
            comment: None,
            rst_sent: None,
            rst_rcvd: None,
            start: at,
            end: at,
            freq_hz: hz,
            mode,
            own_grid: None,
            station_callsign: None,
            extras: Vec::new(),
        })
        .unwrap();
        Log::new(&self.log, Callsign::new("K9TST").unwrap())
            .append(&contact)
            .unwrap();
    }
}

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

#[test]
fn the_list_shows_the_columns_newest_first() {
    let s = Setup::entered();
    let rows = s.screen();

    assert!(has(&rows, "POTA Hunter"), "{rows:#?}");
    for header in ["age", "call", "freq", "mode", "ref", "dist", "comment"] {
        assert!(has(&rows, header), "{header}: {rows:#?}");
    }
    let k = rows.iter().find(|r| r.contains("K1AAA")).unwrap();
    assert!(k.contains("21s"), "{k}");
    assert!(k.contains("14.062.500"), "{k}");
    assert!(k.contains("CW"), "{k}");
    assert!(k.contains("US-0001"), "{k}");
    assert!(
        k.contains("83km") || k.contains("82km") || k.contains("84km"),
        "{k}"
    );
    assert!(k.contains("QRP"), "{k}");

    let order: Vec<_> = ["K1AAA", "N3CCC", "AA4DDD", "W2BBB"]
        .iter()
        .map(|c| s.row_of(c).unwrap())
        .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?} {rows:#?}");
    // The first spot is selected, and the detail row describes it.
    assert!(
        has(&rows, "US-0001 Alpha SP (US-IN)  spotted by W9XYZ"),
        "{rows:#?}"
    );
    assert!(has(&rows, "4 of 4 spots  0 logged"), "{rows:#?}");
}

#[test]
fn without_a_position_the_distance_column_goes_and_d_says_why() {
    let mut s = Setup::build(true, None, None, &[]);
    s.enter();
    s.settle();
    assert!(!has(&s.screen(), "dist"), "{:#?}", s.screen());

    s.press('d');
    assert!(s.app.status().unwrap().contains("Locator"));
    assert!(!has(&s.screen(), "[nearest"));
}

#[test]
fn d_sorts_nearest_first_and_toggles_back() {
    let mut s = Setup::entered();
    // Nearest to EN71DQ: K1AAA (83 km), then AA4DDD, W2BBB, N3CCC by distance.
    s.press('d');
    let rows = s.screen();
    assert!(has(&rows, "[nearest first]"), "{rows:#?}");
    let order: Vec<_> = ["K1AAA", "AA4DDD", "W2BBB", "N3CCC"]
        .iter()
        .map(|c| s.row_of(c).unwrap())
        .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?} {rows:#?}");

    s.press('d');
    assert!(!has(&s.screen(), "nearest first"));
}

#[test]
fn markers_each_have_a_character_that_needs_no_colour() {
    let mut s = Setup::build(
        true,
        Some("EN71DQ"),
        Some(LicenseClass::General),
        &["US-9999"],
    );
    s.append_contact("K1AAA", 14_062_500, Mode::Cw);
    s.enter();
    s.settle();
    let rows = s.screen();
    let line = |call: &str| rows.iter().find(|r| r.contains(call)).unwrap().clone();

    // Worked today (call, band and mode group match) and a park-to-park candidate.
    assert!(line("K1AAA").contains("✓ "), "{}", line("K1AAA"));
    assert!(line("K1AAA").contains('P'));
    // The same call on another mode group would not count; N3CCC is not worked.
    assert!(!line("N3CCC").contains('✓'));
    // Stale (older than two minutes) and outside privileges.
    assert!(line("W2BBB").contains('~'), "{}", line("W2BBB"));
    assert!(
        line("AA4DDD").contains('~') && line("AA4DDD").contains('!'),
        "{}",
        line("AA4DDD")
    );
    assert!(!line("N3CCC").contains('!'));
}

#[test]
fn worked_today_comes_from_the_log_with_no_state_of_its_own() {
    let s = Setup::entered();
    assert!(!s.screen().iter().any(|r| r.contains('✓')));
    // A Contact appears in the Log by another route; the next frame shows it.
    s.append_contact(
        "N3CCC",
        14_074_000,
        Mode::Other {
            mode: "FT8".into(),
            submode: None,
        },
    );
    assert!(
        s.screen()
            .iter()
            .any(|r| r.contains("N3CCC") && r.contains('✓'))
    );
    // A different band is not the same Contact.
    let t = Setup::entered();
    t.append_contact("W2BBB", 14_250_000, Mode::Ssb(logbook::Sideband::Upper));
    assert!(!t.screen().iter().any(|r| r.contains('✓')));
}

#[test]
fn filters_are_announced_and_hide_the_rest() {
    let mut s = Setup::entered();
    s.append_contact("K1AAA", 14_062_500, Mode::Cw);

    s.press('b');
    let rows = s.screen();
    // 40 m is the lowest band with a spot.
    assert!(has(&rows, "[band 40m]"), "{rows:#?}");
    assert!(has(&rows, "W2BBB") && !has(&rows, "K1AAA"), "{rows:#?}");
    assert!(has(&rows, "1 of 4 spots"), "{rows:#?}");
    s.press('b');
    assert!(has(&s.screen(), "[band 20m]"));
    s.press('b');
    assert!(!has(&s.screen(), "[band"));

    s.press('o');
    let rows = s.screen();
    assert!(has(&rows, "[mode CW]"), "{rows:#?}");
    assert!(has(&rows, "K1AAA") && !has(&rows, "N3CCC") && !has(&rows, "W2BBB"));
    s.press('o');
    assert!(has(&s.screen(), "[mode phone]"));
    s.press('o');
    assert!(has(&s.screen(), "[mode digital]"));
    s.press('o');
    assert!(!has(&s.screen(), "[mode"));

    s.press('w');
    let rows = s.screen();
    assert!(has(&rows, "[hide worked]"), "{rows:#?}");
    assert!(!has(&rows, "K1AAA") && has(&rows, "N3CCC"), "{rows:#?}");
    s.press('w');
    assert!(has(&s.screen(), "K1AAA"));
}

#[test]
fn a_filter_that_matches_nothing_says_so() {
    let mut s = Setup::entered();
    s.press('o');
    s.press('o');
    s.press('o'); // digital: only N3CCC
    s.press('b'); // 40 m: nothing digital there
    let rows = s.screen();
    assert!(has(&rows, "No spots match the filters"), "{rows:#?}");
}

#[test]
fn j_and_k_move_the_selection_and_the_detail_row_follows() {
    let mut s = Setup::entered();
    s.press('j');
    assert!(has(&s.screen(), "US-0003 Gamma NP"), "{:#?}", s.screen());
    s.app.on_key(key(KeyCode::Down));
    assert!(has(&s.screen(), "US-0004 Delta"));
    s.press('k');
    assert!(has(&s.screen(), "US-0003 Gamma NP"));
    for _ in 0..10 {
        s.press('j');
    }
    assert!(has(&s.screen(), "US-0002 Beta SF"));
}

#[test]
fn enter_opens_the_form_on_the_call_and_ref_and_never_tunes() {
    let mut s = Setup::entered();
    s.press('j'); // N3CCC, FT8 on 14.074

    let commands = s.app.on_key(key(KeyCode::Enter));

    assert_eq!(commands, vec![], "selecting a spot must not touch the rig");
    assert!(s.app.has_overlay());
    let rows = s.screen();
    assert!(has(&rows, "N3CCC"), "{rows:#?}");
    assert!(has(&rows, "US-0003"), "{rows:#?}");
    assert!(has(&rows, "Their ref"), "{rows:#?}");
    // Esc closes without a prompt: the prefill is not typing.
    s.app.on_key(key(KeyCode::Esc));
    assert!(!s.app.has_overlay());
}

#[test]
fn the_prefilled_form_saves_a_contact_with_their_ref() {
    let mut s = Setup::entered();
    s.app.on_key(key(KeyCode::Enter));
    s.app.on_key(key(KeyCode::Enter));
    assert_eq!(s.app.logged_count(), 1, "{:#?}", s.screen());
    let text = std::fs::read_to_string(&s.log).unwrap();
    assert!(text.contains("K1AAA"), "{text}");
    assert!(text.contains("US-0001"), "{text}");
}

#[test]
fn t_tunes_frequency_and_mapped_mode_without_touching_split() {
    let mut s = Setup::entered();
    // K1AAA is CW and the rig is in CW already: frequency only.
    assert_eq!(s.press('t'), vec![Command::SetFreq(14_062_500)]);

    // W2BBB is SSB on 40 m: lower sideband, then the frequency.
    for _ in 0..3 {
        s.press('j');
    }
    assert_eq!(
        s.press('t'),
        vec![
            Command::SetMode {
                mode: RigMode::Lsb,
                passband: 0
            },
            Command::SetFreq(7_200_000)
        ]
    );
}

#[test]
fn t_on_an_unmapped_mode_tunes_frequency_only_with_a_note() {
    let mut s = Setup::entered();
    s.press('j'); // N3CCC, FT8
    assert_eq!(s.press('t'), vec![Command::SetFreq(14_074_000)]);
    assert!(
        s.app.status().unwrap().contains("FT8 not set"),
        "{:?}",
        s.app.status()
    );
}

#[test]
fn t_is_refused_while_doppler_tracking() {
    let mut setup = Setup::new();
    setup.app.host_activity(Box::new(Tracking));
    setup.enter();
    setup.settle();

    assert_eq!(setup.press('t'), vec![]);
    assert!(setup.app.status().unwrap().contains("Doppler tracking"));
    // Selecting and logging are still fine.
    assert_eq!(setup.app.on_key(key(KeyCode::Enter)), vec![]);
}

#[test]
fn t_needs_a_connected_rig() {
    let mut s = Setup::entered();
    s.app.on_rig(RigEvent::Disconnected("gone".into()));
    assert_eq!(s.press('t'), vec![]);
    assert!(s.app.status().unwrap().contains("not connected"));
}

#[test]
fn spots_are_polled_on_entry_then_no_faster_than_once_a_minute() {
    let mut s = Setup::new();
    s.enter();
    s.app.tick(s.now);
    s.client.sync();
    assert_eq!(s.transport.requests(), 1, "an immediate poll on entry");
    assert_eq!(s.transport.urls.lock().unwrap()[0], rig_tui::SPOTS_URL);

    // Ticking ten times a second for 59 seconds asks for nothing more.
    for _ in 0..590 {
        s.now += Duration::from_millis(100);
        s.clock.advance(Duration::from_millis(100));
        s.app.tick(s.now);
    }
    s.client.sync();
    assert_eq!(s.transport.requests(), 1);

    // Past the minute it polls again.
    for _ in 0..30 {
        s.now += Duration::from_millis(100);
        s.clock.advance(Duration::from_millis(100));
        s.app.tick(s.now);
    }
    s.client.sync();
    assert_eq!(s.transport.requests(), 2);
}

#[test]
fn nothing_is_polled_while_another_activity_is_in_the_foreground() {
    let mut s = Setup::new();
    // General is in the foreground: the hunter is never ticked.
    for _ in 0..50 {
        s.now += Duration::from_secs(10);
        s.clock.advance(Duration::from_secs(10));
        s.app.tick(s.now);
    }
    s.client.sync();
    assert_eq!(s.transport.requests(), 0);

    // Coming back after a long absence polls at once; leaving and returning inside the minute
    // does not poll again (the client's floor holds).
    s.enter();
    s.app.tick(s.now);
    s.client.sync();
    assert_eq!(s.transport.requests(), 1);
    s.app.set_foreground(0);
    s.enter();
    s.now += Duration::from_secs(5);
    s.clock.advance(Duration::from_secs(5));
    s.app.tick(s.now);
    s.client.sync();
    assert_eq!(s.transport.requests(), 1);
}

#[test]
fn offline_mode_never_asks_the_network() {
    let mut s = Setup::build(false, None, None, &[]);
    s.enter();
    for _ in 0..5 {
        s.settle();
    }
    assert_eq!(s.transport.requests(), 0);
    let rows = s.screen();
    assert!(has(&rows, "offline mode"), "{rows:#?}");
    assert!(has(&rows, "Spots are off"), "{rows:#?}");
    assert!(s.press('r').is_empty());
    assert!(s.app.status().unwrap().contains("off"));
}

#[test]
fn r_refreshes_now_or_says_when() {
    let mut s = Setup::entered();
    // The entry poll just ran; a refresh inside the minute is refused with the wait.
    s.press('r');
    let status = s.app.status().unwrap().to_string();
    assert!(status.starts_with("next in "), "{status}");
    assert!(status.ends_with('s'), "{status}");
    assert_eq!(s.transport.requests(), 1);
    assert!(has(&s.screen(), "next in"), "{:#?}", s.screen());

    // After the floor, `r` goes through.
    s.clock.advance(Duration::from_secs(61));
    s.press('r');
    s.client.sync();
    assert_eq!(s.transport.requests(), 2);
}

#[test]
fn a_failing_feed_shows_one_offline_indicator_with_the_last_success() {
    let mut s = Setup::new();
    s.enter();
    s.settle();
    assert!(!has(&s.screen(), "offline"));

    s.transport
        .replies
        .lock()
        .unwrap()
        .push_back(Err("connection refused".to_string()));
    s.clock.advance(Duration::from_secs(61));
    s.now += Duration::from_secs(61);
    s.app
        .set_utc(UNIX_EPOCH + Duration::from_secs(NOW + s.clock.now().as_secs()));
    s.settle();
    s.settle();

    let rows = s.screen();
    assert_eq!(
        rows.iter().filter(|r| r.contains("offline")).count(),
        1,
        "{rows:#?}"
    );
    assert!(has(&rows, "last ok 12:03:2"), "{rows:#?}");
    // The spots already held stay on screen (until they expire).
    assert!(has(&rows, "K1AAA"));
}

#[test]
fn spots_drop_at_their_expiry() {
    let mut s = Setup::entered();
    assert!(has(&s.screen(), "W2BBB"));
    // 601 s later none of the (600 s) spots is left, though the feed has not been re-read.
    s.app.set_utc(UNIX_EPOCH + Duration::from_secs(NOW + 605));
    let rows = s.screen();
    assert!(!has(&rows, "K1AAA") && !has(&rows, "W2BBB"), "{rows:#?}");
    assert!(has(&rows, "No spots right now"), "{rows:#?}");
}

#[test]
fn it_fits_the_smallest_screen_with_the_cw_panel() {
    let mut s = Setup::entered();
    let rows = screen(&s.app, 80, 24);
    assert!(has(&rows, "POTA Hunter"), "{rows:#?}");
    assert!(has(&rows, "K1AAA"), "{rows:#?}");
    assert!(
        has(&rows, "Send (Enter)"),
        "the Mode panel is on the screen: {rows:#?}"
    );
    assert!(has(&rows, "logged"), "{rows:#?}");
    assert!(rows.iter().all(|r| r.chars().count() <= 80));
    // Tab moves into the CW input and `j` then types instead of selecting.
    s.app.on_key(key(KeyCode::Tab));
    s.press('j');
    let after = screen(&s.app, 80, 24);
    assert!(has(&after, "> J"), "{after:#?}");
}

#[test]
fn the_palette_lists_the_hunter_and_its_keys_pass_the_reserved_key_check() {
    let s = Setup::new();
    assert_eq!(s.app.check_activity_keys(), Ok(()));
}

#[test]
fn slash_filters_the_list_by_any_word_and_esc_clears_it() {
    let mut s = Setup::entered();

    s.press('/');
    // The search line owns printable keys, so `j` types instead of selecting.
    for c in "gamma np".chars() {
        s.press(c);
    }
    let rows = s.screen();
    assert!(has(&rows, "N3CCC"), "{rows:#?}");
    assert!(!has(&rows, "K1AAA") && !has(&rows, "W2BBB"), "{rows:#?}");
    assert!(has(&rows, "[search gamma np]"), "{rows:#?}");

    s.app.on_key(key(KeyCode::Enter));
    assert!(has(&s.screen(), "[search gamma np]"));
    s.press('/');
    s.app.on_key(key(KeyCode::Esc));
    let rows = s.screen();
    assert!(has(&rows, "K1AAA") && !has(&rows, "[search"), "{rows:#?}");
}

#[test]
fn the_hunter_shows_todays_contacts_with_the_width_or_height() {
    let s = Setup::entered();
    s.append_contact("W1AW", 14_062_500, Mode::Cw);
    let wide = screen(&s.app, 128, 24);
    assert!(has(&wide, "Today's contacts"), "{wide:#?}");
    let tall = screen(&s.app, 83, 47);
    assert!(has(&tall, "Today's contacts"), "{tall:#?}");
    let small = screen(&s.app, 80, 24);
    assert!(!has(&small, "Today's contacts"), "{small:#?}");
}
