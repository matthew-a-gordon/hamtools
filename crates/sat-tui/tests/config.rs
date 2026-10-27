use std::path::PathBuf;

use sat_tui::config::{Config, config_path};
use sat_tui::observer::Spec;

#[test]
fn an_empty_file_gives_the_defaults() {
    let config = Config::parse("").unwrap();

    assert_eq!(config.rig_addr, "127.0.0.1:4533");
    assert_eq!(config.horizon_hours, 24);
    assert_eq!(config.min_elevation_deg, 10.0);
    assert!(config.watch.is_empty());
    assert_eq!(config.observer, Spec::default());
}

#[test]
fn reads_every_setting() {
    let config = Config::parse(
        r#"
        watch = [25544, 27607, 44909]

        [rig]
        addr = "192.168.1.30:4533"

        [passes]
        horizon_hours = 12
        min_elevation_deg = 5.5

        [observer]
        grid = "EN71dq"
        alt_m = 260.0
        "#,
    )
    .unwrap();

    assert_eq!(config.watch, [25544, 27607, 44909]);
    assert_eq!(config.rig_addr, "192.168.1.30:4533");
    assert_eq!(config.horizon_hours, 12);
    assert_eq!(config.min_elevation_deg, 5.5);
    assert_eq!(
        config.observer,
        Spec {
            grid: Some("EN71dq".into()),
            lat_deg: None,
            lon_deg: None,
            alt_m: Some(260.0)
        }
    );
}

#[test]
fn observer_coordinates_are_read() {
    let config = Config::parse("[observer]\nlat_deg = 41.7\nlon_deg = -85.7").unwrap();

    assert_eq!(config.observer.lat_deg, Some(41.7));
    assert_eq!(config.observer.lon_deg, Some(-85.7));
}

#[test]
fn unknown_keys_are_errors_so_typos_are_not_ignored() {
    assert!(Config::parse("wach = [1]").is_err());
    assert!(Config::parse("[passes]\nhorizon = 5").is_err());
    assert!(Config::parse("[rig]\nadress = \"x\"").is_err());
}

#[test]
fn rejects_unusable_pass_settings() {
    assert!(Config::parse("[passes]\nhorizon_hours = 0").is_err());
    assert!(Config::parse("[passes]\nhorizon_hours = 200").is_err());
    assert!(Config::parse("[passes]\nmin_elevation_deg = -1.0").is_err());
    assert!(Config::parse("[passes]\nmin_elevation_deg = 91.0").is_err());
}

mod loading {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("sat-tui-config-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_default_path_is_sat_tui_toml_beside_the_other_tools_config() {
        assert_eq!(
            config_path(Some("/x"), Some("/h")),
            Some(PathBuf::from("/x/hamtools/sat-tui.toml"))
        );
        assert_eq!(
            config_path(None, Some("/h")),
            Some(PathBuf::from("/h/.config/hamtools/sat-tui.toml"))
        );
        assert_eq!(config_path(Some(""), None), None);
    }

    #[test]
    fn an_explicit_path_is_read_and_must_exist() {
        let dir = scratch("explicit");
        let path = dir.join("c.toml");
        std::fs::write(&path, "watch = [1]").unwrap();

        assert_eq!(Config::load(Some(&path), None).unwrap().watch, [1]);
        assert!(Config::load(Some(&dir.join("nope.toml")), None).is_err());
    }

    #[test]
    fn a_missing_default_file_means_defaults_but_a_broken_one_is_an_error() {
        let dir = scratch("default");

        assert_eq!(
            Config::load(None, Some(&dir.join("absent.toml"))).unwrap(),
            Config::default()
        );

        let broken = dir.join("broken.toml");
        std::fs::write(&broken, "watch = ").unwrap();
        assert!(Config::load(None, Some(&broken)).is_err());
    }
}

#[test]
fn short_pass_threshold_defaults_to_three_minutes_and_can_be_set() {
    assert_eq!(Config::parse("").unwrap().short_pass_secs, 180);
    assert_eq!(
        Config::parse("[passes]\nshort_pass_secs = 120")
            .unwrap()
            .short_pass_secs,
        120
    );
}

#[test]
fn rejects_an_unusable_short_pass_threshold() {
    assert!(Config::parse("[passes]\nshort_pass_secs = 0").is_err());
    assert!(Config::parse("[passes]\nshort_pass_secs = 7200").is_err());
}
