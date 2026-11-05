//! The key layer: the key table, the focus rule, the Chords and the quit confirmation.

mod common;

use common::*;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rig_tui::keys::{self, Action, Key, KeyEntry, Scope, validate_activity};
use rig_tui::{App, Command};

fn has(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|r| r.contains(needle))
}

fn plain(c: char) -> KeyEntry {
    KeyEntry {
        scope: Scope::Plain,
        key: Key::plain(KeyCode::Char(c)),
        action: Action::Activity("test"),
        label: "test",
        blocked: None,
    }
}

mod table {
    use super::*;

    #[test]
    fn no_two_shared_entries_share_a_key() {
        let table = keys::shared();
        for (i, a) in table.iter().enumerate() {
            for b in &table[i + 1..] {
                assert_ne!(a.key, b.key, "{} and {} collide", a.label, b.label);
            }
        }
    }

    #[test]
    fn the_chords_are_exactly_the_documented_set() {
        let mut chords: Vec<(char, Action)> = keys::shared()
            .iter()
            .filter(|e| e.scope == Scope::Chord)
            .map(|e| match e.key.code {
                KeyCode::Char(c) => {
                    assert_eq!(e.key.mods, KeyModifiers::CONTROL);
                    (c, e.action)
                }
                other => panic!("chord on {other:?}"),
            })
            .collect();
        chords.sort_by_key(|(c, _)| *c);
        assert_eq!(
            chords,
            vec![
                ('b', Action::Bandplan),
                ('c', Action::Quit),
                ('g', Action::GoTo),
                ('j', Action::SessionList),
                ('k', Action::Keybindings),
                ('l', Action::LogForm),
                ('o', Action::Config),
                ('r', Action::RigSwitcher),
            ]
        );
    }

    #[test]
    fn l_capital_l_and_c_are_aliases_of_their_chords() {
        let alias = |c| {
            keys::shared()
                .iter()
                .find(|e| e.scope == Scope::Alias && e.key == Key::plain(KeyCode::Char(c)))
                .map(|e| e.action)
        };
        assert_eq!(alias('l'), Some(Action::LogForm));
        assert_eq!(alias('L'), Some(Action::SessionList));
        assert_eq!(alias('c'), Some(Action::Config));
    }

    #[test]
    fn an_activity_may_not_reuse_a_shared_plain_key() {
        for c in ['f', 'm', 'M', 'v', 's', 'S', 'l', 'L', 'c', 'q', '5'] {
            let err = validate_activity("Test", &[plain(c)]).unwrap_err();
            assert!(err.contains(&c.to_string()), "{err}");
        }
        for code in [KeyCode::Left, KeyCode::Right, KeyCode::Up, KeyCode::Down] {
            let entry = KeyEntry {
                key: Key::plain(code),
                ..plain('x')
            };
            assert!(validate_activity("Test", &[entry]).is_err(), "{code:?}");
        }
    }

    #[test]
    fn an_activity_may_not_bind_a_chord_or_a_soft_key() {
        let chord = KeyEntry {
            key: Key::ctrl('g'),
            ..plain('x')
        };
        assert!(validate_activity("Test", &[chord]).is_err());
        let soft = KeyEntry {
            key: Key::plain(KeyCode::F(1)),
            ..plain('x')
        };
        assert!(validate_activity("Test", &[soft]).is_err());
    }

    #[test]
    fn an_activity_may_not_bind_one_key_twice() {
        assert!(validate_activity("Test", &[plain('x'), plain('x')]).is_err());
        assert!(validate_activity("Test", &[plain('x'), plain('y')]).is_ok());
    }

    #[test]
    fn z_is_reserved_for_satellite() {
        assert!(validate_activity("Satellite", &[plain('Z')]).is_ok());
        let err = validate_activity("Activator", &[plain('Z')]).unwrap_err();
        assert!(err.contains("Satellite"), "{err}");
    }

    #[test]
    fn the_hosted_activities_respect_the_reserved_keys() {
        assert_eq!(App::new().check_activity_keys(), Ok(()));
    }
}

mod chords {
    use super::*;

    fn quiet(app: &mut App, key: KeyEvent) {
        assert_eq!(app.on_key(key), vec![]);
    }

    #[test]
    fn ctrl_j_and_ctrl_o_open_their_overlays_from_the_strip_and_from_a_text_area() {
        for mut app in [connected_app_in(App::new(), "CW"), cw_app()] {
            quiet(&mut app, ctrl('j'));
            assert!(has(&screen(&app, 80, 24), "Session"));
            app.on_key(key(KeyCode::Esc));
            quiet(&mut app, ctrl('o'));
            assert!(has(&screen(&app, 80, 24), "Config"));
        }
    }

