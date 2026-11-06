mod common;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use common::*;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use rig_tui::softkeys::Exchange;
use rig_tui::{
    App, Command, Focus, Overlay, OverlayAction, RECONNECT_EVERY, RigEvent, STATE_POLL_EVERY,
    STATUS_TTL,
};

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

mod cw_input {
    use super::*;

    #[test]
    fn typed_text_is_uppercased_and_backspace_edits_it() {
        let mut app = cw_app();

        type_str(&mut app, "cq x");
        app.on_key(key(KeyCode::Backspace));
        type_str(&mut app, "de");

        assert!(has(&screen(&app, 80, 24), "> CQ DE"));
    }

    #[test]
    fn enter_sends_the_input_clears_it_and_echoes_it_into_the_transcript() {
        let mut app = cw_app();
        type_str(&mut app, "cq cq");

        let commands = app.on_key(key(KeyCode::Enter));

        assert_eq!(commands, vec![Command::Send("CQ CQ".to_string())]);
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "CQ CQ"), "{rows:#?}");
        assert!(has(&rows, ">"), "{rows:#?}");
        assert!(!has(&rows, "> CQ"), "input should be cleared: {rows:#?}");
    }

    #[test]
    fn enter_on_an_empty_input_sends_nothing() {
        let mut app = cw_app();

        assert_eq!(app.on_key(key(KeyCode::Enter)), vec![]);
    }

    #[test]
    fn call_placeholder_expands_to_the_configured_callsign_when_sending() {
        let mut app = focus_input(connected_app_in(
            App::with_callsign(Some("KD0RHV".to_string())),
            "CW",
        ));
        type_str(&mut app, "cq cq de {mycall} {mycall} k");

        let commands = app.on_key(key(KeyCode::Enter));

        assert_eq!(
            commands,
            vec![Command::Send("CQ CQ DE KD0RHV KD0RHV K".to_string())]
        );
        assert!(has(&screen(&app, 80, 24), "CQ CQ DE KD0RHV KD0RHV K"));
    }

    #[test]
    fn call_placeholder_without_a_callsign_blocks_the_send_and_keeps_the_input() {
        let mut app = cw_app();
        type_str(&mut app, "de {mycall}");

        let commands = app.on_key(key(KeyCode::Enter));

        assert_eq!(commands, vec![]);
        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "> DE {MYCALL}"), "{rows:#?}");
        assert!(
            app.status().is_some_and(|s| s.contains("MYCALL")),
            "{:?}",
            app.status()
        );
    }

    #[test]
    fn decoded_text_accumulates_on_one_rx_line() {
        let mut app = cw_app();

        rx(&mut app, "TB006CQ CQ ;");
        rx(&mut app, "TB005DE W9;");
        rx(&mut app, "TB003XYZ;");

        assert!(has(&screen(&app, 80, 24), "CQ CQ DE W9XYZ"));
    }

    #[test]
    fn rx_after_a_transmission_starts_a_new_line() {
        let mut app = cw_app();
        rx(&mut app, "TB002CQ;");
        type_str(&mut app, "de me");
        app.on_key(key(KeyCode::Enter));

        rx(&mut app, "TB003R R;");

        let rows = screen(&app, 80, 24);
        let at = |needle: &str| rows.iter().position(|r| r.contains(needle)).unwrap();
        assert!(
            at("CQ") < at("DE ME") && at("DE ME") < at("R R"),
            "{rows:#?}"
        );
    }

    #[test]
    fn a_malformed_tb_reply_shows_in_the_status_line_and_a_good_one_clears_it() {
        let mut app = cw_app();

        rx(&mut app, "TB005AB;");
        assert!(app.status().is_some_and(|s| s.contains("TB")));

        rx(&mut app, "TB000;");
        assert_eq!(app.status(), None);
    }

    #[test]
    fn sending_is_refused_while_the_rig_is_disconnected() {
        let mut app = cw_app();
        type_str(&mut app, "cq");
        app.on_rig(RigEvent::Disconnected("gone".to_string()));

        assert_eq!(app.on_key(key(KeyCode::Enter)), vec![]);

        assert!(
            app.status().is_some_and(|s| s.contains("disconnected")),
            "{:?}",
            app.status()
        );
        assert!(has(&screen(&app, 80, 24), "> CQ"), "input is kept");
    }
}

mod soft_keys {
    use super::*;

    fn app() -> App {
        focus_input(connected_app_in(
            App::with_callsign(Some("KD0RHV".to_string())).with_soft_keys(BTreeMap::from([
                (1, "CQ CQ CQ DE {MYCALL} {MYCALL} K".to_string()),
                (2, "73 TU".to_string()),
            ])),
            "CW",
        ))
    }

