use std::path::{Path, PathBuf};

use rig_tui::{Config, config_path};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rig-tui-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(name: &str, text: &str) -> PathBuf {
    let path = scratch(name).join("config.toml");
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn defaults_when_the_default_file_is_missing() {
    let missing = scratch("nodefault").join("config.toml");

    let config = Config::load(None, Some(&missing)).unwrap();

    assert_eq!(config.callsign, None);
    assert_eq!(config.rig_addr, "127.0.0.1:4532");
    assert!(config.soft_keys.is_empty());
}

#[test]
fn reads_the_shared_schema() {
    let path = write(
        "shared",
        r#"
        callsign = "kd0rhv"

        [rig]
        addr = "127.0.0.1:4533"

        [macros]
        F1 = "cq de {call}"
        F6 = "73"
        "#,
    );

    let config = Config::load(Some(&path), None).unwrap();

    assert_eq!(config.callsign.as_deref(), Some("KD0RHV"));
    assert_eq!(config.rig_addr, "127.0.0.1:4533");
    assert_eq!(config.soft_keys[&1], "CQ DE {CALL}");
    assert_eq!(config.soft_keys[&6], "73");
}

#[test]
fn the_old_kx3_table_is_read_as_the_rig_table() {
    let path = write("alias", "[kx3]\naddr = \"192.168.1.20:4532\"\n");

    let config = Config::load(Some(&path), None).unwrap();

    assert_eq!(config.rig_addr, "192.168.1.20:4532");
}

#[test]
fn unknown_keys_are_warnings_not_errors() {
    let path = write("unknown", "watch = [\"ISS\"]\n[rig]\naddr = \"h:1\"\n");

    let config = Config::load(Some(&path), None).unwrap();

    assert_eq!(config.rig_addr, "h:1");
    assert_eq!(config.warnings.len(), 1);
    assert_eq!(config.warnings[0].name, "watch");
}

#[test]
fn a_wrong_value_is_an_error_naming_the_key() {
    let path = write("badvalue", "callsign = \"KD0 RHV\"\n");

    let err = Config::load(Some(&path), None).unwrap_err().to_string();

    assert!(err.contains("callsign"), "{err}");
}

#[test]
fn a_broken_default_file_is_still_an_error() {
    let path = write("broken", "callsign = ");

    assert!(Config::load(None, Some(Path::new(&path))).is_err());
}

#[test]
fn a_missing_explicit_path_is_an_error() {
    let path = scratch("missing").join("nope.toml");

    assert!(Config::load(Some(&path), None).is_err());
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
