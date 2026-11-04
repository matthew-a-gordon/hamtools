use std::path::{Path, PathBuf};

use sat_tui::config::{Config, log_path, logging_callsign, shared_callsign, shared_config_path};

#[test]
fn the_logbook_section_defaults_to_nothing_set() {
    let config = Config::parse("").unwrap();

    assert_eq!(config.logbook.path, None);
    assert_eq!(config.logbook.grid, None);
    assert!(config.logbook.satellites.is_empty());
}

#[test]
fn reads_the_logbook_keys_and_per_satellite_names() {
    let config = Config::parse(
        r#"
        [logbook]
        path = "/data/my.adi"
        grid = "en71dq"

        [logbook.satellites]
        "ISS (ZARYA)" = "ISS"
        "RS-44 & BREEZE-KM R/B" = "RS-44"
        "#,
    )
    .unwrap();

    assert_eq!(config.logbook.path, Some(PathBuf::from("/data/my.adi")));
    assert_eq!(config.logbook.grid.as_deref(), Some("EN71DQ"));
    assert_eq!(config.logbook.satellites["ISS (ZARYA)"], "ISS");
    assert_eq!(config.logbook.satellites.len(), 2);
}

#[test]
fn a_bad_grid_override_or_unknown_logbook_key_is_an_error() {
    assert!(Config::parse("[logbook]\ngrid = \"EN71\"").is_err());
    assert!(Config::parse("[logbook]\ngrid = \"ZZ99zz\"").is_err());
    assert!(Config::parse("[logbook]\npth = \"x\"").is_err());
}

mod log_location {
    use super::*;

    #[test]
    fn the_command_line_beats_the_config_which_beats_the_default() {
        let cli = Some(Path::new("/cli.adi"));
        let cfg = Some(Path::new("/cfg.adi"));

        assert_eq!(
            log_path(cli, cfg, Some("/x"), Some("/h")),
            Some(PathBuf::from("/cli.adi"))
        );
        assert_eq!(
            log_path(None, cfg, Some("/x"), Some("/h")),
            Some(PathBuf::from("/cfg.adi"))
        );
        assert_eq!(
            log_path(None, None, Some("/x"), Some("/h")),
            Some(PathBuf::from("/x/hamtools/log.adi"))
        );
    }

    #[test]
    fn without_xdg_data_home_it_falls_back_to_the_local_share_directory() {
        let expected = Some(PathBuf::from("/h/.local/share/hamtools/log.adi"));

        assert_eq!(log_path(None, None, None, Some("/h")), expected);
        assert_eq!(log_path(None, None, Some(""), Some("/h")), expected);
        assert_eq!(log_path(None, None, None, None), None);
    }
}

mod shared_config {
    use super::*;

    #[test]
    fn only_the_callsign_is_read_and_other_sections_are_ignored() {
        let call = shared_callsign(
            r#"
            callsign = "w1aw"
            [kx3]
            addr = "127.0.0.1:4532"
            [macros]
            F1 = "CQ CQ DE {call} K"
            "#,
        )
        .unwrap();

        assert_eq!(call.unwrap().as_str(), "W1AW");
    }

    #[test]
    fn no_callsign_is_none_and_a_bad_one_is_an_error() {
        assert!(shared_callsign("[kx3]\nx = 1").unwrap().is_none());
        assert!(shared_callsign("callsign = \"no way!\"").is_err());
    }

    #[test]
    fn a_toml_error_in_an_unrelated_section_does_not_hide_the_callsign() {
        let call = shared_callsign("callsign = \"w1aw\"\n[kx3]\naddr = = broken\n").unwrap();
        assert_eq!(call.unwrap().as_str(), "W1AW");
        // No callsign above the broken part: nothing to say about it, and no error either.
        assert!(
            shared_callsign("[kx3]\naddr = = broken\n")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn startup_never_fails_on_the_shared_file_it_only_reports_why_logging_is_off() {
        let dir = std::env::temp_dir().join(format!("sat-tui-shared-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, text: &str| {
            let path = dir.join(name);
            std::fs::write(&path, text).unwrap();
            path
        };

        let (call, note) =
            logging_callsign(Some(&write("ok.toml", "callsign = \"w1aw\"\n[kx3]\nx = = 1\n")));
        assert_eq!(call.unwrap().as_str(), "W1AW");
        assert_eq!(note, None);

        let (call, note) = logging_callsign(Some(&write("bad.toml", "callsign = \"no way!\"")));
        assert!(call.is_none());
        assert!(note.unwrap().contains("bad.toml"));

        // Unreadable (a directory) is reported, not fatal.
        let (call, note) = logging_callsign(Some(&dir));
        assert!(call.is_none());
        assert!(note.is_some());

        assert_eq!(logging_callsign(Some(&dir.join("missing.toml"))), (None, None));
        assert_eq!(logging_callsign(None), (None, None));
    }

    #[test]
    fn it_lives_in_config_toml_beside_sat_tui_toml() {
        assert_eq!(
            shared_config_path(Some("/x"), Some("/h")),
            Some(PathBuf::from("/x/hamtools/config.toml"))
        );
        assert_eq!(
            shared_config_path(None, Some("/h")),
            Some(PathBuf::from("/h/.config/hamtools/config.toml"))
        );
    }
}
