//! Frequency, mode, VFO and split control against the scripted fake `rigctld`: set, read-back,
//! refusal and "not supported". The fake is scripted, not a radio: all of this is untested on
//! hardware through `rig-tui`.

mod common;

use std::time::Instant;

use common::*;
use ratatui::crossterm::event::KeyCode;
use rig_tui::{App, Command, Feature, Rig, RigMode};

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

/// Connected and polled once, so the app knows the rig's state.
async fn ready(fake: &FakeRigctld) -> (Rig, App) {
    let mut rig = Rig::new(fake.addr.to_string());
    let mut app = App::new();
    let t = Instant::now();
    tick(&mut rig, &mut app, t).await; // connects
    tick(&mut rig, &mut app, t).await; // polls
    assert!(app.connected());
    (rig, app)
}

fn press(app: &mut App, text: &str) -> Vec<Command> {
    let mut out = Vec::new();
    for c in text.chars() {
        out.extend(app.on_key(key(KeyCode::Char(c))));
    }
    out
}

#[tokio::test]
async fn polling_reads_vfo_split_ptt_and_the_s_meter() {
    let fake = FakeRigctld::start_with(
        &[],
        RigState {
            vfo: "VFOB".to_string(),
            strength: 20,
            ..RigState::default()
        },
    )
    .await;
    let (_rig, app) = ready(&fake).await;

    assert_eq!(app.vfo(), Some("VFOB"));
    assert_eq!(app.split(), Some((false, "VFOA")));
    assert_eq!(app.ptt(), Some(false));
    assert_eq!(app.strength(), Some(20));
    let rows = screen(&app, 80, 24);
    assert!(has(&rows, "VFO B"), "{rows:#?}");
    assert!(has(&rows, "S9+20"), "{rows:#?}");
}

#[tokio::test]
async fn set_frequency_is_read_back_and_the_strip_shows_the_rig_value() {
    let fake = FakeRigctld::start(&[]).await;
    let (mut rig, mut app) = ready(&fake).await;

    let commands = press(&mut app, "f7030");
    assert_eq!(commands, vec![]);
    let commands = app.on_key(key(KeyCode::Enter));
    dispatch(&mut rig, &mut app, commands).await;

    assert_eq!(app.frequency(), Some(7_030_000));
    let commands = fake.commands();
    let set = commands.iter().position(|c| c == "+F 7030000").unwrap();
    assert!(
        commands[set + 1..].contains(&"+f".to_string()),
        "no read-back after the set: {commands:?}"
    );
    assert!(has(&screen(&app, 80, 24), "7.030.000"));
}

#[tokio::test]
async fn tuning_with_the_arrow_keys_moves_by_the_step() {
    let fake = FakeRigctld::start(&[]).await;
    let (mut rig, mut app) = ready(&fake).await;

    let commands = app.on_key(key(KeyCode::Right));
    dispatch(&mut rig, &mut app, commands).await;

    assert_eq!(app.frequency(), Some(14_075_000));
}

#[tokio::test]
async fn set_mode_is_read_back() {
    let fake = FakeRigctld::start(&[]).await;
    let (mut rig, mut app) = ready(&fake).await;

    let commands = press(&mut app, "m"); // CW -> CWR
    dispatch(&mut rig, &mut app, commands).await;

    assert_eq!(app.rig_mode(), Some(&RigMode::Cwr));
    assert!(fake.commands().contains(&"+M CWR 0".to_string()));
}

#[tokio::test]
async fn a_refused_set_shows_its_error_and_the_strip_keeps_the_rigs_value() {
    let fake = FakeRigctld::start_with(
        &[],
        RigState {
            refuse_set_freq: true,
            ..RigState::default()
        },
    )
    .await;
    let (mut rig, mut app) = ready(&fake).await;

    press(&mut app, "f7030");
    let commands = app.on_key(key(KeyCode::Enter));
    dispatch(&mut rig, &mut app, commands).await;

    assert!(
        app.status()
            .is_some_and(|s| s.contains("set frequency failed")),
        "{:?}",
        app.status()
    );
    assert_eq!(
        app.frequency(),
        Some(14_074_000),
        "never what was requested"
    );
    assert!(app.connected());
}

