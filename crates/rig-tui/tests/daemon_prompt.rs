//! The start-rigctld prompt: when it is offered, what it starts, and that it never starts
//! anything by itself. No process is spawned and no port is opened here; untested on hardware.

mod common;

use std::time::{Duration, Instant};

use common::*;
use hamtools_config::{RigKind, RigProfile};
use ratatui::crossterm::event::KeyCode;
use rig_tui::daemon::{Launch, Port};
use rig_tui::{App, Command, RECONNECT_EVERY, RigEvent};

fn profile(name: &str, addr: &str, kind: RigKind) -> RigProfile {
    RigProfile {
        name: name.into(),
        addr: addr.into(),
        kind,
        cw: None,
        split: None,
        sat_mode: None,
    }
}

fn app() -> App {
    App::new()
        .with_rigs(
            &[
                profile("kx3", "127.0.0.1:4532", RigKind::Kx3),
                profile("ft847", "127.0.0.1:4533", RigKind::Ft847),
                profile("remote", "shack.lan:4532", RigKind::Kx3),
                profile("other", "127.0.0.1:4534", RigKind::Generic),
            ],
            None,
        )
        .with_daemon_prompt(true)
}

fn ports(names: &[&str]) -> Vec<Port> {
    names.iter().map(|n| Port { path: (*n).into() }).collect()
}

fn scans(commands: &[Command]) -> usize {
    commands
        .iter()
        .filter(|c| matches!(c, Command::ScanPorts))
        .count()
}

#[test]
fn is_offered_once_after_the_first_connect_fails() {
    let mut app = app();
    let t0 = Instant::now();
    assert_eq!(scans(&app.tick(t0)), 0, "no attempt has been made yet");
    let t1 = t0 + RECONNECT_EVERY;
    assert_eq!(scans(&app.tick(t1)), 1);
    let t2 = t1 + RECONNECT_EVERY;
    assert_eq!(scans(&app.tick(t2)), 0, "only once per session");
}

#[test]
fn is_never_offered_when_the_prompt_is_off() {
    let mut app = app().with_daemon_prompt(false);
    let t0 = Instant::now();
    app.tick(t0);
    assert_eq!(scans(&app.tick(t0 + RECONNECT_EVERY)), 0);
}

#[test]
fn ctrl_d_asks_for_the_ports() {
    let mut app = app();
    assert_eq!(scans(&app.on_key(ctrl('d'))), 1);
}

#[test]
fn enter_starts_the_active_rigs_daemon_on_the_chosen_port() {
    let mut app = app();
    app.on_ports(ports(&["/dev/ttyUSB0", "/dev/ttyUSB1"]));
    assert!(app.has_overlay());
    let commands = app.on_key(key(KeyCode::Enter));
    assert_eq!(
        commands,
        vec![Command::StartDaemon(Launch {
            rig: 0,
            name: "kx3".into(),
            kind: RigKind::Kx3,
            port: "/dev/ttyUSB0".into(),
            tcp: 4532,
        })]
    );
    assert!(app.has_overlay(), "the second port is still on offer");
    app.on_key(key(KeyCode::Enter));
    assert!(!app.has_overlay(), "closes when no port is left");
}

#[test]
fn only_local_known_profiles_are_choices() {
    let mut app = app();
    app.on_ports(ports(&["/dev/ttyUSB0"]));
    // kx3 -> ft847 -> back to kx3: the remote and generic profiles are skipped.
    app.on_key(key(KeyCode::Right));
    let Command::StartDaemon(l) = &app.on_key(key(KeyCode::Enter))[0] else {
        panic!("expected a start");
    };
    assert_eq!((l.name.as_str(), l.tcp), ("ft847", 4533));
}

#[test]
fn esc_starts_nothing() {
    let mut app = app();
    app.on_ports(ports(&["/dev/ttyUSB0"]));
    assert!(app.on_key(key(KeyCode::Esc)).is_empty());
    assert!(!app.has_overlay());
}

#[test]
fn an_unasked_prompt_stays_quiet_with_no_ports_but_the_chord_explains() {
    let mut quiet = app();
    quiet.on_ports(Vec::new());
    assert!(!quiet.has_overlay());
    assert_eq!(quiet.status(), None);

    let mut asked = app();
    asked.on_key(ctrl('d'));
    asked.on_ports(Vec::new());
    assert!(asked.status().unwrap().contains("no serial ports"));
}

#[test]
fn a_rig_that_connected_meanwhile_suppresses_the_unasked_prompt() {
    let mut app = app();
    app.on_rig(RigEvent::Connected);
    app.on_ports(ports(&["/dev/ttyUSB0"]));
    assert!(!app.has_overlay());
}

#[test]
fn without_a_local_profile_the_chord_says_what_is_missing() {
    let mut app = App::new().with_daemon_prompt(true);
    app.on_key(ctrl('d'));
    app.on_ports(ports(&["/dev/ttyUSB0"]));
    assert!(app.status().unwrap().contains("no local KX3 or FT-847"));
}

#[test]
fn a_prompt_is_not_raised_over_an_open_screen() {
    let mut app = app();
    let t0 = Instant::now();
    app.tick(t0);
    app.on_key(ctrl('k'));
    assert_eq!(
        scans(&app.tick(t0 + RECONNECT_EVERY + Duration::from_secs(1))),
        0
    );
}
