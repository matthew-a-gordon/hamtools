//! The rig-state strip and the strip's keys, through `App` and rendered buffers.

mod common;

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use common::*;
use ratatui::crossterm::event::KeyCode;
use rig_tui::{App, Command, Feature, RigEvent, RigMode};

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

fn press(app: &mut App, text: &str) {
    for c in text.chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
}

/// Connected, in USB on 14.074 MHz, VFO A, split off, receiving, S9+20.
fn rig_app() -> App {
    let mut app = connected_app_in(App::new(), "USB");
    app.on_rig(RigEvent::Vfo("VFOA".to_string()));
    app.on_rig(RigEvent::Split {
        enabled: false,
        tx_vfo: "VFOA".to_string(),
    });
    app.on_rig(RigEvent::Ptt(false));
    app.on_rig(RigEvent::Strength(20));
    app
}

mod strip {
    use super::*;

    #[test]
    fn connected_shows_both_rows() {
        let mut app = rig_app();
        app.set_utc(UNIX_EPOCH + Duration::from_secs(86_400 * 19_000 + 45_296)); // 12:34:56

        let rows = screen(&app, 80, 24);

        assert!(has(&rows, "14.074.000"), "{rows:#?}");
        assert!(has(&rows, "USB 500"), "{rows:#?}");
        assert!(has(&rows, "step 1 kHz"), "{rows:#?}");
        assert!(has(&rows, "connected"), "{rows:#?}");
        assert!(has(&rows, "VFO A"), "{rows:#?}");
        assert!(has(&rows, "split off"), "{rows:#?}");
        assert!(has(&rows, "RX"), "{rows:#?}");
        assert!(has(&rows, "S9+20"), "{rows:#?}");
        assert!(has(&rows, "12:34:56Z"), "{rows:#?}");
    }

    #[test]
    fn disconnected_says_so() {
        let app = App::new();

        let rows = screen(&app, 80, 24);

        assert!(has(&rows, "disconnected"), "{rows:#?}");
        assert!(!has(&rows, "14.074"), "{rows:#?}");
    }

    #[test]
    fn s_meter_reads_in_s_units_below_s9_and_db_over() {
        let mut app = rig_app();
        for (db, shown) in [(-54, "S0"), (-6, "S8"), (0, "S9"), (40, "S9+40")] {
            app.on_rig(RigEvent::Strength(db));
            assert!(has(&screen(&app, 80, 24), shown), "{db} dB -> {shown}");
        }
    }

    #[test]
    fn a_value_older_than_two_seconds_shows_as_a_dash() {
        let mut app = rig_app();
        let t0 = Instant::now();
        app.tick(t0);
        app.on_rig(RigEvent::Freq(14_074_000));
        app.on_rig(RigEvent::Strength(20));

        app.tick(t0 + Duration::from_millis(1_500));
        let fresh = screen(&app, 80, 24);
        app.tick(t0 + Duration::from_millis(2_500));
        let stale = screen(&app, 80, 24);

        assert!(has(&fresh, "14.074.000"), "{fresh:#?}");
        assert!(!has(&stale, "14.074.000"), "{stale:#?}");
        assert!(!has(&stale, "S9+20"), "{stale:#?}");
        assert!(!has(&stale, "USB"), "{stale:#?}");
        assert_eq!(app.frequency(), None);
    }

    #[test]
    fn transmitting_shows_tx() {
        let mut app = rig_app();

        app.on_rig(RigEvent::Ptt(true));

        assert!(has(&screen(&app, 80, 24), "TX"));
    }

    #[test]
    fn split_on_shows_the_transmit_vfo_and_frequency() {
        let mut app = rig_app();

        app.on_rig(RigEvent::Split {
            enabled: true,
            tx_vfo: "VFOB".to_string(),
        });
        app.on_rig(RigEvent::SplitFreq(14_075_500));

        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "split B 14.075.500"), "{rows:#?}");
    }

    #[test]
    fn unsupported_vfo_and_split_say_so() {
        let mut app = rig_app();

        app.on_rig(RigEvent::Unsupported(Feature::Vfo));
        app.on_rig(RigEvent::Unsupported(Feature::Split));

        let rows = screen(&app, 80, 24);
        assert!(has(&rows, "VFO not supported"), "{rows:#?}");
        assert!(has(&rows, "split not supported"), "{rows:#?}");
    }
}

/// The one place that answers "where is the rig transmitting?", for the strip and the log form.
mod transmit_frequency {
    use super::*;

    #[test]
    fn without_split_it_is_the_receive_frequency() {
        let app = rig_app();

        assert_eq!(app.transmit_frequency(), Some(14_074_000));
    }

