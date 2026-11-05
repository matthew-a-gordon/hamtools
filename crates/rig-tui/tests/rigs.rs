//! Rig profiles, the capability table and the rig switcher, against two scripted `rigctld`s.
//! The fakes are not radios: the real-rig behaviour here is untested on hardware.

mod common;

use std::time::{Duration, Instant};

use common::*;
use hamtools_config::{RigKind, RigProfile};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use rig_tui::{
    Activity, ActivityCtx, ActivityOutput, App, Capability, Command, LogDefaults, LogMode, Rig,
    RigEvent, RigMode, capability_table,
};

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

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

/// Two profiles, `kx3` first, on addresses that nothing listens on (no I/O in these tests).
fn two_rigs() -> App {
    App::new().with_rigs(
        &[
            profile("kx3", "127.0.0.1:1", RigKind::Kx3),
            profile("ft847", "127.0.0.1:2", RigKind::Ft847),
        ],
        None,
    )
}

/// Route commands like `main` does: ordinary ones to the Active rig, light polls to theirs.
async fn dispatch_all(rigs: &mut [Rig], app: &mut App, commands: Vec<Command>) {
    for command in commands {
        let target = match &command {
            Command::LightPoll(rig) | Command::SetRigAddr { rig, .. } => *rig,
            _ => app.active_rig(),
        };
        let background = matches!(command, Command::LightPoll(_));
        for event in rigs[target].execute(command).await {
            if background {
                app.on_rig_at(target, event);
            } else {
                app.on_rig(event);
            }
        }
    }
}

fn switch_via_keys(app: &mut App, down: usize) {
    assert_eq!(app.on_key(ctrl('r')), vec![]);
    for _ in 0..down {
        app.on_key(key(KeyCode::Down));
    }
    assert_eq!(app.on_key(key(KeyCode::Enter)), vec![]);
}

// ---- the capability table ----------------------------------------------------------------

#[test]
fn every_row_not_verified_on_hardware_is_labelled_and_verified_rows_are_not() {
    for kind in [RigKind::Kx3, RigKind::Ft847, RigKind::Generic] {
        let caps = capability_table(kind);
        let rows = caps.rows();
        let verified = [
            (Capability::Cw, caps.cw.verified, "CW"),
            (Capability::Split, caps.split.verified, "split"),
            (Capability::SatMode, caps.sat_mode.verified, "sat mode"),
        ];
        for (_, is_verified, label) in verified {
            let row = rows.iter().find(|r| r.starts_with(label)).unwrap();
            assert_eq!(
                row.contains("untested on hardware"),
                !is_verified,
                "{kind:?}: {row}"
            );
        }
        let bands = rows.iter().find(|r| r.starts_with("bands")).unwrap();
        assert_eq!(
            bands.contains("untested on hardware"),
            !caps.bands_verified,
            "{bands}"
        );
    }
}

#[test]
fn the_kx3_row_follows_claude_md_and_the_ft847_has_split_and_sat_mode_verified_but_not_cw() {
    let kx3 = capability_table(RigKind::Kx3);
    assert!(kx3.has(Capability::Cw) && kx3.verified(Capability::Cw));
    assert!(kx3.has(Capability::Split) && kx3.verified(Capability::Split));
    assert!(!kx3.has(Capability::SatMode));

    let ft = capability_table(RigKind::Ft847);
    // Split seen on the FT-847 on 2026-11-04 and SAT mode engaging and releasing on 2026-11-05
    // (CLAUDE.md); CW was not.
    assert!(ft.has(Capability::Split) && ft.verified(Capability::Split));
    assert!(ft.has(Capability::SatMode) && ft.verified(Capability::SatMode));
    assert!(!ft.verified(Capability::Cw));
    let rows = ft.rows();
    let row = |label: &str| rows.iter().find(|r| r.starts_with(label)).unwrap().clone();
    assert!(!row("split").contains("untested"), "{rows:?}");
    assert!(row("CW").contains("untested on hardware"), "{rows:?}");
    assert!(!row("sat mode").contains("untested"), "{rows:?}");
}

#[test]
fn a_profile_override_changes_a_cell_and_is_never_verified() {
    let mut p = profile("ft847", "h:1", RigKind::Ft847);
    p.cw = Some(true);

    let caps = rig_tui::Capabilities::for_profile(&p);

    assert!(caps.has(Capability::Cw));
    assert!(!caps.verified(Capability::Cw));
    assert!(
        caps.has(Capability::SatMode),
        "untouched cells keep the table"
    );
}

