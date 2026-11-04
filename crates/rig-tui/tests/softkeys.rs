//! The Soft key template seam: validation, expansion and layering, all pure.

use std::collections::BTreeMap;

use rig_tui::softkeys::{
    Layer, Layers, Live, NamedSet, Problem, SendRefusal, Variable, Warning, expand, resolve,
    validate,
};

fn set(pairs: &[(u8, &str)]) -> BTreeMap<u8, String> {
    pairs.iter().map(|&(n, t)| (n, t.to_string())).collect()
}

fn full() -> Live<'static> {
    Live {
        my_call: Some("KD0RHV"),
        their_call: Some("W1AW"),
        my_ref: Some("K-1234"),
        their_ref: Some("W7W/KG-001"),
        my_rst: Some("599"),
        their_rst: Some("579"),
    }
}

mod validation {
    use super::*;

    #[test]
    fn a_plain_message_and_every_known_variable_are_valid() {
        let msg = "CQ POTA DE {MYCALL} {THEIRCALL} {MYREF} {THEIRREF} {MYRST} {THEIRRST} K";
        let report = validate(msg);
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn an_unknown_variable_is_rejected_by_name() {
        let report = validate("DE {MYCAL}");
        assert_eq!(
            report.problems,
            vec![Problem::UnknownVariable("MYCAL".into())]
        );
    }

    #[test]
    fn call_is_removed_and_the_error_says_what_to_use() {
        let report = validate("DE {CALL}");
        assert_eq!(report.problems, vec![Problem::CallRemoved]);
        assert_eq!(report.problems[0].to_string(), "use {MYCALL}");
    }

    #[test]
    fn unbalanced_braces_are_rejected() {
        for msg in [
            "DE {MYCALL",
            "DE MYCALL}",
            "{{MYCALL}}",
            "DE {MY{CALL}}",
            "{",
        ] {
            assert!(
                validate(msg).problems.contains(&Problem::UnbalancedBrace),
                "{msg:?}"
            );
        }
    }

    #[test]
    fn characters_the_keyer_cannot_send_are_rejected() {
        let report = validate("CQ #1 [K]");
        assert_eq!(
            report.problems,
            vec![
                Problem::UnsendableChar('#'),
                Problem::UnsendableChar('['),
                Problem::UnsendableChar(']'),
            ]
        );
    }

    #[test]
    fn lowercase_is_fine_because_messages_are_sent_uppercase() {
        assert!(validate("cq de {mycall} k").problems.is_empty());
    }

    #[test]
    fn punctuation_ham_messages_use_is_sendable() {
        assert!(
            validate("CQ? 5NN = 73, TU. DE W1AW/P (K) +-:'\"@")
                .problems
                .is_empty()
        );
    }

    #[test]
    fn past_120_characters_after_worst_case_expansion_warns_but_is_valid() {
        let long = "A".repeat(121);
        let report = validate(&long);
        assert!(report.problems.is_empty());
        assert_eq!(report.warnings, vec![Warning::TooLong { len: 121 }]);
        assert!(validate(&"A".repeat(120)).warnings.is_empty());
    }

    #[test]
    fn the_length_counts_variables_at_their_longest() {
        // Ten short variables are well under 120 as written but over it when filled in.
        let msg = "{THEIRREF} ".repeat(10);
        assert!(msg.len() < 120);
        let report = validate(&msg);
        assert!(
            matches!(report.warnings[..], [Warning::TooLong { .. }]),
            "{report:?}"
        );
    }
}

mod expansion {
    use super::*;

    #[test]
    fn every_variable_is_filled_from_live_state() {
        let out = expand(
            "{THEIRCALL} DE {MYCALL} {MYREF} {THEIRREF} UR {THEIRRST} {MYRST}",
            &full(),
        );
        assert_eq!(
            out.as_deref(),
            Ok("W1AW DE KD0RHV K-1234 W7W/KG-001 UR 579 599")
        );
    }

    #[test]
    fn values_are_sent_uppercase() {
        let live = Live {
            their_call: Some("w1aw"),
            ..full()
        };
        assert_eq!(expand("{THEIRCALL}", &live).as_deref(), Ok("W1AW"));
    }

    #[test]
    fn a_missing_value_refuses_the_whole_send_naming_it() {
        let live = Live {
            their_call: None,
            ..full()
        };
        let refusal = expand("CQ DE {MYCALL} {THEIRCALL}", &live).unwrap_err();
        assert_eq!(refusal, SendRefusal::Missing(vec![Variable::TheirCall]));
        assert!(refusal.to_string().contains("THEIRCALL"), "{refusal}");
    }

