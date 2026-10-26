use kx3_cw::Config;

#[test]
fn defaults_when_the_file_is_empty() {
    let config = Config::parse("").unwrap();

    assert_eq!(config.callsign, None);
    assert_eq!(config.kx3.addr, "127.0.0.1:4532");
}

#[test]
fn reads_callsign_and_rigctld_address() {
    let config = Config::parse(
        r#"
        callsign = "kd0rhv"

        [kx3]
        addr = "192.168.1.20:4532"
        "#,
    )
    .unwrap();

    // Callsigns are stored uppercase, however they were typed.
    assert_eq!(config.callsign.as_deref(), Some("KD0RHV"));
    assert_eq!(config.kx3.addr, "192.168.1.20:4532");
}

#[test]
fn accepts_portable_callsigns() {
    let config = Config::parse(r#"callsign = "KD0RHV/P""#).unwrap();

    assert_eq!(config.callsign.as_deref(), Some("KD0RHV/P"));
}

#[test]
fn rejects_unusable_callsigns() {
    for bad in ["", "AB", "KD0 RHV", "KD0RHV!", "WAYTOOLONGCALLSIGN"] {
        let text = format!("callsign = \"{bad}\"");
        assert!(Config::parse(&text).is_err(), "{bad:?} should be rejected");
    }
}

#[test]
fn rejects_unknown_keys_so_typos_are_not_silently_ignored() {
    assert!(Config::parse("callsing = \"KD0RHV\"").is_err());
    assert!(Config::parse("[kx3]\nadress = \"x\"").is_err());
}

mod loading {
    use std::path::{Path, PathBuf};

    use kx3_cw::{Config, config_path};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kx3-cw-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn default_path_prefers_xdg_config_home_then_home() {
        assert_eq!(
            config_path(Some("/x"), Some("/h")),
            Some(PathBuf::from("/x/hamtools/config.toml"))
        );
        assert_eq!(
            config_path(None, Some("/h")),
            Some(PathBuf::from("/h/.config/hamtools/config.toml"))
        );
        // An empty XDG_CONFIG_HOME counts as unset, per the XDG spec.
        assert_eq!(
            config_path(Some(""), Some("/h")),
            Some(PathBuf::from("/h/.config/hamtools/config.toml"))
        );
        assert_eq!(config_path(None, None), None);
    }

    #[test]
    fn an_explicit_path_is_read() {
        let path = scratch("explicit").join("c.toml");
        std::fs::write(&path, "callsign = \"KD0RHV\"").unwrap();

        let config = Config::load(Some(&path), None).unwrap();

        assert_eq!(config.callsign.as_deref(), Some("KD0RHV"));
    }

    #[test]
    fn a_missing_explicit_path_is_an_error() {
        let path = scratch("missing").join("nope.toml");

        assert!(Config::load(Some(&path), None).is_err());
    }

    #[test]
    fn a_missing_default_file_means_defaults() {
        let missing_default = scratch("nodefault").join("config.toml");

        let config = Config::load(None, Some(&missing_default)).unwrap();

        assert_eq!(config, Config::default());
    }

    #[test]
    fn a_broken_default_file_is_still_an_error() {
        let path = scratch("broken").join("config.toml");
        std::fs::write(&path, "callsign = ").unwrap();

        assert!(Config::load(None, Some(Path::new(&path))).is_err());
    }
}

mod macros {
    use kx3_cw::Config;

    #[test]
    fn none_by_default() {
        assert!(Config::parse("").unwrap().macros.is_empty());
    }

    #[test]
    fn f1_to_f6_are_read_and_uppercased() {
        let config = Config::parse(
            r#"
            [macros]
            F1 = "cq cq cq de {call} {call} k"
            F6 = "73 TU"
            "#,
        )
        .unwrap();

        assert_eq!(config.macros.len(), 2);
        assert_eq!(config.macros[&1], "CQ CQ CQ DE {CALL} {CALL} K");
        assert_eq!(config.macros[&6], "73 TU");
    }

    #[test]
    fn rejects_keys_other_than_f1_to_f6() {
        for key in ["F0", "F7", "G1", "f1", "F10"] {
            let text = format!("[macros]\n{key} = \"CQ\"");
            assert!(Config::parse(&text).is_err(), "{key} should be rejected");
        }
    }

    #[test]
    fn rejects_blank_macro_text() {
        assert!(Config::parse("[macros]\nF1 = \"\"").is_err());
        assert!(Config::parse("[macros]\nF1 = \"   \"").is_err());
    }
}
