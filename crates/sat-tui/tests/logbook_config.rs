use std::path::{Path, PathBuf};

use logbook::Grid;
use sat_tui::config::{Config, Shared, load_shared, log_path, shared_config_path};

#[test]
fn the_logbook_section_defaults_to_nothing_set() {
    let config = Config::parse("").unwrap();

    assert_eq!(config.logbook.grid, None);
    assert!(config.logbook.satellites.is_empty());
}

#[test]
fn reads_the_logbook_keys_and_per_satellite_names() {
    let config = Config::parse(
        r#"
        [logbook]
        grid = "en71dq"

        [logbook.satellites]
        "ISS (ZARYA)" = "ISS"
        "RS-44 & BREEZE-KM R/B" = "RS-44"
        "#,
    )
    .unwrap();

    assert_eq!(
        config.logbook.grid.as_ref().map(Grid::as_str),
        Some("EN71DQ")
    );
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
    fn callsign_locator_and_log_path_are_read_and_other_sections_ignored() {
        let dir = std::env::temp_dir().join(format!("sat-tui-shared-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("full.toml");
        std::fs::write(
            &path,
            r#"
            callsign = "w1aw"
            [station]
            locator = "en71dq"
            [logbook]
            path = "/data/shared.adi"
            [kx3]
            addr = "127.0.0.1:4532"
            [macros]
            F1 = "CQ CQ DE {call} K"
            "#,
        )
        .unwrap();

        let (shared, note) = load_shared(Some(&path));

        assert_eq!(shared.callsign.unwrap().as_str(), "W1AW");
        assert_eq!(shared.locator.unwrap().as_str(), "EN71DQ");
        assert_eq!(shared.log_path, Some(PathBuf::from("/data/shared.adi")));
        assert_eq!(note, None);
    }

    #[test]
    fn startup_never_fails_on_the_shared_file_it_only_reports_why_logging_is_off() {
        let dir = std::env::temp_dir().join(format!("sat-tui-shared-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.toml");
        std::fs::write(&bad, "callsign = \"no way!\"").unwrap();

        let (shared, note) = load_shared(Some(&bad));
        assert_eq!(shared, Shared::default());
        assert!(note.unwrap().contains("callsign"));

        // Unreadable (a directory) is reported, not fatal.
        let (shared, note) = load_shared(Some(&dir));
        assert_eq!(shared, Shared::default());
        assert!(note.is_some());

        assert_eq!(
            load_shared(Some(&dir.join("missing.toml"))),
            (Shared::default(), None)
        );
        assert_eq!(load_shared(None), (Shared::default(), None));
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
