mod common;

use common::FakeRigctld;
use rig_tui::Session;
use rigctld_client::Client;

async fn session(rig: &FakeRigctld) -> Session {
    Session::new(Client::connect(rig.addr).await.unwrap())
}

#[tokio::test]
async fn poll_returns_decoded_text_and_pending_transmit_count() {
    let rig = FakeRigctld::start(&["TB305HELLO;"]).await;
    let mut session = session(&rig).await;

    let polled = session.poll().await.unwrap();

    assert_eq!(polled.text, "HELLO");
    assert_eq!(polled.tx_pending, 3);
    assert_eq!(*rig.received.lock().unwrap(), ["+w TB;"]);
}

#[tokio::test]
async fn send_hands_the_text_to_the_rigs_morse_keyer() {
    let rig = FakeRigctld::start(&[]).await;
    let mut session = session(&rig).await;

    session.send("CQ CQ DE W9XYZ").await.unwrap();

    assert_eq!(*rig.received.lock().unwrap(), ["+b CQ CQ DE W9XYZ"]);
}

#[tokio::test]
async fn send_collapses_line_breaks_so_one_command_stays_one_line() {
    let rig = FakeRigctld::start(&[]).await;
    let mut session = session(&rig).await;

    session.send("CQ\nDE\r\nW9XYZ").await.unwrap();

    assert_eq!(*rig.received.lock().unwrap(), ["+b CQ DE W9XYZ"]);
}

#[tokio::test]
async fn poll_reports_a_malformed_tb_reply_as_an_error() {
    let rig = FakeRigctld::start(&["TB005AB;"]).await;
    let mut session = session(&rig).await;

    let err = session.poll().await.unwrap_err();

    assert!(matches!(err, rig_tui::SessionError::Tb(_)), "got {err:?}");
}
