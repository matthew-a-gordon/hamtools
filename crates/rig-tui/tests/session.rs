//! `App` driven against the scripted fake `rigctld` through `Rig`, the way `main` does it.

mod common;

use std::time::{Duration, Instant};

use common::*;
use ratatui::crossterm::event::KeyCode;
use rig_tui::{App, Command, RECONNECT_EVERY, Rig, RigMode};

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

async fn connected(rig: &FakeRigctld) -> (Rig, App) {
    let mut rig = Rig::new(rig.addr.to_string());
    let mut app = App::new();
    tick(&mut rig, &mut app, Instant::now()).await;
    assert!(app.connected());
    (rig, app)
}

#[tokio::test]
async fn connecting_then_polling_reads_frequency_and_mode() {
    let fake = FakeRigctld::start(&[]).await;
    let (mut rig, mut app) = connected(&fake).await;

    tick(&mut rig, &mut app, Instant::now()).await;

    assert_eq!(app.frequency(), Some(14_074_000));
    assert_eq!(app.rig_mode(), Some(&RigMode::Cw));
    assert_eq!(app.passband(), Some(500));
    let commands = fake.commands();
    assert!(commands.contains(&"+f".to_string()), "{commands:?}");
    assert!(commands.contains(&"+m".to_string()), "{commands:?}");
}

#[tokio::test]
async fn in_cw_the_tb_buffer_is_polled_and_decoded_text_reaches_the_transcript() {
    let fake = FakeRigctld::start(&["TB305HELLO;"]).await;
    let (mut rig, mut app) = connected(&fake).await;
    let t = Instant::now();
    tick(&mut rig, &mut app, t).await; // learns the mode
    tick(&mut rig, &mut app, t).await; // CW is active now, so TB is polled

    assert!(has(&screen(&app, 80, 24), "HELLO"));
    assert!(fake.commands().contains(&"+w TB;".to_string()));
}

#[tokio::test]
async fn outside_cw_tb_is_never_polled() {
    let fake = FakeRigctld::start_with(
        &[],
        RigState {
            mode: "USB".to_string(),
            passband: 2400,
            ..RigState::default()
        },
    )
    .await;
    let (mut rig, mut app) = connected(&fake).await;
    let t = Instant::now();

    for i in 0..3 {
        tick(&mut rig, &mut app, t + Duration::from_millis(100 * i)).await;
    }

    assert_eq!(app.rig_mode(), Some(&RigMode::Usb));
    assert!(!fake.commands().iter().any(|c| c.contains("TB;")));
}

#[tokio::test]
async fn a_soft_key_keys_the_rigs_morse_sender_with_the_callsign_filled_in() {
    let fake = FakeRigctld::start(&[]).await;
    let mut rig = Rig::new(fake.addr.to_string());
    let mut app = App::with_callsign(Some("W9XYZ".to_string()))
        .with_soft_keys([(1, "CQ DE {CALL}".to_string())].into());
    tick(&mut rig, &mut app, Instant::now()).await; // connects
    tick(&mut rig, &mut app, Instant::now()).await; // learns the mode

    let commands = app.on_key(key(KeyCode::F(1)));
    dispatch(&mut rig, &mut app, commands).await;

    assert!(fake.commands().contains(&"+b CQ DE W9XYZ".to_string()));
}

#[tokio::test]
async fn typed_text_is_sent_as_one_line_of_morse() {
    let fake = FakeRigctld::start(&[]).await;
    let (mut rig, mut app) = connected(&fake).await;
    tick(&mut rig, &mut app, Instant::now()).await;
    app.on_key(key(KeyCode::Tab));
    type_str(&mut app, "cq cq de w9xyz");

    let commands = app.on_key(key(KeyCode::Enter));
    dispatch(&mut rig, &mut app, commands).await;

    assert!(fake.commands().contains(&"+b CQ CQ DE W9XYZ".to_string()));
}

#[tokio::test]
async fn send_collapses_line_breaks_so_one_command_stays_one_line() {
    let fake = FakeRigctld::start(&[]).await;
    let (mut rig, mut app) = connected(&fake).await;

    dispatch(
        &mut rig,
        &mut app,
        vec![Command::Send("CQ\nDE\r\nW9XYZ".to_string())],
    )
    .await;

    assert!(fake.commands().contains(&"+b CQ DE W9XYZ".to_string()));
}

#[tokio::test]
async fn the_app_starts_with_no_rig_and_connects_once_the_daemon_appears() {
    let addr = reserve_addr().await;
    let mut rig = Rig::new(addr.to_string());
    let mut app = App::new();
    let t0 = Instant::now();

    tick(&mut rig, &mut app, t0).await;
    assert!(!app.connected());
    assert!(
        app.status()
            .is_some_and(|s| s.contains("cannot reach rigctld")),
        "{:?}",
        app.status()
    );
    assert!(has(&screen(&app, 80, 24), "disconnected"));

    // Not due yet: no second attempt.
    assert_eq!(app.tick(t0 + Duration::from_secs(1)), vec![]);

    let fake = FakeRigctld::start_at(addr, &[], RigState::default()).await;
    tick(&mut rig, &mut app, t0 + RECONNECT_EVERY).await;

    assert!(app.connected());
    assert_eq!(app.status(), None);
    tick(&mut rig, &mut app, t0 + RECONNECT_EVERY).await;
    assert_eq!(app.rig_mode(), Some(&RigMode::Cw));
    drop(fake);
}

#[tokio::test]
async fn losing_the_connection_is_reported_and_the_app_reconnects() {
    let fake = FakeRigctld::start(&[]).await;
    let (mut rig, mut app) = connected(&fake).await;
    let t0 = Instant::now();

    fake.hang_up
        .store(true, std::sync::atomic::Ordering::SeqCst);
    tick(&mut rig, &mut app, t0).await;

    assert!(!app.connected());
    assert!(app.status().is_some(), "the loss is reported");
    assert!(has(&screen(&app, 80, 24), "disconnected"));

    tick(&mut rig, &mut app, t0 + RECONNECT_EVERY).await;
    assert!(app.connected());
    tick(&mut rig, &mut app, t0 + RECONNECT_EVERY).await;
    assert_eq!(app.frequency(), Some(14_074_000));
}

#[tokio::test]
async fn commands_before_a_connection_do_nothing() {
    let mut rig = Rig::new("127.0.0.1:1".to_string());

    let events = rig.execute(Command::PollState).await;

    assert_eq!(events, vec![]);
}

#[tokio::test]
async fn the_fake_rig_applies_frequency_and_mode_sets() {
    let fake = FakeRigctld::start(&[]).await;
    let mut client = rigctld_client::Client::connect(fake.addr).await.unwrap();

    client.set_freq(7_030_000).await.unwrap();
    client.set_mode("LSB", 2400).await.unwrap();

    assert_eq!(client.get_freq().await.unwrap(), 7_030_000);
    assert_eq!(client.get_mode().await.unwrap(), ("LSB".to_string(), 2400));
}
