mod common;

use common::TempDir;
use hamtools_config::{ConfigError, ConfigFile, RigKind};

const FULL: &str = r#"# my station
callsign = "w1aw"

[station]
locator = "en71dq"

[logbook]
path = "/var/log/hams.adi"

[rig]
default = "ft847"
[rig.kx3]
addr = "127.0.0.1:4532"
kind = "kx3"
[rig.ft847]
addr = "127.0.0.1:4533"
kind = "ft847"
sat_mode = true

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
    let names: Vec<&str> = s.rigs.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["kx3", "ft847"]);
    assert_eq!(s.rig("ft847").unwrap().addr, "127.0.0.1:4533");
    assert_eq!(s.rig("ft847").unwrap().kind, RigKind::Ft847);
    assert_eq!(s.rig("ft847").unwrap().sat_mode, Some(true));
    assert_eq!(s.rig("kx3").unwrap().sat_mode, None);
    assert_eq!(s.rig_default.as_deref(), Some("ft847"));
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
fn the_old_rig_addr_and_kx3_table_are_rejected_with_the_replacement_named() {
    for (text, key) in [
        ("[rig]\naddr = \"127.0.0.1:4532\"\n", "rig.addr"),
        ("[kx3]\naddr = \"127.0.0.1:4532\"\n", "kx3"),
    ] {
        let dir = TempDir::new();
        let path = dir.write("config.toml", text);
        match ConfigFile::load(&path) {
            Err(ConfigError::Invalid { key: k, message }) => {
                assert_eq!(k, key);
                assert!(message.contains("[rig.NAME]"), "{message}");
            }
            other => panic!("{text}: {other:?}"),
        }
    }
}

#[test]
fn the_default_rig_falls_back_to_the_first_profile_and_must_exist() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", "[rig.a]\naddr = \"h:1\"\nkind = \"kx3\"\n");
    let cfg = ConfigFile::load(&path).unwrap();
    assert_eq!(cfg.settings().rig_default, None);
    assert_eq!(cfg.settings().rigs[0].name, "a");

    let path = dir.write(
        "bad.toml",
        "[rig]\ndefault = \"zz\"\n[rig.a]\naddr = \"h:1\"\nkind = \"kx3\"\n",
    );
    match ConfigFile::load(&path) {
        Err(ConfigError::Invalid { key, .. }) => assert_eq!(key, "rig.default"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn unknown_profile_keys_are_warnings() {
    let dir = TempDir::new();
    let path = dir.write(
        "config.toml",
        "[rig.a]\naddr = \"h:1\"\nkind = \"kx3\"\nbaud = 1\n",
    );
    let cfg = ConfigFile::load(&path).unwrap();
    assert_eq!(cfg.warnings()[0].name, "rig.a.baud");
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
        (
            "[rig.a]\naddr = \"nohostport\"\nkind = \"kx3\"\n",
            "rig.a.addr",
        ),
        ("[rig.a]\naddr = \"h:1\"\nkind = \"ic7300\"\n", "rig.a.kind"),
        ("[rig.a]\naddr = \"h:1\"\n", "rig.a"),
        (
            "[rig.a]\naddr = \"h:1\"\nkind = \"kx3\"\ncw = 1\n",
            "rig.a.cw",
        ),
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