    #[test]
    fn with_split_on_it_is_the_reported_split_transmit_frequency() {
        let mut app = rig_app();
        app.on_rig(RigEvent::Split {
            enabled: true,
            tx_vfo: "VFOB".to_string(),
        });
        app.on_rig(RigEvent::SplitFreq(14_075_500));

        assert_eq!(app.transmit_frequency(), Some(14_075_500));
    }

    #[test]
    fn with_split_on_and_no_transmit_frequency_reported_it_is_unknown_not_the_receive_one() {
        let mut app = rig_app();
        app.on_rig(RigEvent::Split {
            enabled: true,
            tx_vfo: "VFOB".to_string(),
        });

        assert_eq!(app.transmit_frequency(), None);
    }

    #[test]
    fn a_stale_split_transmit_frequency_is_unknown() {
        let mut app = rig_app();
        let t0 = Instant::now();
        app.tick(t0);
        app.on_rig(RigEvent::Split {
            enabled: true,
            tx_vfo: "VFOB".to_string(),
        });
        app.on_rig(RigEvent::SplitFreq(14_075_500));

        app.tick(t0 + Duration::from_secs(3));

        assert_eq!(app.transmit_frequency(), None);
    }
}

mod keys {
    use super::*;

    #[test]
    fn f_then_kilohertz_then_enter_sets_the_frequency() {
        let mut app = rig_app();

        press(&mut app, "f7030");
        let commands = app.on_key(key(KeyCode::Enter));

        assert_eq!(commands, vec![Command::SetFreq(7_030_000)]);
    }

    #[test]
    fn digits_alone_start_a_frequency_entry_and_a_dot_means_megahertz() {
        let mut app = rig_app();

        press(&mut app, "7.0305");
        assert!(has(&screen(&app, 80, 24), "7.0305"));
        let commands = app.on_key(key(KeyCode::Enter));

        assert_eq!(commands, vec![Command::SetFreq(7_030_500)]);
    }