    #[test]
    fn ctrl_l_does_not_type_into_a_text_area() {
        let mut app = cw_app();
        quiet(&mut app, ctrl('l'));
        // No callsign or log path in this app, so the gate says why.
        assert!(app.status().is_some());
        assert!(!has(&screen(&app, 80, 24), "> L"));
    }

    #[test]
    fn ctrl_r_opens_the_rig_switcher_from_a_text_area_without_typing() {
        let mut app = cw_app();
        quiet(&mut app, ctrl('r'));
        assert!(app.has_overlay());
        assert!(has(&screen(&app, 80, 24), "Rigs"));
        app.on_key(key(KeyCode::Esc));
        assert!(has(&screen(&app, 80, 24), ">"));
        assert!(!has(&screen(&app, 80, 24), "> R"));
    }

    #[test]
    fn ctrl_g_and_ctrl_k_open_their_overlays_from_a_text_area() {
        for c in ['g', 'k'] {
            let mut app = cw_app();
            quiet(&mut app, ctrl(c));
            assert!(app.has_overlay(), "Ctrl-{c} opened nothing");
            app.on_key(key(KeyCode::Esc));
            assert!(!app.has_overlay());
            // The input line is untouched.
            assert!(has(&screen(&app, 80, 24), ">"));
        }
    }

    #[test]
    fn unbound_control_keys_do_not_type_into_the_input() {
        let mut app = cw_app();
        quiet(&mut app, ctrl('x'));
        assert!(!has(&screen(&app, 80, 24), "> X"));
    }

    #[test]
    fn a_chord_works_over_the_session_list() {
        let mut app = connected_app_in(App::new(), "CW");
        quiet(&mut app, ctrl('j'));
        quiet(&mut app, ctrl('o'));
        assert!(has(&screen(&app, 80, 24), "Config"));
    }

    #[test]
    fn chords_open_over_every_modal_that_holds_nothing_to_lose() {
        // (opening chord, text the modal shows)
        for (opener, shown) in [
            ('g', "Go to"),
            ('k', "Keybindings"),
            ('b', "Band"),
            ('r', "Rigs"),
            ('j', "Session"),
        ] {
            for (chord, wanted) in [('o', "Config"), ('r', "Rigs")] {
                let mut app = connected_app_in(App::new(), "CW");
                quiet(&mut app, ctrl(opener));
                assert!(
                    has(&screen(&app, 80, 24), shown),
                    "Ctrl-{opener} did not open {shown}"
                );
                quiet(&mut app, ctrl(chord));
                assert!(
                    has(&screen(&app, 80, 24), wanted),
                    "Ctrl-{chord} over {shown}"
                );
                // The first modal is gone, not hidden underneath.
                if opener != chord {
                    app.on_key(key(KeyCode::Esc));
                    assert!(!app.has_overlay(), "{shown} was left under Ctrl-{chord}");
                }
            }
        }
    }

    #[test]
    fn chords_stay_blocked_over_modals_with_unsaved_input() {
        let mut app = App::new();
        quiet(&mut app, ctrl('o'));
        quiet(&mut app, ctrl('r'));
        assert!(has(&screen(&app, 80, 24), "Config"));
        assert!(!has(&screen(&app, 80, 24), "Rigs"));

        let mut app = App::new();
        quiet(&mut app, ctrl('g'));
        type_str(&mut app, "plans");
        quiet(&mut app, key(KeyCode::Enter));
        assert!(has(&screen(&app, 100, 30), "Activation plans"));
        quiet(&mut app, ctrl('r'));
        assert!(has(&screen(&app, 100, 30), "Activation plans"));
        assert!(!has(&screen(&app, 100, 30), "Rigs"));
    }

    #[test]
    fn plain_aliases_work_on_the_strip_and_are_text_in_the_input() {
        let mut app = connected_app_in(App::new(), "CW");
        quiet(&mut app, key(KeyCode::Char('L')));
        assert!(has(&screen(&app, 80, 24), "Session"));
        app.on_key(key(KeyCode::Esc));
        quiet(&mut app, key(KeyCode::Char('c')));
        assert!(has(&screen(&app, 80, 24), "Config"));

        let mut app = cw_app();
        type_str(&mut app, "lLcfmvsS5");
        assert!(has(&screen(&app, 80, 24), "> LLCFMVSS5"));
        assert!(!app.has_overlay());
    }
}

mod quit {
    use super::*;

    #[test]
    fn ctrl_c_asks_from_the_strip_the_input_and_any_overlay() {
        let mut overlay_app = App::new();
        overlay_app.on_key(ctrl('o'));
        for mut app in [App::new(), cw_app(), overlay_app] {
            assert_eq!(app.on_key(ctrl('c')), vec![]);
            assert!(has(&screen(&app, 80, 24), "Quit rig-tui"));
        }
    }