    fn f(n: u8) -> KeyEvent {
        key(KeyCode::F(n))
    }

    #[test]
    fn a_function_key_sends_its_message_with_the_callsign_filled_in() {
        let mut app = app();

        let commands = app.on_key(f(1));

        assert_eq!(
            commands,
            vec![Command::Send("CQ CQ CQ DE KD0RHV KD0RHV K".to_string())]
        );
    }

    #[test]
    fn soft_keys_work_from_the_strip_too() {
        let mut app = app();
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.focus(), Focus::Strip);

        let commands = app.on_key(f(2));

        assert_eq!(commands, vec![Command::Send("73 TU".to_string())]);
    }

    #[test]
    fn sending_a_soft_key_leaves_the_half_typed_input_alone() {
        let mut app = app();
        type_str(&mut app, "de w9");

        app.on_key(f(2));

        assert!(has(&screen(&app, 80, 24), "> DE W9"));
    }

    #[test]
    fn a_key_the_user_left_unset_falls_to_the_built_in_default() {
        let mut app = app();

        // F4 is the built-in `{MYCALL}`.
        assert_eq!(app.on_key(f(4)), vec![Command::Send("KD0RHV".to_string())]);
    }

    #[test]
    fn a_message_using_the_callsign_is_blocked_when_none_is_configured() {
        let mut app = focus_input(connected_app_in(
            App::new().with_soft_keys(BTreeMap::from([(1, "DE {MYCALL}".to_string())])),
            "CW",
        ));

        assert_eq!(app.on_key(f(1)), vec![]);

        assert!(app.status().is_some_and(|s| s.contains("MYCALL not set")));
    }

    #[test]
    fn any_missing_variable_refuses_the_whole_send_naming_it() {
        let mut app = app();

        // F3 is the built-in `{THEIRCALL} TU 73 ...`, and no THEIRCALL is known yet.
        assert_eq!(app.on_key(f(3)), vec![]);
        assert!(
            app.status()
                .is_some_and(|s| s.contains("THEIRCALL not set")),
            "{:?}",
            app.status()
        );

        app.set_exchange(Exchange {
            their_call: Some("w1aw".to_string()),
            ..Exchange::default()
        });
        assert_eq!(
            app.on_key(f(3)),
            vec![Command::Send("W1AW TU 73 DE KD0RHV EE".to_string())]
        );
    }

    #[test]
    fn variables_are_read_at_press_time_not_stored() {
        let mut app = app();
        app.set_exchange(Exchange {
            their_call: Some("W1AW".to_string()),
            ..Exchange::default()
        });
        app.on_key(f(3));
        app.set_exchange(Exchange {
            their_call: Some("N0CALL".to_string()),
            ..Exchange::default()
        });

        assert_eq!(
            app.on_key(f(3)),
            vec![Command::Send("N0CALL TU 73 DE KD0RHV EE".to_string())]
        );
    }

    #[test]
    fn an_invalid_stored_message_loads_and_refuses_to_send() {
        let mut app = focus_input(connected_app_in(
            App::with_callsign(Some("KD0RHV".to_string()))
                .with_soft_keys(BTreeMap::from([(1, "DE {CALL}".to_string())])),
            "CW",
        ));

        assert_eq!(app.on_key(f(1)), vec![]);

        assert!(
            app.status().is_some_and(|s| s.contains("use {MYCALL}")),
            "{:?}",
            app.status()
        );
    }

    #[test]
    fn a_missing_plan_set_degrades_and_is_flagged_without_blocking() {
        let mut app = app();

        app.set_plan_soft_keys(Some("nosuch".to_string()), BTreeMap::new());

        assert!(app.status().is_some_and(|s| s.contains("nosuch")));
        assert_eq!(app.resolved_soft_keys().keys[&2].text, "73 TU");
        assert_eq!(app.on_key(f(2)), vec![Command::Send("73 TU".to_string())]);
    }

    #[test]
    fn a_plan_override_beats_the_users_set_for_that_key_only() {
        let mut app = app();
        app.set_plan_soft_keys(None, BTreeMap::from([(2, "QRZ".to_string())]));

        assert_eq!(app.on_key(f(2)), vec![Command::Send("QRZ".to_string())]);
        assert_eq!(
            app.on_key(f(1)),
            vec![Command::Send("CQ CQ CQ DE KD0RHV KD0RHV K".to_string())]
        );
    }

    #[test]
    fn a_named_user_set_replaces_general_for_the_activity() {
        let mut app = app().with_softkey_sets(BTreeMap::from([(
            "pota".to_string(),
            BTreeMap::from([(1, "CQ POTA DE {MYCALL}".to_string())]),
        )]));
        app.set_activity_soft_keys(Some("pota".to_string()));

        assert_eq!(
            app.on_key(f(1)),
            vec![Command::Send("CQ POTA DE KD0RHV".to_string())]
        );
    }

    #[test]
    fn templates_never_send_on_their_own() {
        // Loading, layering, selecting plans and sets, changing the exchange, ticking and
        // rendering produce no rig command that keys the transmitter; only a key press does.
        let mut app = app().with_softkey_sets(BTreeMap::from([(
            "pota".to_string(),
            BTreeMap::from([(1, "CQ".to_string())]),
        )]));
        app.set_activity_soft_keys(Some("pota".to_string()));
        app.set_plan_soft_keys(Some("pota".to_string()), BTreeMap::from([(2, "X".into())]));
        app.set_exchange(Exchange {
            their_call: Some("W1AW".to_string()),
            ..Exchange::default()
        });
        let _ = app.resolved_soft_keys();
        let _ = screen(&app, 80, 24);
        let mut commands = app.tick(std::time::Instant::now());
        commands.extend(app.tick(std::time::Instant::now()));

        assert!(
            commands.iter().all(|c| !matches!(c, Command::Send(_))),
            "{commands:?}"
        );
    }

    #[test]
    fn function_keys_beyond_f6_do_nothing() {
        let mut app = app();

        assert_eq!(app.on_key(f(7)), vec![]);

        assert_eq!(app.status(), None);
    }

    #[test]
    fn soft_keys_do_nothing_outside_cw() {
        let mut app = connected_app_in(
            App::new().with_soft_keys(BTreeMap::from([(1, "CQ".to_string())])),
            "USB",
        );

        assert_eq!(app.on_key(key(KeyCode::F(1))), vec![]);
    }
}

