//! Reference data (parks and summits): parsers, offline search and lookup, and fetch policy,
//! through the public interface with a fake clock and transport. No network.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rig_tui::refdata::*;
use rig_tui::reference::*;

const SUMMITS: &str = "SOTA Summits List (Date=04/11/2026)\n\
SummitCode,AssociationName,RegionName,SummitName,AltM,AltFt,GridRef1,GridRef2,Longitude,Latitude,Points,BonusPoints,ValidFrom,ValidTo,ActivationCount,ActivationDate,ActivationCall\n\
W4C/CM-001,\"USA - North Carolina\",\"Central Mountains\",\"Mount Mitchell\",2037,6685,,,-82.2652,35.7648,10,3,01/05/2002,31/12/2099,100,01/01/2025,K4XYZ\n\
W4C/CM-002,\"USA - North Carolina\",\"Central Mountains\",\"Clingmans Dome, North\",2025,6643,,,-83.4985,35.5628,10,3,01/05/2002,31/12/2099,0,,\n\
W4C/CM-003,\"USA - North Carolina\",\"Central Mountains\",\"Old Summit\",1000,3280,,,-82.0,35.0,6,0,01/05/2002,01/01/2010,0,,\n";

const PARKS: &str = r#"[
 {"reference":"US-4571","name":"Potato Creek State Park","latitude":41.5546,"longitude":-86.3697,"grid":"EN61","locationDesc":"US-IN"},
 {"reference":"US-1234","name":"Pigeon River Fish and Wildlife Area","latitude":"41.65","longitude":-85.4,"grid":"EN71","locationDesc":"US-IN"}
]"#;

const DETAIL: &str = r#"{"reference":"US-4571","name":"Potato Creek State Park","latitude":41.5546,"longitude":-86.3697,"grid6":"EN61uo","locationDesc":"US-IN","parktypeDesc":"State Park"}"#;

#[test]
fn summit_list_parses_quoted_commas() {
    let s = parse_summit_list(SUMMITS).unwrap();
    assert_eq!(s.len(), 3);
    assert_eq!(s[0].reference, "W4C/CM-001");
    assert_eq!(s[0].name, "Mount Mitchell");
    assert_eq!(s[0].alt_m, Some(2037));
    assert_eq!(s[0].points, Some(10));
    assert_eq!(s[1].name, "Clingmans Dome, North");
    assert_eq!(s[2].valid_to, Some(20100101));
    assert!((s[0].lat.unwrap() - 35.7648).abs() < 1e-9);
    assert!((s[0].lon.unwrap() + 82.2652).abs() < 1e-9);
}

#[test]
fn summit_list_with_a_changed_header_fails_closed() {
    let bad = SUMMITS.replace("SummitName,AltM", "Name,AltM");
    let e = parse_summit_list(&bad).unwrap_err();
    assert!(e.contains("header"), "{e}");
    let bad = SUMMITS.replace("SOTA Summits List", "Something else");
    assert!(parse_summit_list(&bad).is_err());
    assert!(parse_summit_list("").is_err());
    assert!(parse_summit_list("<html>blocked</html>").is_err());
}

#[test]
fn park_list_and_detail_parse_defensively() {
    let p = parse_park_list(PARKS).unwrap();
    assert_eq!(p.len(), 2);
    assert_eq!(p[0].reference, "US-4571");
    assert_eq!(p[0].grid.as_deref(), Some("EN61"));
    assert_eq!(p[1].lat, Some(41.65)); // a number sent as a string
    let d = parse_park_detail(DETAIL).unwrap().unwrap();
    assert_eq!(d.grid.as_deref(), Some("EN61uo"));
    assert_eq!(d.location.as_deref(), Some("US-IN"));
    // Unknown shapes are errors, not panics; missing optional fields are fine.
    assert!(parse_park_list("{\"nope\":1}").is_err());
    assert!(parse_park_detail("[]").is_err());
    assert_eq!(
        parse_park_detail(r#"{"reference":"K-0039","name":"X"}"#)
            .unwrap()
            .unwrap()
            .grid,
        None
    );
}

struct FakeClock(AtomicU64);
impl Clock for FakeClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.load(Ordering::SeqCst))
    }
    fn unix(&self) -> u64 {
        1_800_000_000 + self.0.load(Ordering::SeqCst) / 1000
    }
    fn sleep(&self, d: Duration) {
        self.0.fetch_add(d.as_millis() as u64, Ordering::SeqCst);
    }
}