// ---- profiles and the app ---------------------------------------------------------------

#[test]
fn the_strip_names_the_active_profile_and_the_default_picks_it() {
    let app = connected_app_in(
        App::new().with_rigs(
            &[
                profile("kx3", "127.0.0.1:1", RigKind::Kx3),
                profile("ft847", "127.0.0.1:2", RigKind::Ft847),
            ],
            Some("ft847"),
        ),
        "CW",
    );

    assert_eq!(app.active_rig(), 1);
    assert_eq!(app.rig_addr(), "127.0.0.1:2");
    assert!(has(&screen(&app, 80, 24), "ft847"));
    assert_eq!(app.rig_addrs(), ["127.0.0.1:1", "127.0.0.1:2"]);
}

#[test]
fn without_a_default_the_first_profile_is_active() {
    assert_eq!(two_rigs().active_rig(), 0);
}

// ---- both rigs connected, the inactive one polled lightly --------------------------------

#[tokio::test]
async fn both_rigs_connect_and_the_inactive_one_is_polled_lightly_about_every_five_seconds() {
    let kx3 = FakeRigctld::start(&[]).await;
    let ft = FakeRigctld::start_with(
        &[],
        RigState {
            freq: 435_000_000,
            mode: "FM".into(),
            ..RigState::default()
        },
    )
    .await;
    let mut app = App::new().with_rigs(
        &[
            profile("kx3", &kx3.addr.to_string(), RigKind::Kx3),
            profile("ft847", &ft.addr.to_string(), RigKind::Ft847),
        ],
        None,
    );
    let mut rigs: Vec<Rig> = app.rig_addrs().into_iter().map(Rig::new).collect();
    let t0 = Instant::now();

    let first = app.tick(t0);
    assert!(first.contains(&Command::LightPoll(1)), "{first:?}");
    dispatch_all(&mut rigs, &mut app, first).await;
    assert!(app.connected(), "the Active rig connected");
    assert!(ft.commands().is_empty(), "connecting only, no reads yet");

    // Not due again after 1 s.
    let soon = app.tick(t0 + Duration::from_secs(1));
    assert!(!soon.contains(&Command::LightPoll(1)), "{soon:?}");

    let later = app.tick(t0 + Duration::from_secs(5));
    assert!(later.contains(&Command::LightPoll(1)), "{later:?}");
    dispatch_all(&mut rigs, &mut app, later).await;
    let polled = ft.commands();
    assert_eq!(polled, ["+f", "+m"], "health, frequency and mode only");

    // What it reported shows in the switcher; the Active rig is not polled lightly.
    app.on_key(ctrl('r'));
    let rows = screen(&app, 100, 24);
    assert!(has(&rows, "435.000.000 FM"), "{rows:#?}");
    assert!(!has(&rows, "disconnected"), "{rows:#?}");
}

#[tokio::test]
async fn an_unreachable_inactive_rig_shows_disconnected_and_is_retried() {
    let kx3 = FakeRigctld::start(&[]).await;
    let dead = reserve_addr().await;
    let mut app = App::new().with_rigs(
        &[
            profile("kx3", &kx3.addr.to_string(), RigKind::Kx3),
            profile("ft847", &dead.to_string(), RigKind::Ft847),
        ],
        None,
    );
    let mut rigs: Vec<Rig> = app.rig_addrs().into_iter().map(Rig::new).collect();
    let t0 = Instant::now();
    let commands = app.tick(t0);
    dispatch_all(&mut rigs, &mut app, commands).await;

    app.on_key(ctrl('r'));

    let rows = screen(&app, 100, 24);
    assert!(has(&rows, "disconnected"), "{rows:#?}");
    assert!(
        app.tick(t0 + Duration::from_secs(5))
            .contains(&Command::LightPoll(1))
    );
}

// ---- switching -----------------------------------------------------------------------

#[test]
fn ctrl_r_opens_the_switcher_showing_both_rigs_and_enter_switches() {
    let mut app = connected_app_in(two_rigs(), "CW");
    app.on_key(ctrl('r'));
    assert!(app.has_overlay());
    let rows = screen(&app, 100, 24);
    assert!(has(&rows, "kx3") && has(&rows, "ft847"), "{rows:#?}");
    assert!(has(&rows, "untested on hardware"), "{rows:#?}");
    assert!(has(&rows, "disconnected"), "{rows:#?}");

    app.on_key(key(KeyCode::Down));
    app.on_key(key(KeyCode::Enter));

    assert!(!app.has_overlay());
    assert_eq!(app.active_rig(), 1);
    assert_eq!(app.rig_addr(), "127.0.0.1:2");
}