    #[test]
    fn escape_cancels_an_entry_and_backspace_edits_it() {
        let mut app = rig_app();
        press(&mut app, "f70");
        app.on_key(key(KeyCode::Backspace));
        press(&mut app, "3");
        assert_eq!(
            app.on_key(key(KeyCode::Enter)),
            vec![Command::SetFreq(73_000)]
        );

        press(&mut app, "f9");
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.on_key(key(KeyCode::Enter)), vec![]);
    }

    #[test]
    fn a_bad_entry_shows_an_error_and_sends_nothing() {
        let mut app = rig_app();

        press(&mut app, "f1.2.3");
        let commands = app.on_key(key(KeyCode::Enter));

        assert_eq!(commands, vec![]);
        assert!(app.status().is_some_and(|s| s.contains("frequency")));
    }

    #[test]
    fn entry_keys_do_not_quit_or_leak_to_commands() {
        let mut app = rig_app();
        press(&mut app, "f");

        assert_eq!(app.on_key(key(KeyCode::Char('q'))), vec![]);
        assert_eq!(app.on_key(key(KeyCode::Char('m'))), vec![]);
    }

    #[test]
    fn frequency_entry_is_refused_while_disconnected() {
        let mut app = App::new();

        press(&mut app, "f");

        assert!(app.status().is_some_and(|s| s.contains("not connected")));
        assert_eq!(app.on_key(key(KeyCode::Enter)), vec![]);
    }

    #[test]
    fn left_and_right_tune_by_the_step_and_up_and_down_change_it() {
        let mut app = rig_app();

        assert_eq!(
            app.on_key(key(KeyCode::Right)),
            vec![Command::SetFreq(14_075_000)]
        );
        assert_eq!(
            app.on_key(key(KeyCode::Left)),
            vec![Command::SetFreq(14_073_000)]
        );
        app.on_key(key(KeyCode::Up));
        assert!(has(&screen(&app, 80, 24), "step 10 kHz"));
        assert_eq!(
            app.on_key(key(KeyCode::Right)),
            vec![Command::SetFreq(14_084_000)]
        );
        app.on_key(key(KeyCode::Down));
        app.on_key(key(KeyCode::Down));
        assert!(has(&screen(&app, 80, 24), "step 100 Hz"));
    }

    #[test]
    fn tuning_needs_a_current_frequency() {
        let mut app = App::new();
        app.on_rig(RigEvent::Connected);

        assert_eq!(app.on_key(key(KeyCode::Right)), vec![]);
        assert!(app.status().is_some());
    }

    #[test]
    fn m_and_shift_m_cycle_the_mode_forward_and_back() {
        let mut app = rig_app(); // USB

        assert_eq!(
            app.on_key(key(KeyCode::Char('m'))),
            vec![Command::SetMode {
                mode: RigMode::Cw,
                passband: 0
            }]
        );
        assert_eq!(
            app.on_key(key(KeyCode::Char('M'))),
            vec![Command::SetMode {
                mode: RigMode::Lsb,
                passband: 0
            }]
        );
    }

    #[test]
    fn v_selects_the_other_vfo() {
        let mut app = rig_app();

        assert_eq!(
            app.on_key(key(KeyCode::Char('v'))),
            vec![Command::SetVfo("VFOB".to_string())]
        );
        app.on_rig(RigEvent::Vfo("VFOB".to_string()));
        assert_eq!(
            app.on_key(key(KeyCode::Char('v'))),
            vec![Command::SetVfo("VFOA".to_string())]
        );
    }

    #[test]
    fn s_toggles_split_with_the_other_vfo_transmitting() {
        let mut app = rig_app();

        assert_eq!(
            app.on_key(key(KeyCode::Char('s'))),
            vec![Command::SetSplit {
                enabled: true,
                tx_vfo: "VFOB".to_string()
            }]
        );
        app.on_rig(RigEvent::Split {
            enabled: true,
            tx_vfo: "VFOB".to_string(),
        });
        assert_eq!(
            app.on_key(key(KeyCode::Char('s'))),
            vec![Command::SetSplit {
                enabled: false,
                tx_vfo: "VFOA".to_string()
            }]
        );
    }

    #[test]
    fn shift_s_sets_the_split_transmit_frequency_only_while_split_is_on() {
        let mut app = rig_app();
        press(&mut app, "S");
        assert!(app.status().is_some_and(|s| s.contains("split is off")));

        app.on_rig(RigEvent::Split {
            enabled: true,
            tx_vfo: "VFOB".to_string(),
        });
        press(&mut app, "S14075");
        let commands = app.on_key(key(KeyCode::Enter));

        assert_eq!(commands, vec![Command::SetSplitFreq(14_075_000)]);
    }

    #[test]
    fn unsupported_controls_say_so_and_emit_nothing() {
        let mut app = rig_app();
        app.on_rig(RigEvent::Unsupported(Feature::Vfo));
        app.on_rig(RigEvent::Unsupported(Feature::Split));

        assert_eq!(app.on_key(key(KeyCode::Char('v'))), vec![]);
        assert!(app.status().is_some_and(|s| s.contains("not supported")));
        app.set_status(None);
        assert_eq!(app.on_key(key(KeyCode::Char('s'))), vec![]);
        assert!(app.status().is_some_and(|s| s.contains("not supported")));
    }

    #[test]
    fn a_failed_set_shows_its_error() {
        let mut app = rig_app();

        app.on_rig(RigEvent::SetFailed(
            "set frequency failed: rigctld error -1".to_string(),
        ));

        assert!(has(
            &screen(&app, 80, 24),
            "set frequency failed: rigctld error -1"
        ));
    }

    #[test]
    fn controls_do_nothing_while_disconnected() {
        let mut app = App::new();

        for c in ['m', 'M', 'v', 's'] {
            assert_eq!(app.on_key(key(KeyCode::Char(c))), vec![], "{c}");
        }
        assert_eq!(app.on_key(key(KeyCode::Right)), vec![]);
    }

    #[test]
    fn strip_keys_in_a_cw_panel_go_to_the_input_when_it_has_focus() {
        let mut app = cw_app();

        type_str(&mut app, "m7v");

        assert!(has(&screen(&app, 80, 24), "> M7V"));
    }

    #[test]
    fn a_reconnect_forgets_which_controls_were_unsupported() {
        let mut app = rig_app();
        app.on_rig(RigEvent::Unsupported(Feature::Vfo));

        app.on_rig(RigEvent::Disconnected("lost".to_string()));
        app.on_rig(RigEvent::Connected);

        assert!(!has(&screen(&app, 80, 24), "VFO not supported"));
    }
}

mod polling {
    use super::*;

    #[test]
    fn the_s_meter_is_polled_more_often_than_the_rest() {
        let mut app = connected_app_in(App::new(), "USB");
        let t0 = Instant::now();

        assert_eq!(app.tick(t0), vec![Command::PollState, Command::PollSMeter]);
        assert_eq!(
            app.tick(t0 + rig_tui::SMETER_POLL_EVERY),
            vec![Command::PollSMeter]
        );
        assert_eq!(
            app.tick(t0 + rig_tui::STATE_POLL_EVERY),
            vec![Command::PollState, Command::PollSMeter]
        );
    }

    #[test]
    fn the_utc_clock_is_whatever_the_app_was_told() {
        let mut app = App::new();
        assert!(has(&screen(&app, 80, 24), "--:--:--Z"));

        app.set_utc(UNIX_EPOCH + Duration::from_secs(86_400 * 3 + 3_661));

        assert!(has(&screen(&app, 80, 24), "01:01:01Z"));
        assert_eq!(
            app.utc(),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(86_400 * 3 + 3_661))
        );
    }
}
