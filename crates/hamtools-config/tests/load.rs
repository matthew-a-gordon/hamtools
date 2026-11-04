mod common;

use common::TempDir;
use hamtools_config::{ConfigError, ConfigFile};

const FULL: &str = r#"# my station
callsign = "w1aw"

[station]
locator = "en71dq"

[logbook]
path = "/var/log/hams.adi"

[rig]
addr = "127.0.0.1:4533"

[macros]
F1 = "cq de {call}"
F6 = "73"
"#;

#[test]
fn loads_every_known_key() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", FULL);
    let cfg = ConfigFile::load(&path).unwrap();
    let s = cfg.settings();
    assert_eq!(s.callsign.as_ref().unwrap().as_str(), "W1AW");
    assert_eq!(s.locator.as_ref().unwrap().as_str(), "EN71DQ");
    assert_eq!(
        s.log_path.as_deref(),
        Some(std::path::Path::new("/var/log/hams.adi"))
    );
    assert_eq!(s.rig_addr.as_deref(), Some("127.0.0.1:4533"));
    assert_eq!(s.macros.get(&1).map(String::as_str), Some("CQ DE {CALL}"));
    assert_eq!(s.macros.get(&6).map(String::as_str), Some("73"));
    assert!(cfg.warnings().is_empty());
}

#[test]
fn missing_file_loads_defaults() {
    let dir = TempDir::new();
    let cfg = ConfigFile::load(&dir.path().join("nope/config.toml")).unwrap();
    assert_eq!(cfg.settings(), &Default::default());
}

#[test]
fn kx3_table_is_an_alias_for_rig() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", "[kx3]\naddr = \"127.0.0.1:4532\"\n");
    let cfg = ConfigFile::load(&path).unwrap();
    assert_eq!(cfg.settings().rig_addr.as_deref(), Some("127.0.0.1:4532"));
    assert!(cfg.warnings().is_empty());
}

#[test]
fn rig_wins_over_kx3() {
    let dir = TempDir::new();
    let path = dir.write(
        "config.toml",
        "[kx3]\naddr = \"a:1\"\n[rig]\naddr = \"b:2\"\n",
    );
    let cfg = ConfigFile::load(&path).unwrap();
    assert_eq!(cfg.settings().rig_addr.as_deref(), Some("b:2"));
}

#[test]
fn unknown_keys_are_warnings_with_line_and_name() {
    let dir = TempDir::new();
    let path = dir.write(
        "config.toml",
        "callsign = \"W1AW\"\nwatch = [\"ISS\"]\n\n[station]\nlocator = \"EN71DQ\"\nfoo = 1\n\n[passes]\nmin = 5\n",
    );
    let cfg = ConfigFile::load(&path).unwrap();
    let w: Vec<(usize, &str)> = cfg
        .warnings()
        .iter()
        .map(|w| (w.line, w.name.as_str()))
        .collect();
    assert_eq!(w, [(2, "watch"), (6, "station.foo"), (8, "passes")]);
}

#[test]
fn wrong_values_are_errors_naming_the_key() {
    let cases = [
        ("callsign = \"no\"\n", "callsign"),
        ("callsign = 5\n", "callsign"),
        ("[station]\nlocator = \"EN71\"\n", "station.locator"),
        ("[logbook]\npath = 3\n", "logbook.path"),
        ("[rig]\naddr = \"nohostport\"\n", "rig.addr"),
        ("[kx3]\naddr = \"\"\n", "kx3.addr"),
        ("[macros]\nF7 = \"x\"\n", "macros.F7"),
        ("[macros]\nF1 = \"\"\n", "macros.F1"),
    ];
    for (text, key) in cases {
        let dir = TempDir::new();
        let path = dir.write("config.toml", text);
        match ConfigFile::load(&path) {
            Err(ConfigError::Invalid { key: k, .. }) => assert_eq!(k, key, "{text}"),
            other => panic!("{text}: {other:?}"),
        }
    }
}

#[test]
fn syntax_errors_carry_a_line() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", "callsign = \"W1AW\"\nthis is not toml\n");
    match ConfigFile::load(&path) {
        Err(ConfigError::Parse { line, .. }) => assert_eq!(line, 2),
        other => panic!("{other:?}"),
    }
}