#[test]
fn escape_closes_the_switcher_without_switching() {
    let mut app = two_rigs();
    app.on_key(ctrl('r'));
    app.on_key(key(KeyCode::Down));
    app.on_key(key(KeyCode::Esc));

    assert!(!app.has_overlay());
    assert_eq!(app.active_rig(), 0);
}

#[test]
fn ctrl_r_works_from_the_cw_input_line() {
    let mut app = focus_input(connected_app_in(two_rigs(), "CW"));
    switch_via_keys(&mut app, 1);
    assert_eq!(app.active_rig(), 1);
}

#[test]
fn switching_is_refused_while_ptt_is_on() {
    let mut app = connected_app_in(two_rigs(), "CW");
    app.on_rig(RigEvent::Ptt(true));

    switch_via_keys(&mut app, 1);

    assert_eq!(app.active_rig(), 0);
    assert!(
        app.status().unwrap().contains("transmitting"),
        "{:?}",
        app.status()
    );

    app.on_rig(RigEvent::Ptt(false));
    switch_via_keys(&mut app, 1);
    assert_eq!(app.active_rig(), 1);
}

#[test]
fn switching_is_refused_while_a_cw_send_is_in_progress_until_the_rig_has_drained_it() {
    let mut app = focus_input(connected_app_in(two_rigs(), "CW"));
    type_str(&mut app, "CQ");
    let sent = app.on_key(key(KeyCode::Enter));
    assert_eq!(sent, vec![Command::Send("CQ".into())]);

    // Sent, and no TB; reply seen yet.
    switch_via_keys(&mut app, 1);
    assert_eq!(app.active_rig(), 0, "send not yet confirmed drained");

    // The rig still has three characters waiting.
    rx(&mut app, "TB300;");
    switch_via_keys(&mut app, 1);
    assert_eq!(app.active_rig(), 0, "{:?}", app.status());
    assert!(app.status().unwrap().contains("transmitting"));

    rx(&mut app, "TB000;");
    switch_via_keys(&mut app, 1);
    assert_eq!(app.active_rig(), 1);
}

#[test]
fn a_dead_active_rig_never_blocks_a_switch() {
    let mut app = focus_input(connected_app_in(two_rigs(), "CW"));
    type_str(&mut app, "CQ");
    app.on_key(key(KeyCode::Enter));
    app.on_rig(RigEvent::Disconnected("gone".into()));

    switch_via_keys(&mut app, 1);

    assert_eq!(app.active_rig(), 1);
}

#[test]
fn each_rig_keeps_its_own_readings_step_and_cw_transcript() {
    let mut app = connected_app_in(two_rigs(), "CW");
    rx(&mut app, "TB006F C E ;");
    app.on_key(key(KeyCode::Up)); // step 1 kHz -> 10 kHz
    let step = app.tuning_step();
    assert_eq!(step, 10_000);
    assert_eq!(app.frequency(), Some(14_074_000));

    switch_via_keys(&mut app, 1);

    // The new rig has seen nothing yet.
    assert!(!app.connected());
    assert_eq!(app.frequency(), None);
    assert_eq!(app.tuning_step(), 1_000);
    assert!(!has(&screen(&app, 80, 24), "F C E"));
    app.on_rig(RigEvent::Connected);
    app.on_rig(RigEvent::Freq(435_000_000));
    app.on_rig(mode_event("FM"));
    assert_eq!(app.frequency(), Some(435_000_000));

    app.on_key(ctrl('r'));
    app.on_key(key(KeyCode::Up));
    app.on_key(key(KeyCode::Enter));

    assert_eq!(app.active_rig(), 0);
    assert!(app.connected());
    assert_eq!(app.frequency(), Some(14_074_000), "last frequency kept");
    assert_eq!(app.tuning_step(), 10_000, "step kept");
    assert!(has(&screen(&app, 80, 24), "F C E"), "CW transcript kept");
}

// ---- Activities the new rig cannot run ---------------------------------------------------