    #[test]
    fn y_or_a_second_ctrl_c_quits() {
        let mut app = App::new();
        app.on_key(ctrl('c'));
        assert_eq!(app.on_key(key(KeyCode::Char('y'))), vec![Command::Quit]);
        let mut app = App::new();
        app.on_key(ctrl('c'));
        assert_eq!(app.on_key(ctrl('c')), vec![Command::Quit]);
    }

    #[test]
    fn n_esc_and_enter_cancel_and_other_keys_do_nothing() {
        for cancel in [KeyCode::Char('n'), KeyCode::Esc, KeyCode::Enter] {
            let mut app = App::new();
            app.on_key(ctrl('c'));
            assert_eq!(app.on_key(key(KeyCode::Char('x'))), vec![]);
            assert!(
                has(&screen(&app, 80, 24), "Quit rig-tui"),
                "a stray key closed it"
            );
            assert_eq!(app.on_key(key(cancel)), vec![]);
            assert!(!has(&screen(&app, 80, 24), "Quit rig-tui"));
        }
    }

    #[test]
    fn cancelling_returns_exactly_where_you_were() {
        let mut app = cw_app();
        type_str(&mut app, "cq de");
        app.on_key(ctrl('c'));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.focus(), rig_tui::Focus::Panel);
        assert!(has(&screen(&app, 80, 24), "> CQ DE"));

        let mut app = App::new();
        app.on_key(ctrl('o'));
        app.on_key(ctrl('c'));
        app.on_key(key(KeyCode::Char('n')));
        assert!(has(&screen(&app, 80, 24), "Config"), "the overlay is back");
    }

    #[test]
    fn esc_never_quits() {
        let mut app = App::new();
        assert_eq!(app.on_key(key(KeyCode::Esc)), vec![]);
        app.on_key(ctrl('c'));
        assert_eq!(app.on_key(key(KeyCode::Esc)), vec![]);
        assert_eq!(app.on_key(key(KeyCode::Esc)), vec![]);
    }

    #[test]
    fn q_asks_only_in_general_with_no_overlay_and_no_text_area() {
        let mut app = connected_app_in(App::new(), "CW");
        assert_eq!(app.on_key(key(KeyCode::Char('q'))), vec![]);
        assert!(has(&screen(&app, 80, 24), "Quit rig-tui"));

        let mut app = cw_app();
        type_str(&mut app, "q");
        assert!(has(&screen(&app, 80, 24), "> Q"));
        assert!(!has(&screen(&app, 80, 24), "Quit rig-tui"));

        let mut app = App::new();
        app.on_key(ctrl('j'));
        app.on_key(key(KeyCode::Char('q')));
        assert!(!has(&screen(&app, 80, 24), "Quit rig-tui"));
    }

    #[test]
    fn a_clean_app_lists_nothing_to_lose() {
        let mut app = App::new();
        app.on_key(ctrl('c'));
        assert!(!has(&screen(&app, 80, 24), "lost"));
    }

    #[test]
    fn unsent_cw_input_is_named() {
        let mut app = cw_app();
        type_str(&mut app, "cq");
        app.on_key(ctrl('c'));
        assert!(has(&screen(&app, 80, 24), "Unsent CW"));
    }

    #[test]
    fn unsaved_contact_text_is_named_and_the_form_survives_a_cancel() {
        let dir = std::env::temp_dir().join(format!("rig-tui-keys-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = connected_app_in(
            App::with_callsign(Some("K9TST".to_string())).with_log_path(Some(dir.join("log.adi"))),
            "CW",
        );
        app.on_rig(rig_tui::RigEvent::Split {
            enabled: false,
            tx_vfo: "VFOA".to_string(),
        });
        app.on_key(ctrl('l'));
        assert!(
            has(&screen(&app, 80, 24), "Log contact"),
            "{:?}",
            app.status()
        );

        app.on_key(ctrl('c'));
        assert!(
            !has(&screen(&app, 80, 24), "Unsaved Contact"),
            "form is empty"
        );
        app.on_key(key(KeyCode::Char('n')));

        type_str(&mut app, "w1aw");
        app.on_key(ctrl('c'));
        assert!(has(&screen(&app, 80, 24), "Unsaved Contact"));
        app.on_key(key(KeyCode::Esc));
        assert!(has(&screen(&app, 80, 24), "W1AW"), "the form kept its text");
    }

    #[test]
    fn it_never_refuses() {
        let mut app = cw_app();
        type_str(&mut app, "cq");
        app.on_key(ctrl('c'));
        assert_eq!(app.on_key(key(KeyCode::Char('y'))), vec![Command::Quit]);
    }
}
