mod common;

use common::TempDir;
use hamtools_config::{ConfigError, ConfigFile};

#[test]
fn reference_intervals_are_read_and_absent_means_none() {
    let dir = TempDir::new();
    let path = dir.write(
        "config.toml",
        "[reference]\nspots_secs = 120\nactivations_secs = 600\ngap_secs = 2\n",
    );
    let cfg = ConfigFile::load(&path).unwrap();
    let r = &cfg.settings().reference;
    assert_eq!(r.spots_secs, Some(120));
    assert_eq!(r.activations_secs, Some(600));
    assert_eq!(r.gap_secs, Some(2));
    assert!(cfg.warnings().is_empty(), "{:?}", cfg.warnings());

    let path = dir.write("none.toml", "callsign = \"K9TST\"\n");
    let cfg = ConfigFile::load(&path).unwrap();
    assert_eq!(cfg.settings().reference.spots_secs, None);
}

#[test]
fn a_negative_or_non_number_interval_names_the_key() {
    let dir = TempDir::new();
    for (text, key) in [
        ("[reference]\nspots_secs = -5\n", "reference.spots_secs"),
        ("[reference]\ngap_secs = \"fast\"\n", "reference.gap_secs"),
    ] {
        let path = dir.write("bad.toml", text);
        match ConfigFile::load(&path) {
            Err(ConfigError::Invalid { key: k, .. }) => assert_eq!(k, key),
            other => panic!("{other:?}"),
        }
    }
}