    #[test]
    fn every_variable_can_be_the_missing_one() {
        for (variable, live) in [
            (
                Variable::MyCall,
                Live {
                    my_call: None,
                    ..full()
                },
            ),
            (
                Variable::TheirCall,
                Live {
                    their_call: None,
                    ..full()
                },
            ),
            (
                Variable::MyRef,
                Live {
                    my_ref: None,
                    ..full()
                },
            ),
            (
                Variable::TheirRef,
                Live {
                    their_ref: None,
                    ..full()
                },
            ),
            (
                Variable::MyRst,
                Live {
                    my_rst: None,
                    ..full()
                },
            ),
            (
                Variable::TheirRst,
                Live {
                    their_rst: None,
                    ..full()
                },
            ),
        ] {
            let msg = format!("{{{}}}", variable.name());
            assert_eq!(
                expand(&msg, &live),
                Err(SendRefusal::Missing(vec![variable]))
            );
        }
    }

    #[test]
    fn a_blank_value_counts_as_missing() {
        let live = Live {
            my_ref: Some("  "),
            ..full()
        };
        assert_eq!(
            expand("{MYREF}", &live),
            Err(SendRefusal::Missing(vec![Variable::MyRef]))
        );
    }

    #[test]
    fn several_missing_values_are_all_named() {
        let refusal = expand("{MYREF} {THEIRREF}", &Live::default()).unwrap_err();
        assert_eq!(
            refusal,
            SendRefusal::Missing(vec![Variable::MyRef, Variable::TheirRef])
        );
    }

    #[test]
    fn an_invalid_message_is_refused_even_with_every_value_present() {
        for (msg, problem) in [
            ("DE {CALL}", Problem::CallRemoved),
            ("DE {BOGUS}", Problem::UnknownVariable("BOGUS".into())),
            ("DE {MYCALL", Problem::UnbalancedBrace),
            ("DE #", Problem::UnsendableChar('#')),
        ] {
            let refusal = expand(msg, &full()).unwrap_err();
            assert_eq!(refusal, SendRefusal::Invalid(vec![problem]), "{msg:?}");
        }
    }

    #[test]
    fn a_value_the_keyer_cannot_send_is_refused_not_sent() {
        let live = Live {
            their_call: Some("W1#AW"),
            ..full()
        };
        assert_eq!(
            expand("{THEIRCALL}", &live),
            Err(SendRefusal::Invalid(vec![Problem::UnsendableChar('#')]))
        );
    }

    #[test]
    fn no_literal_braces_ever_reach_the_keyer() {
        let out = expand("{MYCALL} {MYCALL}", &full()).unwrap();
        assert!(!out.contains('{') && !out.contains('}'));
    }
}

mod layering {
    use super::*;

    fn named(name: &str, pairs: &[(u8, &str)]) -> NamedSet {
        NamedSet {
            name: name.into(),
            keys: Some(set(pairs)),
        }
    }

    #[test]
    fn built_in_defaults_fill_keys_nothing_else_sets() {
        let resolved = resolve(&Layers::default());
        assert!(!resolved.keys.is_empty());
        assert!(resolved.keys.values().all(|k| k.layer == Layer::BuiltIn));
        assert!(resolved.keys.values().all(|k| k.is_valid()));
        assert!(resolved.missing_sets.is_empty());
    }

    #[test]
    fn each_key_comes_from_the_highest_layer_that_sets_it() {
        let layers = Layers {
            plan_override: set(&[(1, "OVERRIDE")]),
            plan_set: Some(named("pota", &[(1, "PLAN ONE"), (2, "PLAN TWO")])),
            user_set: Some(named(
                "pota",
                &[(1, "USER ONE"), (2, "USER TWO"), (3, "USER THREE")],
            )),
        };
        let keys = resolve(&layers).keys;
        assert_eq!(
            (keys[&1].text.as_str(), keys[&1].layer),
            ("OVERRIDE", Layer::PlanOverride)
        );
        assert_eq!(
            (keys[&2].text.as_str(), keys[&2].layer),
            ("PLAN TWO", Layer::PlanSet)
        );
        assert_eq!(
            (keys[&3].text.as_str(), keys[&3].layer),
            ("USER THREE", Layer::UserSet)
        );
        assert_eq!(keys[&4].layer, Layer::BuiltIn);
    }

    #[test]
    fn a_missing_set_degrades_to_the_lower_layers_and_is_flagged() {
        let layers = Layers {
            plan_override: BTreeMap::new(),
            plan_set: Some(NamedSet {
                name: "nosuch".into(),
                keys: None,
            }),
            user_set: Some(named("general", &[(1, "USER ONE")])),
        };
        let resolved = resolve(&layers);
        assert_eq!(resolved.keys[&1].layer, Layer::UserSet);
        assert_eq!(resolved.keys[&2].layer, Layer::BuiltIn);
        assert_eq!(resolved.missing_sets, vec!["nosuch".to_string()]);
    }

    #[test]
    fn a_stored_invalid_message_loads_marked_invalid() {
        let layers = Layers {
            user_set: Some(named("general", &[(1, "DE {CALL}"), (2, "73")])),
            ..Layers::default()
        };
        let keys = resolve(&layers).keys;
        assert_eq!(keys[&1].text, "DE {CALL}");
        assert!(!keys[&1].is_valid());
        assert_eq!(keys[&1].problems, vec![Problem::CallRemoved]);
        assert!(keys[&2].is_valid());
    }
}
