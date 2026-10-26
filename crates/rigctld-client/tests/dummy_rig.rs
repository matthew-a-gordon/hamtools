//! Integration tests for `Client`, run against Hamlib's dummy rig (`rigctld -m 1`).
//! Requires `rigctld` on PATH (provided by the dev shell).

use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use rigctld_client::{Client, Error};

/// Ports already handed to a rigctld in this process. The OS may reuse an ephemeral port as soon
/// as the probe listener closes, which would point two parallel tests at the same daemon.
static USED_PORTS: Mutex<Vec<u16>> = Mutex::new(Vec::new());

fn free_port() -> u16 {
    loop {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut used = USED_PORTS.lock().unwrap();
        if !used.contains(&port) {
            used.push(port);
            return port;
        }
    }
}

struct DummyRig {
    child: Child,
}

impl DummyRig {
    /// Start rigctld and return it with a connected, verified client.
    ///
    /// Under parallel load rigctld sometimes accepts the first connection and then resets it
    /// (cause not identified; its log is empty in those cases). Each attempt therefore proves the
    /// daemon answers a real command, and a fresh daemon on a new port is tried otherwise.
    async fn start() -> (Self, Client) {
        for _ in 0..5 {
            let port = free_port();
            let child = Command::new("rigctld")
                .args(["-m", "1", "-T", "127.0.0.1", "-t", &port.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("rigctld must be on PATH");
            let rig = Self { child };
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            for _ in 0..100 {
                if let Ok(mut client) = Client::connect(addr).await {
                    if client.get_freq().await.is_ok() {
                        return (rig, client);
                    }
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        panic!("could not start a working rigctld");
    }
}

impl Drop for DummyRig {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[tokio::test]
async fn set_frequency_is_read_back() {
    let (_rig, mut client) = DummyRig::start().await;

    client.set_freq(14_074_000).await.unwrap();

    assert_eq!(client.get_freq().await.unwrap(), 14_074_000);
}

#[tokio::test]
async fn set_mode_is_read_back_with_passband() {
    let (_rig, mut client) = DummyRig::start().await;

    client.set_mode("USB", 2400).await.unwrap();

    assert_eq!(client.get_mode().await.unwrap(), ("USB".to_string(), 2400));
}

#[tokio::test]
async fn set_vfo_is_read_back() {
    let (_rig, mut client) = DummyRig::start().await;

    client.set_vfo("VFOB").await.unwrap();

    assert_eq!(client.get_vfo().await.unwrap(), "VFOB");
}

#[tokio::test]
async fn set_split_is_read_back_with_tx_vfo() {
    let (_rig, mut client) = DummyRig::start().await;

    client.set_split(true, "VFOB").await.unwrap();

    assert_eq!(
        client.get_split().await.unwrap(),
        (true, "VFOB".to_string())
    );
}

#[tokio::test]
async fn set_ptt_is_read_back() {
    let (_rig, mut client) = DummyRig::start().await;

    client.set_ptt(true).await.unwrap();
    assert!(client.get_ptt().await.unwrap());

    client.set_ptt(false).await.unwrap();
    assert!(!client.get_ptt().await.unwrap());
}

#[tokio::test]
async fn unsupported_command_surfaces_as_rig_error() {
    let (_rig, mut client) = DummyRig::start().await;

    // The dummy rig has no CW keyer, so rigctld answers with a negative RPRT code.
    let err = client.send_morse("CQ").await.unwrap_err();

    assert!(matches!(err, Error::Rig(code) if code < 0), "got {err:?}");
    // The connection stays usable after a rig error.
    client.set_freq(7_030_000).await.unwrap();
}

#[tokio::test]
async fn raw_command_reply_is_returned_clean() {
    let (_rig, mut client) = DummyRig::start().await;

    // The dummy rig echoes raw commands back, with a stray CR/NUL after the text.
    let reply = client.send_cmd("TB;").await.unwrap();

    assert_eq!(reply, "TB;");
    client.set_freq(7_030_000).await.unwrap();
}
