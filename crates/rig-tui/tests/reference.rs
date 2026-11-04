//! The ReferenceClient through its public interface, with a fake clock and a fake transport.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rig_tui::reference::*;

struct FakeClock {
    millis: AtomicU64,
}
impl Clock for FakeClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.millis.load(Ordering::SeqCst))
    }
    fn unix(&self) -> u64 {
        1_000_000 + self.millis.load(Ordering::SeqCst) / 1000
    }
    fn sleep(&self, d: Duration) {
        self.millis
            .fetch_add(d.as_millis() as u64, Ordering::SeqCst);
    }
}

type Reply = Result<HttpResponse, String>;

struct FakeTransport {
    clock: Arc<FakeClock>,
    replies: Mutex<VecDeque<Reply>>,
    seen: Mutex<Vec<(Duration, HttpRequest)>>,
}
impl Transport for FakeTransport {
    fn get(&self, req: &HttpRequest) -> Reply {
        self.seen
            .lock()
            .unwrap()
            .push((self.clock.now(), req.clone()));
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(ok("default")))
    }
}

fn ok(body: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        body: body.as_bytes().to_vec(),
        ..Default::default()
    }
}
fn status(code: u16) -> HttpResponse {
    HttpResponse {
        status: code,
        ..Default::default()
    }
}

struct Rig {
    clock: Arc<FakeClock>,
    transport: Arc<FakeTransport>,
    client: ReferenceClient,
    dir: PathBuf,
}

