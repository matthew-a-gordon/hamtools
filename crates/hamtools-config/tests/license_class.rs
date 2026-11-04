mod common;

use common::TempDir;
use hamtools_config::{ConfigError, ConfigFile, LicenseClass};

#[test]
fn license_class_is_read_case_insensitively_and_has_no_default() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", "[station]\nlicense_class = \"General\"\n");
    let cfg = ConfigFile::load(&path).unwrap();
    assert_eq!(cfg.settings().license_class, Some(LicenseClass::General));
    assert!(cfg.warnings().is_empty());

    let path = dir.write("none.toml", "[station]\nlocator = \"EN71DQ\"\n");
    let cfg = ConfigFile::load(&path).unwrap();
    assert_eq!(cfg.settings().license_class, None);
}

#[test]
fn an_unknown_license_class_is_an_error_naming_the_key() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", "[station]\nlicense_class = \"wizard\"\n");
    match ConfigFile::load(&path) {
        Err(ConfigError::Invalid { key, .. }) => assert_eq!(key, "station.license_class"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn license_class_is_written_without_disturbing_the_rest() {
    let dir = TempDir::new();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[station]\nlocator = \"EN71DQ\" # coarse\n").unwrap();
    let mut cfg = ConfigFile::load(&path).unwrap();
    let mut settings = cfg.settings().clone();
    settings.license_class = Some(LicenseClass::Extra);
    cfg.save(&settings).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[station]\nlocator = \"EN71DQ\" # coarse\nlicense_class = \"extra\"\n"
    );
}