struct Routes {
    urls: Mutex<Vec<String>>,
    calls: AtomicUsize,
}
impl Transport for Routes {
    fn get(&self, req: &HttpRequest) -> Result<HttpResponse, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.urls.lock().unwrap().push(req.url.clone());
        let body = if req.url.contains("summitslist") {
            SUMMITS
        } else if req.url.contains("/location/parks/") {
            PARKS
        } else if req.url.contains("/park/") {
            DETAIL
        } else {
            return Err("unexpected url".into());
        };
        Ok(HttpResponse {
            status: 200,
            body: body.as_bytes().to_vec(),
            ..Default::default()
        })
    }
}

struct Offline;
impl Transport for Offline {
    fn get(&self, _: &HttpRequest) -> Result<HttpResponse, String> {
        panic!("a lookup or search touched the network");
    }
}

struct Fx {
    data: ReferenceData,
    client: Arc<ReferenceClient>,
    routes: Arc<Routes>,
    clock: Arc<FakeClock>,
    dir: PathBuf,
}

fn fx(name: &str) -> Fx {
    let dir = std::env::temp_dir().join(format!("hamtools-refdata-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let clock = Arc::new(FakeClock(AtomicU64::new(0)));
    let routes = Arc::new(Routes {
        urls: Mutex::new(vec![]),
        calls: AtomicUsize::new(0),
    });
    let client = Arc::new(ReferenceClient::start(
        routes.clone(),
        clock.clone(),
        Limits::default(),
        user_agent(None),
        dir.clone(),
    ));
    let data = ReferenceData::new(client.clone(), dir.clone());
    Fx {
        data,
        client,
        routes,
        clock,
        dir,
    }
}
impl Fx {
    fn calls(&self) -> usize {
        self.routes.calls.load(Ordering::SeqCst)
    }
    fn now(&self) -> u64 {
        self.clock.unix()
    }
}

// ---- Live shapes from docs/research/missing-data.md section 3 (2026-11-04) ----

fn client_over(
    t: Arc<dyn Transport>,
    tag: &str,
) -> (Arc<ReferenceClient>, ReferenceData, Arc<FakeClock>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("hamtools-refdata-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let clock = Arc::new(FakeClock(AtomicU64::new(0)));
    let client = Arc::new(ReferenceClient::start(
        t,
        clock.clone(),
        Limits::default(),
        user_agent(None),
        dir.clone(),
    ));
    let data = ReferenceData::new(client.clone(), dir.clone());
    (client, data, clock, dir)
}

#[test]
fn the_live_17_column_summit_list_parses() {
    let live = "SOTA Summits List (Date=04/11/2026)\n\
SummitCode,AssociationName,RegionName,SummitName,AltM,AltFt,GridRef1,GridRef2,Longitude,Latitude,Points,BonusPoints,ValidFrom,ValidTo,ActivationCount,ActivationDate,ActivationCall\n\
3B8/MU-001,Mauritius,Mauritius,\"Piton de la Petite Rivi\u{e8}re Noire (Black River Peak)\",828,2716,57.40771,-20.40882,57.40771,-20.40882,10,0,01/08/2026,31/12/2099,0,,\n\
W9/IL-001,\"USA - W9\",Illinois,\"Williams Hill\",324,1064,-88.4756,37.5823,-88.47560,37.58230,1,0,01/07/2013,31/12/2099,40,23/01/2026,K8ARE\n";
    let s = parse_summit_list(live).unwrap();
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].reference, "3B8/MU-001");
    assert_eq!(s[0].alt_m, Some(828));
    assert!((s[0].lon.unwrap() - 57.40771).abs() < 1e-9);
    assert!((s[0].lat.unwrap() + 20.40882).abs() < 1e-9);
    assert_eq!(s[1].name, "Williams Hill");
    assert_eq!(s[1].points, Some(1));
    assert_eq!(s[1].valid_to, Some(20991231));
}

#[test]
fn a_park_detail_of_null_is_unknown_not_an_error() {
    // `GET /park/K-0001` answers HTTP 200 with the body `null`.
    assert_eq!(parse_park_detail("null"), Ok(None));
    assert_eq!(parse_park_detail("  null\n"), Ok(None));
    assert!(parse_park_detail(DETAIL).unwrap().is_some());
}

#[test]
fn a_null_park_reply_leaves_the_lookup_not_cached_and_no_error() {
    struct NullPark;
    impl Transport for NullPark {
        fn get(&self, _: &HttpRequest) -> Result<HttpResponse, String> {
            Ok(HttpResponse {
                status: 200,
                body: b"null".to_vec(),
                ..Default::default()
            })
        }
    }
    let (client, data, clock, dir) = client_over(Arc::new(NullPark), "null");
    data.refresh_park("K-0001", clock.unix());
    client.sync();
    assert_eq!(data.lookup("K-0001", clock.unix()), Lookup::NotCached);
    assert!(data.search("K-0001", clock.unix(), 5).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_location_of_sixteen_states_is_kept_whole() {
    let desc = [
        "US-ID", "US-MT", "US-OR", "US-WA", "US-WY", "US-ND", "US-SD", "US-NE", "US-IA", "US-MO",
        "US-IL", "US-IN", "US-OH", "US-KY", "US-TN", "US-VA",
    ]
    .join(",");
    let json = format!(
        r#"[{{"reference":"US-4572","name":"Lewis and Clark National Historic Trail","locationDesc":"{desc}","grid":"DN36ss"}}]"#
    );
    let p = parse_park_list(&json).unwrap();
    assert_eq!(p[0].location.as_deref(), Some(desc.as_str()));
    assert_eq!(p[0].location.as_deref().unwrap().split(',').count(), 16);
}

#[test]
fn the_summit_list_download_follows_a_redirect_to_another_host() {
    struct Redirecting(Mutex<Vec<String>>);
    impl Transport for Redirecting {
        fn get(&self, req: &HttpRequest) -> Result<HttpResponse, String> {
            self.0.lock().unwrap().push(req.url.clone());
            if req.url.starts_with("https://www.sotadata.org.uk/") {
                Ok(HttpResponse {
                    status: 302,
                    location: Some("https://storage.sota.org.uk/summitslist.csv".into()),
                    ..Default::default()
                })
            } else {
                Ok(HttpResponse {
                    status: 200,
                    body: SUMMITS.as_bytes().to_vec(),
                    ..Default::default()
                })
            }
        }
    }
    let t = Arc::new(Redirecting(Mutex::new(vec![])));
    let (client, data, clock, dir) = client_over(t.clone(), "302");
    data.refresh_summits();
    client.sync();
    assert_eq!(
        *t.0.lock().unwrap(),
        [SUMMIT_LIST_URL, "https://storage.sota.org.uk/summitslist.csv"]
    );
    assert!(matches!(
        data.lookup("W4C/CM-001", clock.unix()),
        Lookup::Found(_)
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nothing_is_fetched_at_startup_or_by_a_lookup() {
    let f = fx("startup");
    assert_eq!(f.calls(), 0);
    assert!(matches!(
        f.data.lookup("US-4571", f.now()),
        Lookup::NotCached
    ));
    assert!(matches!(
        f.data.lookup("W4C/CM-001", f.now()),
        Lookup::NotCached
    ));
    assert!(f.data.search("potato", f.now(), 10).is_empty());
    f.client.sync();
    assert_eq!(f.calls(), 0);
    let _ = std::fs::remove_dir_all(&f.dir);
}

#[test]
fn refresh_then_search_works_offline_and_reports_age() {
    let f = fx("search");
    f.data.refresh_summits();
    f.data.refresh_park_list("US-IN");
    f.client.sync();
    assert_eq!(f.calls(), 2);

    f.clock.sleep(Duration::from_secs(3 * 24 * 3600));
    // A restarted app: a new client over the same cache directory, and a transport that panics.
    let offline = Arc::new(ReferenceClient::start(
        Arc::new(Offline),
        f.clock.clone(),
        Limits::default(),
        user_agent(None),
        f.dir.clone(),
    ));
    let data = ReferenceData::new(offline, f.dir.clone());

    let hits = data.search("potato state", f.now(), 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].entry.reference, "US-4571");
    assert_eq!(hits[0].age_secs, 3 * 24 * 3600);
    assert!(!hits[0].stale);

    let by_ref = data.search("cm-001", f.now(), 10);
    assert_eq!(by_ref[0].entry.name, "Mount Mitchell");
    let by_word = data.search("dome", f.now(), 10);
    assert_eq!(by_word[0].entry.reference, "W4C/CM-002");
    assert!(data.search("old summit", f.now(), 10).is_empty()); // retired

    match data.lookup("us-1234", f.now()) {
        Lookup::Found(h) => assert_eq!(h.entry.name, "Pigeon River Fish and Wildlife Area"),
        Lookup::NotCached => panic!("should be cached"),
    }
    assert!(matches!(data.lookup("US-9999", f.now()), Lookup::NotCached));
    let _ = std::fs::remove_dir_all(&f.dir);
}

#[test]
fn data_older_than_thirty_days_is_flagged_but_still_found() {
    let f = fx("stale");
    f.data.refresh_park_list("US-IN");
    f.client.sync();
    f.clock.sleep(Duration::from_secs(31 * 24 * 3600));
    let hits = f.data.search("potato", f.now(), 10);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].stale);
    let _ = std::fs::remove_dir_all(&f.dir);
}

#[test]
fn first_lookup_fetches_once_and_only_when_nothing_is_cached() {
    let f = fx("first");
    assert!(matches!(
        f.data.lookup_or_fetch("US-4571", f.now()),
        Lookup::NotCached
    ));
    f.client.sync();
    assert_eq!(f.calls(), 1);
    assert!(f.routes.urls.lock().unwrap()[0].ends_with("/park/US-4571"));
    assert!(matches!(
        f.data.lookup_or_fetch("US-4571", f.now()),
        Lookup::Found(_)
    ));
    f.client.sync();
    assert_eq!(f.calls(), 1);
    let _ = std::fs::remove_dir_all(&f.dir);
}

#[test]
fn park_detail_is_not_refetched_within_a_day() {
    let f = fx("detail");
    f.data.refresh_park("US-4571", f.now());
    f.client.sync();
    f.data.refresh_park("US-4571", f.now());
    f.client.sync();
    assert_eq!(f.calls(), 1);
    f.clock.sleep(Duration::from_secs(25 * 3600));
    f.data.refresh_park("US-4571", f.now());
    f.client.sync();
    assert_eq!(f.calls(), 2);
    let _ = std::fs::remove_dir_all(&f.dir);
}

#[test]
fn only_the_summit_list_and_pota_urls_are_ever_requested() {
    let f = fx("urls");
    f.data.refresh_summits();
    f.data.refresh_park_list("US-IN");
    f.data.refresh_park("US-4571", f.now());
    f.data.lookup_or_fetch("W4C/CM-009", f.now());
    f.client.sync();
    for u in f.routes.urls.lock().unwrap().iter() {
        assert!(
            u.starts_with("https://api.pota.app/")
                || u == "https://www.sotadata.org.uk/summitslist.csv",
            "{u}"
        );
    }
    let _ = std::fs::remove_dir_all(&f.dir);
}

#[test]
fn offline_data_never_asks_the_client_for_anything() {
    let mut f = fx("offline");
    f.data = ReferenceData::new(f.client.clone(), f.dir.clone()).with_offline(true);
    f.data.refresh_summits();
    f.data.refresh_park_list("US-IN");
    f.data.refresh_park("US-4571", f.now());
    let _ = f.data.lookup_or_fetch("US-4571", f.now());
    let _ = f.data.lookup_or_fetch("W4C/CM-001", f.now());
    f.client.sync();
    assert_eq!(f.calls(), 0);
    let _ = std::fs::remove_dir_all(&f.dir);
}