mod panel_visibility {
    use super::*;

    #[test]
    fn the_cw_panel_appears_only_in_cw_and_cwr() {
        for (mode, shown) in [
            ("CW", true),
            ("CWR", true),
            ("USB", false),
            ("LSB", false),
            ("FM", false),
            ("PKTUSB", false),
        ] {
            let app = connected_app_in(App::new(), mode);

            let rows = screen(&app, 80, 24);

            assert_eq!(has(&rows, "Send (Enter)"), shown, "{mode}: {rows:#?}");
        }
    }

    #[test]
    fn the_transcript_and_typed_text_survive_the_panel_being_hidden() {
        let mut app = cw_app();
        rx(&mut app, "TB002CQ;");
        type_str(&mut app, "de me");

        app.on_rig(mode_event("USB"));
        assert!(!has(&screen(&app, 80, 24), "CQ"));
        app.on_rig(mode_event("CWR"));

        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "CQ"), "{rows:#?}");
        assert!(has(&rows, "> DE ME"), "{rows:#?}");
    }

    #[test]
    fn tb_is_polled_each_tick_only_while_the_cw_panel_is_active() {
        let t0 = Instant::now();
        let mut app = connected_app_in(App::new(), "CW");
        let tb = Command::Panel("TB;".to_string());

        assert!(app.tick(t0).contains(&tb));
        assert!(app.tick(t0 + Duration::from_millis(100)).contains(&tb));

        app.on_rig(mode_event("USB"));
        assert!(!app.tick(t0 + Duration::from_millis(200)).contains(&tb));
    }

    #[test]
    fn a_panel_maps_the_rig_mode_to_a_logbook_mode_with_default_rst() {
        for mode in ["CW", "CWR"] {
            let app = connected_app_in(App::new(), mode);

            let log_mode = app.log_mode().unwrap();

            assert_eq!(log_mode.mode, "CW");
            assert_eq!(log_mode.submode, None);
            assert_eq!(log_mode.default_rst.as_deref(), Some("599"));
            assert_eq!(app.log_defaults().unwrap().extra_adif, vec![]);
        }
        assert_eq!(connected_app_in(App::new(), "USB").log_mode(), None);
    }
}

mod keys {
    use super::*;

    #[test]
    fn the_app_starts_with_the_strip_focused() {
        assert_eq!(App::new().focus(), Focus::Strip);
        assert_eq!(cw_app().focus(), Focus::Panel);
        assert_eq!(
            connected_app_in(App::new(), "CW").focus(),
            Focus::Strip,
            "entering CW does not steal focus"
        );
    }