/// An Activity that needs the FT-847's satellite mode.
struct NeedsSat;

impl Activity for NeedsSat {
    fn name(&self) -> &'static str {
        "Needs sat"
    }
    fn requires(&self) -> &'static [Capability] {
        &[Capability::SatMode]
    }
    fn on_key(&mut self, _key: KeyEvent, _ctx: &ActivityCtx) -> Option<ActivityOutput> {
        None
    }
    fn on_tick(&mut self, _now: Instant, _ctx: &ActivityCtx) -> Vec<Command> {
        Vec::new()
    }
    fn on_reply(&mut self, _request: &str, _reply: Result<String, String>) -> Result<(), String> {
        Ok(())
    }
    fn on_mode(&mut self, _ctx: &ActivityCtx) {}
    fn text_focus(&self, _ctx: &ActivityCtx) -> bool {
        false
    }
    fn has_panel(&self, _ctx: &ActivityCtx) -> bool {
        false
    }
    fn log_hints(&self, _mode: &RigMode) -> (Option<LogMode>, LogDefaults) {
        (None, LogDefaults::default())
    }
    fn hint(&self, _ctx: &ActivityCtx) -> &'static str {
        ""
    }
    fn render(&self, frame: &mut Frame, area: Rect, _ctx: &ActivityCtx) {
        frame.render_widget(Paragraph::new("SAT BODY"), area);
    }
}

#[test]
fn switching_to_a_rig_that_cannot_run_the_activity_drops_to_general_and_says_so() {
    let mut app = App::new().with_rigs(
        &[
            profile("kx3", "127.0.0.1:1", RigKind::Kx3),
            profile("ft847", "127.0.0.1:2", RigKind::Ft847),
        ],
        Some("ft847"),
    );
    let sat = app.host_activity(Box::new(NeedsSat));
    app.set_foreground(sat);
    assert_eq!(app.activity_name(), "Needs sat", "the FT-847 can run it");

    app.on_key(ctrl('r'));
    app.on_key(key(KeyCode::Up));
    app.on_key(key(KeyCode::Enter));

    assert_eq!(app.active_rig(), 0);
    assert_eq!(app.activity_name(), "General");
    let status = app.status().unwrap();
    assert!(
        status.contains("Needs sat") && status.contains("sat mode"),
        "{status}"
    );
    assert!(status.contains("General"), "{status}");
}

#[test]
fn an_activity_the_active_rig_cannot_run_is_unavailable_with_the_reason_and_is_not_entered() {
    let mut app = two_rigs(); // starts on the KX3
    let sat = app.host_activity(Box::new(NeedsSat));

    let why = app.activity_unavailable(sat).unwrap();
    assert!(why.contains("sat mode") && why.contains("kx3"), "{why}");
    assert_eq!(app.activity_unavailable(0), None, "General runs anywhere");

    app.set_foreground(sat);

    assert_eq!(app.activity_name(), "General");
    assert!(app.status().unwrap().contains("sat mode"));
}

#[test]
fn rig_flag_picks_the_active_rig_by_name() {
    let app = two_rigs().with_active_rig("ft847").unwrap();
    assert_eq!(app.active_rig(), 1);
    assert_eq!(app.rig_addr(), "127.0.0.1:2");
}

#[test]
fn rig_flag_with_an_unknown_name_errors_and_lists_the_profiles() {
    let err = two_rigs().with_active_rig("ic7300").err().unwrap();
    assert!(
        err.contains("ic7300") && err.contains("kx3") && err.contains("ft847"),
        "{err}"
    );
}

#[test]
fn the_switcher_shows_ft847_split_and_sat_mode_verified_while_cw_stays_untested() {
    let mut app = two_rigs();
    app.on_key(ctrl('r'));
    let rows = screen(&app, 100, 30);
    // The FT-847 capability line wraps across two screen rows; join them before reading.
    let at = rows
        .iter()
        .position(|r| r.contains("CW no"))
        .expect("FT-847 row");
    let text = format!("{} {}", rows[at], rows[at + 1]);
    assert!(
        text.contains("CW no, untested on hardware; split yes;"),
        "{text}"
    );
    assert!(text.contains("sat mode yes;"), "{text}");
    assert!(!text.contains("sat mode yes, untested"), "{text}");
}

#[test]
fn rig_flag_with_no_profiles_errors() {
    assert!(App::new().with_active_rig("kx3").is_err());
}
