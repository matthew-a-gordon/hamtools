mod common;

use common::TempDir;
use hamtools_config::{Callsign, ConfigError, ConfigFile};

const WITH_SAT: &str = r#"callsign = "W1AW"

[satellite]
watch = [25544, 44909]
horizon_hours = 48
min_elevation_deg = 5
short_pass_secs = 120

[satellite.names]
"ISS (ZARYA)" = "ISS"
"#;

#[test]
fn the_satellite_table_is_read() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", WITH_SAT);
    let file = ConfigFile::load(&path).unwrap();
    let sat = &file.settings().satellite;
    assert_eq!(sat.watch, [25544, 44909]);
    assert_eq!(sat.horizon_hours, 48);
    assert!((sat.min_elevation_deg - 5.0).abs() < f64::EPSILON);
    assert_eq!(sat.short_pass_secs, 120);
    assert_eq!(
        sat.names.get("ISS (ZARYA)").map(String::as_str),
        Some("ISS")
    );
    assert!(file.warnings().is_empty(), "{:?}", file.warnings());
}

#[test]
fn an_absent_satellite_table_gives_the_old_defaults() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", "callsign = \"W1AW\"\n");
    let sat = ConfigFile::load(&path)
        .unwrap()
        .settings()
        .satellite
        .clone();
    assert!(sat.watch.is_empty());
    assert_eq!(sat.horizon_hours, 24);
    assert!((sat.min_elevation_deg - 10.0).abs() < f64::EPSILON);
    assert_eq!(sat.short_pass_secs, 180);
    assert!(sat.names.is_empty());
}

#[test]
fn out_of_range_values_name_their_key() {
    for (text, key) in [
        (
            "[satellite]\nhorizon_hours = 0\n",
            "satellite.horizon_hours",
        ),
        (
            "[satellite]\nmin_elevation_deg = 91\n",
            "satellite.min_elevation_deg",
        ),
        (
            "[satellite]\nshort_pass_secs = 0\n",
            "satellite.short_pass_secs",
        ),
        ("[satellite]\nwatch = [\"ISS\"]\n", "satellite.watch"),
    ] {
        let dir = TempDir::new();
        let path = dir.write("config.toml", text);
        match ConfigFile::load(&path) {
            Err(ConfigError::Invalid { key: k, .. }) => assert_eq!(k, key),
            other => panic!("{text}: {other:?}"),
        }
    }
}

#[test]
fn an_unknown_satellite_key_is_a_warning_and_a_save_keeps_the_table() {
    let dir = TempDir::new();
    let text = WITH_SAT.replace("short_pass_secs = 120", "short_pass_secs = 120\nbogus = 1");
    let path = dir.write("config.toml", &text);
    let mut file = ConfigFile::load(&path).unwrap();
    assert_eq!(file.warnings()[0].name, "satellite.bogus");
    let mut new = file.settings().clone();
    new.callsign = Some(Callsign::new("N0CALL").unwrap());
    file.save(&new).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("watch = [25544, 44909]"), "{saved}");
    assert!(saved.contains("bogus = 1"), "{saved}");
}