    #[test]
    fn q_quits_from_the_strip_but_is_text_in_the_input_line() {
        let mut app = connected_app_in(App::new(), "CW");
        assert_eq!(app.on_key(key(KeyCode::Char('q'))), vec![]);
        assert!(has(&screen(&app, 80, 24), "Quit rig-tui?"));
        assert_eq!(app.on_key(key(KeyCode::Char('y'))), vec![Command::Quit]);

        let mut app = focus_input(connected_app_in(App::new(), "CW"));
        assert_eq!(app.on_key(key(KeyCode::Char('q'))), vec![]);
        assert!(has(&screen(&app, 80, 24), "> Q"));
    }

    #[test]
    fn ctrl_c_always_asks_and_a_second_one_quits() {
        let mut overlaid = App::new();
        overlaid.open_overlay(Box::new(TestOverlay));
        for mut app in [App::new(), cw_app(), overlaid] {
            assert_eq!(app.on_key(ctrl('c')), vec![]);
            assert_eq!(app.on_key(ctrl('c')), vec![Command::Quit]);
        }
    }

    #[test]
    fn tab_moves_focus_between_the_strip_and_the_input() {
        let mut app = connected_app_in(App::new(), "CW");

        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.focus(), Focus::Panel);

        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.focus(), Focus::Strip);
    }

    #[test]
    fn escape_returns_focus_to_the_strip_and_never_quits() {
        let mut app = cw_app();

        assert_eq!(app.on_key(key(KeyCode::Esc)), vec![]);
        assert_eq!(app.focus(), Focus::Strip);

        assert_eq!(app.on_key(key(KeyCode::Esc)), vec![]);
    }

    #[test]
    fn the_strip_keeps_focus_in_a_mode_with_no_panel() {
        let mut app = connected_app_in(App::new(), "USB");

        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.focus(), Focus::Strip);

        // Printable keys are commands, not swallowed text.
        assert_eq!(app.on_key(key(KeyCode::Char('x'))), vec![]);
    }

    #[test]
    fn focus_returns_to_the_strip_when_the_panel_goes_away() {
        let mut app = cw_app();

        app.on_rig(mode_event("USB"));
        app.on_rig(mode_event("CW"));

        assert_eq!(app.focus(), Focus::Strip);
    }

    struct TestOverlay;

    impl Overlay for TestOverlay {
        fn on_key(&mut self, key: KeyEvent) -> OverlayAction {
            if key.code == KeyCode::Esc {
                OverlayAction::Close
            } else {
                OverlayAction::Keep
            }
        }

        fn render(&self, frame: &mut Frame, area: Rect) {
            frame.render_widget(ratatui::widgets::Paragraph::new("TEST MODAL"), area);
        }
    }

    #[test]
    fn an_open_modal_gets_every_key_before_the_app() {
        let mut app = cw_app();
        app.open_overlay(Box::new(TestOverlay));

        // `q`, F-keys and text reach neither the quit command nor the input line.
        assert_eq!(app.on_key(key(KeyCode::Char('q'))), vec![]);
        assert_eq!(app.on_key(key(KeyCode::F(1))), vec![]);
        assert!(has(&screen(&app, 80, 24), "TEST MODAL"));
        app.on_key(key(KeyCode::Esc));
        assert!(!app.has_overlay());
        assert!(!has(&screen(&app, 80, 24), "> Q"));
    }

    #[test]
    fn escape_closes_the_modal_first_and_only_then_leaves_the_input() {
        let mut app = cw_app();
        app.open_overlay(Box::new(TestOverlay));

        app.on_key(key(KeyCode::Esc));
        assert!(!app.has_overlay());
        assert_eq!(
            app.focus(),
            Focus::Panel,
            "the first Esc only closed the modal"
        );

        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.focus(), Focus::Strip);
    }
}

mod connection {
    use super::*;

    #[test]
    fn the_app_starts_disconnected_and_asks_to_connect_at_once() {
        let mut app = App::new();

        assert!(!app.connected());
        assert_eq!(app.tick(Instant::now()), vec![Command::Connect]);
    }

    #[test]
    fn it_retries_every_few_seconds_while_disconnected() {
        let t0 = Instant::now();
        let mut app = App::new();
        app.tick(t0);
        app.on_rig(RigEvent::Disconnected("refused".to_string()));

        assert_eq!(app.tick(t0 + Duration::from_secs(1)), vec![]);
        assert_eq!(
            app.tick(t0 + RECONNECT_EVERY),
            vec![Command::Connect],
            "retry is due"
        );
        assert_eq!(
            app.tick(t0 + RECONNECT_EVERY + Duration::from_secs(1)),
            vec![]
        );
    }