fn rig_with(name: &str, limits: Limits, replies: Vec<Reply>) -> Rig {
    let dir = std::env::temp_dir().join(format!("hamtools-ref-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let clock = Arc::new(FakeClock {
        millis: AtomicU64::new(0),
    });
    let transport = Arc::new(FakeTransport {
        clock: clock.clone(),
        replies: Mutex::new(replies.into()),
        seen: Mutex::new(vec![]),
    });
    let client = ReferenceClient::start(
        transport.clone(),
        clock.clone(),
        limits,
        user_agent(Some("W9XYZ")),
        dir.clone(),
    );
    Rig {
        clock,
        transport,
        client,
        dir,
    }
}
fn rig(name: &str, replies: Vec<Reply>) -> Rig {
    rig_with(name, Limits::default(), replies)
}
impl Rig {
    fn advance(&self, secs: u64) {
        self.clock.sleep(Duration::from_secs(secs));
    }
    fn count(&self) -> usize {
        self.transport.seen.lock().unwrap().len()
    }
    fn go(&self, feed: &Feed) -> Submit {
        let s = self.client.request(feed);
        self.client.sync();
        s
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn spots() -> Feed {
    Feed::new("pota-spots", "https://example.test/spots", Pace::Spots)
}
fn parks() -> Feed {
    Feed::new("pota-parks", "https://example.test/parks", Pace::OnDemand)
}

#[test]
fn spots_are_allowed_once_per_60_seconds() {
    let r = rig("spots60", vec![]);
    assert_eq!(r.go(&spots()), Submit::Queued);
    r.advance(59);
    assert_eq!(
        r.client.request(&spots()),
        Submit::TooSoon(Duration::from_secs(1))
    );
    r.advance(1);
    assert_eq!(r.go(&spots()), Submit::Queued);
    assert_eq!(r.count(), 2);
}

#[test]
fn scheduled_activations_are_allowed_once_per_5_minutes() {
    let r = rig("act5", vec![]);
    let f = Feed::new(
        "pota-activations",
        "https://example.test/a",
        Pace::Activations,
    );
    r.go(&f);
    r.advance(299);
    assert!(matches!(r.client.request(&f), Submit::TooSoon(_)));
    r.advance(1);
    assert_eq!(r.go(&f), Submit::Queued);
    assert_eq!(r.count(), 2);
}

#[test]
fn any_two_requests_are_at_least_one_second_apart() {
    let r = rig("gap", vec![]);
    r.client.request(&spots());
    r.client.request(&parks());
    r.client
        .request(&Feed::new("p2", "https://example.test/p2", Pace::OnDemand));
    r.client.sync();
    let seen = r.transport.seen.lock().unwrap();
    assert_eq!(seen.len(), 3);
    for w in seen.windows(2) {
        assert!(w[1].0 - w[0].0 >= Duration::from_secs(1));
    }
}

#[test]
fn a_5xx_backs_off_60s_then_doubles_up_to_an_hour() {
    let mut replies: Vec<Reply> = (0..9).map(|_| Ok(status(503))).collect();
    replies.push(Ok(ok("fine")));
    let r = rig("backoff", replies);
    let f = parks();
    let mut expected = 60u64;
    for _ in 0..9 {
        assert_eq!(r.go(&f), Submit::Queued);
        let st = r.client.status(&f);
        assert_eq!(st.next_in, Duration::from_secs(expected));
        assert_eq!(st.last_error.as_deref(), Some("HTTP 503"));
        r.advance(expected);
        expected = (expected * 2).min(3600);
    }
    assert_eq!(expected, 3600);
    assert_eq!(r.go(&f), Submit::Queued);
    let st = r.client.status(&f);
    assert_eq!((st.consecutive_failures, st.last_error), (0, None));
    assert_eq!(
        r.client.snapshot("pota-parks").unwrap().body.as_str(),
        "fine"
    );
}

#[test]
fn retry_after_longer_than_the_backoff_is_honoured() {
    let mut resp = status(429);
    resp.retry_after = Some(Duration::from_secs(600));
    let r = rig("retryafter", vec![Ok(resp)]);
    r.go(&parks());
    assert_eq!(r.client.status(&parks()).next_in, Duration::from_secs(600));
}

#[test]
fn a_404_is_reported_but_does_not_back_off() {
    let r = rig("e404", vec![Ok(status(404))]);
    r.go(&parks());
    let st = r.client.status(&parks());
    assert_eq!(st.last_error.as_deref(), Some("HTTP 404"));
    assert_eq!(st.next_in, Duration::ZERO);
}

#[test]
fn a_transport_error_backs_off_and_keeps_old_data() {
    let r = rig("neterr", vec![Ok(ok("old")), Err("timed out".into())]);
    r.go(&parks());
    r.go(&parks());
    assert_eq!(
        r.client.snapshot("pota-parks").unwrap().body.as_str(),
        "old"
    );
    let st = r.client.status(&parks());
    assert_eq!(st.last_error.as_deref(), Some("timed out"));
    assert_eq!(st.next_in, Duration::from_secs(60));
}

#[test]
fn requests_carry_user_agent_and_timeouts() {
    let r = rig("ua", vec![]);
    r.go(&parks());
    let seen = r.transport.seen.lock().unwrap();
    let req = &seen[0].1;
    assert!(req.user_agent.starts_with("hamtools/"));
    assert!(
        req.user_agent
            .contains("https://github.com/matthew-a-gordon/hamtools")
    );
    assert!(req.user_agent.contains("W9XYZ"));
    assert_eq!(req.connect_timeout, Duration::from_secs(5));
    assert_eq!(req.total_timeout, Duration::from_secs(15));
}

#[test]
fn user_agent_omits_an_unset_callsign() {
    let ua = user_agent(None);
    assert!(ua.ends_with("hamtools)"), "{ua}");
}

#[test]
fn success_is_cached_to_disk_with_fetched_at_and_read_back_by_a_new_client() {
    let r = rig("disk", vec![Ok(ok("PARKS"))]);
    r.go(&parks());
    let file = r.dir.join("pota-parks.json");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(v["fetched_at"], 1_000_000);
    let fresh = ReferenceClient::start(
        r.transport.clone(),
        r.clock.clone(),
        Limits::default(),
        user_agent(None),
        r.dir.clone(),
    );
    let s = fresh.snapshot("pota-parks").unwrap();
    assert_eq!((s.body.as_str(), s.fetched_at), ("PARKS", 1_000_000));
    assert_eq!(r.count(), 1, "reading the cache makes no request");
    assert!(fresh.snapshot("nothing-here").is_none());
}

#[test]
fn conditional_requests_use_validators_and_304_keeps_the_body() {
    let mut first = ok("LIST");
    first.etag = Some("\"v1\"".into());
    first.last_modified = Some("Tue, 03 Nov 2026 00:00:00 GMT".into());
    let r = rig("cond", vec![Ok(first), Ok(status(304))]);
    let f = Feed::new("sota-summits", "https://example.test/s", Pace::Daily);
    r.go(&f);
    r.advance(24 * 3600);
    r.go(&f);
    {
        let seen = r.transport.seen.lock().unwrap();
        assert_eq!(seen[0].1.if_none_match, None);
        assert_eq!(seen[1].1.if_none_match.as_deref(), Some("\"v1\""));
        assert_eq!(
            seen[1].1.if_modified_since.as_deref(),
            Some("Tue, 03 Nov 2026 00:00:00 GMT")
        );
    }
    let s = r.client.snapshot("sota-summits").unwrap();
    assert_eq!(s.body.as_str(), "LIST");
    assert_eq!(s.fetched_at, 1_000_000 + 24 * 3600);
}

#[test]
fn the_summit_list_is_allowed_once_a_day() {
    let r = rig("daily", vec![]);
    let f = Feed::new("sota-summits", "https://example.test/s", Pace::Daily);
    r.go(&f);
    r.advance(24 * 3600 - 1);
    assert!(matches!(r.client.request(&f), Submit::TooSoon(_)));
}

#[test]
fn config_can_slow_the_limits_but_never_speed_them() {
    let fast = Limits::slowed(
        Some(Duration::from_secs(1)),
        Some(Duration::from_secs(1)),
        Some(Duration::ZERO),
    );
    assert_eq!(fast, Limits::default());
    let r = rig_with("fast", fast, vec![]);
    r.go(&spots());
    r.advance(30);
    assert!(matches!(r.client.request(&spots()), Submit::TooSoon(_)));

    let slow = rig_with(
        "slow",
        Limits::slowed(Some(Duration::from_secs(120)), None, None),
        vec![],
    );
    slow.go(&spots());
    slow.advance(100);
    assert_eq!(
        slow.client.request(&spots()),
        Submit::TooSoon(Duration::from_secs(20))
    );
}

/// A transport that parks until released, standing in for a stuck connection.
struct Stuck {
    entered: Mutex<mpsc::Sender<()>>,
    release: Mutex<mpsc::Receiver<()>>,
}
impl Transport for Stuck {
    fn get(&self, _: &HttpRequest) -> Reply {
        self.entered.lock().unwrap().send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
        Err("timed out".into())
    }
}

#[test]
fn a_stuck_fetch_never_blocks_readers() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let clock = Arc::new(FakeClock {
        millis: AtomicU64::new(0),
    });
    let dir = std::env::temp_dir().join(format!("hamtools-ref-stuck-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let client = ReferenceClient::start(
        Arc::new(Stuck {
            entered: Mutex::new(entered_tx),
            release: Mutex::new(release_rx),
        }),
        clock,
        Limits::default(),
        user_agent(None),
        dir.clone(),
    );
    assert_eq!(client.request(&parks()), Submit::Queued);
    entered_rx.recv().unwrap();
    // The fetch is in flight; none of these may wait for it.
    assert!(client.snapshot("pota-parks").is_none());
    let _ = client.status(&parks());
    assert!(matches!(
        client.request(&parks()),
        Submit::Queued | Submit::TooSoon(_)
    ));
    release_tx.send(()).unwrap();
    client.sync();
    assert_eq!(
        client.status(&parks()).last_error.as_deref(),
        Some("timed out")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
