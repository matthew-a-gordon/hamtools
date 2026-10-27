//! Downloads are tested against a stub HTTP server on a local port, never the real services.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;

use sat_tui::fetch::Fetcher;

const TLES: &str = include_str!("fixtures/sample.tle");
const TRANSMITTERS: &str = include_str!("fixtures/satnogs_transmitters.json");

struct Stub {
    addr: SocketAddr,
    /// The full request head (request line and headers) of every request received.
    requests: Arc<Mutex<Vec<String>>>,
}

impl Stub {
    /// Serve `status` and `body` to every request.
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
                    .push(String::from_utf8_lossy(&head).into_owned());
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { addr, requests }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn first_request(&self) -> String {
        self.requests.lock().unwrap()[0].clone()
    }
}

fn fetcher(stub: &Stub) -> Fetcher {
    Fetcher::with_base_urls(stub.url(), stub.url())
}

#[test]
fn tles_come_from_the_celestrak_amateur_group() {
    let stub = Stub::start(200, TLES);

    let body = fetcher(&stub).tles().unwrap();

    assert_eq!(body, TLES);
    assert!(
        stub.first_request()
            .starts_with("GET /NORAD/elements/gp.php?GROUP=amateur&FORMAT=tle "),
        "{}",
        stub.first_request()
    );
}

#[test]
fn transmitters_come_from_the_satnogs_api() {
    let stub = Stub::start(200, TRANSMITTERS);

    let body = fetcher(&stub).transmitters().unwrap();

    assert_eq!(body, TRANSMITTERS);
    assert!(
        stub.first_request()
            .starts_with("GET /api/transmitters/?format=json "),
        "{}",
        stub.first_request()
    );
}

#[test]
fn requests_identify_the_tool() {
    let stub = Stub::start(200, TLES);

    fetcher(&stub).tles().unwrap();

    let head = stub.first_request().to_lowercase();
    assert!(head.contains("user-agent: hamtools-sat-tui/"), "{head}");
}

#[test]
fn an_http_error_is_reported_with_its_status() {
    let stub = Stub::start(503, "busy");

    let err = fetcher(&stub).tles().unwrap_err();

    assert!(err.to_string().contains("503"), "{err}");
}

#[test]
fn a_page_that_is_not_tle_data_is_rejected_so_it_cannot_replace_the_cache() {
    // CelesTrak answers 200 with a plain message when it has nothing to give.
    let stub = Stub::start(200, "No GP data found");

    assert!(fetcher(&stub).tles().is_err());
}

#[test]
fn json_that_is_not_a_transmitter_list_is_rejected() {
    let stub = Stub::start(200, "<html>maintenance</html>");

    assert!(fetcher(&stub).transmitters().is_err());
}

#[test]
fn an_unreachable_server_is_an_error_not_a_hang() {
    // Bind then drop to get a port nothing is listening on.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let fetcher = Fetcher::with_base_urls(format!("http://127.0.0.1:{port}"), String::new());

    assert!(fetcher.tles().is_err());
}