    #[test]
    fn a_connection_stops_the_retries_and_starts_polling_the_rig_state() {
        let t0 = Instant::now();
        let mut app = App::new();
        app.on_rig(RigEvent::Connected);

        assert_eq!(app.tick(t0), vec![Command::PollState, Command::PollSMeter]);
        assert_eq!(app.tick(t0 + Duration::from_millis(100)), vec![]);
        assert_eq!(
            app.tick(t0 + STATE_POLL_EVERY),
            vec![Command::PollState, Command::PollSMeter]
        );
    }

    #[test]
    fn losing_the_rig_resumes_the_retries() {
        let t0 = Instant::now();
        let mut app = connected_app_in(App::new(), "USB");
        app.on_rig(RigEvent::Disconnected("rigctld timed out".to_string()));

        assert_eq!(app.tick(t0), vec![Command::Connect]);
        assert_eq!(app.status(), Some("rigctld timed out"));
    }

    #[test]
    fn a_failed_send_shows_its_error() {
        let mut app = cw_app();

        app.on_rig(RigEvent::SendFailed(
            "send failed: rigctld error -1".to_string(),
        ));

        assert_eq!(app.status(), Some("send failed: rigctld error -1"));
    }

    #[test]
    fn the_strip_says_disconnected_and_then_shows_what_the_rig_reported() {
        let mut app = App::new();
        assert!(has(&screen(&app, 80, 24), "disconnected"));

        app.on_rig(RigEvent::Connected);
        app.on_rig(RigEvent::Freq(14_074_000));
        app.on_rig(mode_event("CW"));

        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "14.074.000"), "{rows:#?}");
        assert!(has(&rows, "CW 500"), "{rows:#?}");
        assert!(!has(&rows, "disconnected"), "{rows:#?}");
    }
}

mod status_line {
    use super::*;

    #[test]
    fn a_message_goes_away_by_itself() {
        let mut app = App::new();
        let t0 = Instant::now();
        app.tick(t0);
        app.set_status(Some("split is off".to_string()));
        app.tick(t0);
        app.tick(t0 + STATUS_TTL - Duration::from_millis(1));
        assert_eq!(app.status(), Some("split is off"));

        app.tick(t0 + STATUS_TTL);

        assert_eq!(app.status(), None);
    }

    #[test]
    fn a_newer_message_gets_its_own_full_time() {
        let mut app = App::new();
        let t0 = Instant::now();
        app.set_status(Some("first".to_string()));
        app.tick(t0);
        let later = t0 + STATUS_TTL - Duration::from_secs(1);
        app.on_rig(RigEvent::SetFailed("set frequency failed".to_string()));
        app.tick(later);

        // The first message's time is up, but the second has only just appeared.
        app.tick(t0 + STATUS_TTL + Duration::from_secs(1));
        assert_eq!(app.status(), Some("set frequency failed"));

        app.tick(later + STATUS_TTL);
        assert_eq!(app.status(), None);
    }

    #[test]
    fn why_the_rig_is_unreachable_stays_while_it_is_and_goes_when_it_connects() {
        let mut app = App::new();
        let t0 = Instant::now();
        let why = "cannot reach rigctld at 127.0.0.1:4532: connection refused";
        app.on_rig(RigEvent::Disconnected(why.to_string()));
        app.tick(t0);

        app.tick(t0 + STATUS_TTL * 10);
        assert_eq!(app.status(), Some(why));

        app.on_rig(RigEvent::Connected);
        assert_eq!(app.status(), None);
    }

    #[test]
    fn a_standing_reason_is_marked_and_a_one_off_message_is_not() {
        let mut app = App::new();
        app.on_rig(RigEvent::Disconnected("cannot reach rigctld".to_string()));
        let rows = screen(&app, 100, 30);
        assert!(has(&rows, "\u{26a0} cannot reach rigctld"), "{rows:#?}");

        app.set_status(Some("split is off".to_string()));
        let rows = screen(&app, 100, 30);
        assert!(has(&rows, "split is off"), "{rows:#?}");
        assert!(!has(&rows, "\u{26a0} split is off"), "{rows:#?}");
    }

    #[test]
    fn a_message_about_something_else_does_not_outlive_its_time_while_disconnected() {
        let mut app = App::new();
        let t0 = Instant::now();
        app.on_rig(RigEvent::Disconnected("cannot reach rigctld".to_string()));
        app.tick(t0);
        app.set_status(Some("not connected".to_string()));
        app.tick(t0);

        app.tick(t0 + STATUS_TTL);

        // It falls back to the standing reason rather than blanking the line.
        assert_eq!(app.status(), Some("cannot reach rigctld"));
    }
}
