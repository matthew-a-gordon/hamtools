//! The address lookup is tested against a stub HTTP server on a local port, never the real service.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rig_tui::geocode::{GeocodeError, Geocoder, MIN_INTERVAL, USER_AGENT};

const MIDDLEBURY: &str = r#"[
  {"lat":"41.6755","lon":"-85.7062","display_name":"Middlebury, Elkhart County, Indiana, United States"},
  {"lat":"44.0153","lon":"-73.1673","display_name":"Middlebury, Addison County, Vermont, United States"}
]"#;

struct Stub {
    addr: SocketAddr,
    /// When each request arrived, and its full head (request line and headers).
    requests: Arc<Mutex<Vec<(Instant, String)>>>,
}

impl Stub {
    fn start(status: u16, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap() == 0 {
                        break;
                    }
                    head.push(byte[0]);
                }
                log.lock()
                    .unwrap()
                    .push((Instant::now(), String::from_utf8_lossy(&head).into_owned()));
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { addr, requests }
    }

    fn geocoder(&self) -> Geocoder {
        Geocoder::with_base_url(format!("http://{}", self.addr))
    }

    fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

#[test]
fn candidates_carry_the_name_and_the_six_character_locator() {
    let stub = Stub::start(200, MIDDLEBURY);

    let found = stub.geocoder().lookup("Middlebury").unwrap();

    assert_eq!(found.len(), 2);
    assert_eq!(
        found[0].name,
        "Middlebury, Elkhart County, Indiana, United States"
    );
    assert_eq!(found[0].locator.as_str(), "EN71DQ");
}

#[test]
fn no_match_is_an_empty_list() {
    let stub = Stub::start(200, "[]");

    assert_eq!(stub.geocoder().lookup("zzzzqq").unwrap(), vec![]);
}

#[test]
fn http_429_is_rate_limited() {
    let stub = Stub::start(429, "slow down");

    assert_eq!(
        stub.geocoder().lookup("Middlebury").unwrap_err(),
        GeocodeError::RateLimited
    );
}

#[test]
fn an_unreachable_server_is_offline() {
    let addr = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    let geocoder = Geocoder::with_base_url(format!("http://{addr}"));

    assert!(matches!(
        geocoder.lookup("Middlebury"),
        Err(GeocodeError::Offline(_))
    ));
}

#[test]
fn an_unusable_answer_is_an_error_not_a_panic() {
    let stub = Stub::start(200, "<html>maintenance</html>");

    assert!(matches!(
        stub.geocoder().lookup("Middlebury"),
        Err(GeocodeError::BadReply(_))
    ));
}

#[test]
fn the_request_asks_for_at_most_five_and_names_the_app_in_the_user_agent() {
    let stub = Stub::start(200, "[]");

    stub.geocoder().lookup("1 Main St").unwrap();

    let head = stub.requests.lock().unwrap()[0].1.clone();
    assert!(head.starts_with("GET /search?"), "{head}");
    assert!(head.contains("limit=5"), "{head}");
    assert!(
        head.contains("q=1+Main+St") || head.contains("q=1%20Main%20St"),
        "{head}"
    );
    assert!(
        head.to_ascii_lowercase()
            .contains(&format!("user-agent: {}", USER_AGENT.to_ascii_lowercase())),
        "{head}"
    );
    assert!(USER_AGENT.starts_with("rig-tui/"));
    assert!(USER_AGENT.contains(env!("CARGO_PKG_VERSION")));
    assert!(USER_AGENT.contains("https://github.com/matthew-a-gordon/hamtools"));
}

#[test]
fn requests_are_at_least_the_minimum_interval_apart() {
    let stub = Stub::start(200, "[]");
    let geocoder = stub
        .geocoder()
        .with_min_interval(Duration::from_millis(400));

    geocoder.lookup("one").unwrap();
    geocoder.lookup("two").unwrap();

    let requests = stub.requests.lock().unwrap();
    assert!(requests[1].0 - requests[0].0 >= Duration::from_millis(400));
}

#[test]
fn the_default_spacing_is_one_second() {
    assert_eq!(MIN_INTERVAL, Duration::from_secs(1));
}

#[test]
fn the_same_address_is_answered_from_the_session_cache_until_cleared() {
    let stub = Stub::start(200, MIDDLEBURY);
    let geocoder = stub.geocoder().with_min_interval(Duration::ZERO);

    let first = geocoder.lookup("Middlebury").unwrap();
    let second = geocoder.lookup("  middlebury ").unwrap();
    assert_eq!(first, second);
    assert_eq!(stub.count(), 1);

    geocoder.clear_cache();
    geocoder.lookup("Middlebury").unwrap();
    assert_eq!(stub.count(), 2);
}

#[test]
fn errors_are_not_cached() {
    let stub = Stub::start(429, "");
    let geocoder = stub.geocoder().with_min_interval(Duration::ZERO);

    let _ = geocoder.lookup("Middlebury");
    let _ = geocoder.lookup("Middlebury");

    assert_eq!(stub.count(), 2);
}

#[test]
fn offline_makes_no_request() {
    let stub = Stub::start(200, MIDDLEBURY);
    let geocoder = Geocoder::with_base_url(format!("http://{}", stub.addr)).with_offline(true);

    assert!(matches!(
        geocoder.lookup("Middlebury, IN"),
        Err(GeocodeError::Offline(_))
    ));
    assert_eq!(stub.count(), 0);
}
