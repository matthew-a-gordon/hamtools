mod common;

use common::TempDir;
use hamtools_config::{ConfigError, ConfigFile};
use std::path::PathBuf;

#[test]
fn export_dir_is_read_and_absent_means_none() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", "[export]\ndir = \"/tmp/out\"\n");
    let cfg = ConfigFile::load(&path).unwrap();
    assert_eq!(cfg.settings().export_dir, Some(PathBuf::from("/tmp/out")));
    assert!(cfg.warnings().is_empty());

    let path = dir.write("none.toml", "callsign = \"K9TST\"\n");
    assert_eq!(ConfigFile::load(&path).unwrap().settings().export_dir, None);
}

#[test]
fn an_empty_export_dir_is_an_error_naming_the_key() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", "[export]\ndir = \"\"\n");
    match ConfigFile::load(&path) {
        Err(ConfigError::Invalid { key, .. }) => assert_eq!(key, "export.dir"),
        other => panic!("{other:?}"),
    }
}