#[tokio::test]
async fn vfo_select_is_read_back() {
    let fake = FakeRigctld::start(&[]).await;
    let (mut rig, mut app) = ready(&fake).await;

    let commands = press(&mut app, "v");
    dispatch(&mut rig, &mut app, commands).await;

    assert_eq!(app.vfo(), Some("VFOB"));
    assert!(has(&screen(&app, 80, 24), "VFO B"));
}

#[tokio::test]
async fn split_on_then_a_transmit_frequency_is_read_back_and_shown() {
    let fake = FakeRigctld::start(&[]).await;
    let (mut rig, mut app) = ready(&fake).await;

    let commands = press(&mut app, "s");
    dispatch(&mut rig, &mut app, commands).await;
    assert_eq!(app.split(), Some((true, "VFOB")));

    press(&mut app, "S14080");
    let commands = app.on_key(key(KeyCode::Enter));
    dispatch(&mut rig, &mut app, commands).await;

    assert_eq!(app.transmit_frequency(), Some(14_080_000));
    let rows = screen(&app, 80, 24);
    assert!(has(&rows, "split B 14.080.000"), "{rows:#?}");
    assert!(fake.commands().contains(&"+S 1 VFOB".to_string()));
    assert!(fake.commands().contains(&"+I 14080000".to_string()));
}

#[tokio::test]
async fn split_off_is_read_back() {
    let fake = FakeRigctld::start_with(
        &[],
        RigState {
            split: true,
            tx_vfo: "VFOB".to_string(),
            tx_freq: 14_080_000,
            ..RigState::default()
        },
    )
    .await;
    let (mut rig, mut app) = ready(&fake).await;
    assert_eq!(app.split(), Some((true, "VFOB")));

    let commands = press(&mut app, "s");
    dispatch(&mut rig, &mut app, commands).await;

    assert_eq!(app.split(), Some((false, "VFOA")));
    assert!(has(&screen(&app, 80, 24), "split off"));
    assert!(fake.commands().contains(&"+S 0 VFOA".to_string()));
}

#[tokio::test]
async fn a_rig_without_vfo_support_says_so_is_not_asked_again_and_the_control_is_disabled() {
    let fake = FakeRigctld::start_with(
        &[],
        RigState {
            no_vfo: true,
            ..RigState::default()
        },
    )
    .await;
    let (mut rig, mut app) = ready(&fake).await;
    tick(
        &mut rig,
        &mut app,
        Instant::now() + rig_tui::STATE_POLL_EVERY * 2,
    )
    .await;

    assert!(app.is_unsupported(Feature::Vfo));
    assert!(has(&screen(&app, 80, 24), "VFO not supported"));
    assert_eq!(press(&mut app, "v"), vec![]);
    assert!(app.status().is_some_and(|s| s.contains("not supported")));
    let asked = fake
        .commands()
        .iter()
        .filter(|c| c.contains("get_vfo"))
        .count();
    assert_eq!(asked, 1, "asked again after being refused");
    // The rest of the strip still works.
    assert_eq!(app.frequency(), Some(14_074_000));
}

#[tokio::test]
async fn a_rig_without_split_support_says_so_and_log_frequency_is_the_receive_one() {
    let fake = FakeRigctld::start_with(
        &[],
        RigState {
            no_split: true,
            ..RigState::default()
        },
    )
    .await;
    let (_rig, app) = ready(&fake).await;

    assert!(app.is_unsupported(Feature::Split));
    assert!(has(&screen(&app, 80, 24), "split not supported"));
    assert_eq!(app.transmit_frequency(), Some(14_074_000));
}

#[tokio::test]
async fn nothing_on_the_strip_can_key_the_transmitter() {
    let fake = FakeRigctld::start(&[]).await;
    let (mut rig, mut app) = ready(&fake).await;

    // Every printable key and the arrows, each followed through to the rig.
    for c in (' '..='~').filter(|c| *c != 'q') {
        let commands = app.on_key(key(KeyCode::Char(c)));
        dispatch(&mut rig, &mut app, commands).await;
        let commands = app.on_key(key(KeyCode::Esc));
        dispatch(&mut rig, &mut app, commands).await;
    }
    for code in [KeyCode::Left, KeyCode::Right, KeyCode::Up, KeyCode::Down] {
        let commands = app.on_key(key(code));
        dispatch(&mut rig, &mut app, commands).await;
    }

    let sent = fake.commands();
    assert!(
        !sent
            .iter()
            .any(|c| c.starts_with("+T ") || c.starts_with("+b ") || c.starts_with("+w ")),
        "{sent:?}"
    );
}
