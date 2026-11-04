mod common;

use common::TempDir;
use hamtools_config::{Callsign, ConfigFile, FieldConflict, Grid, SaveError, Saved, Settings};
use std::os::unix::fs::PermissionsExt;

fn call(s: &str) -> Option<Callsign> {
    Some(Callsign::new(s).unwrap())
}

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

const HAND_WRITTEN: &str = "\
# my hamtools config
callsign = \"W1AW\"   # club call
watch = [\"ISS\"]

[station]
# where I live
locator = \"EN71DQ\"  # coarse
extra = 1

[passes]
min = 5

[macros]
F2 = \"{CALL}\"
";

#[test]
fn save_patches_only_the_changed_field() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", HAND_WRITTEN);
    let mut cfg = ConfigFile::load(&path).unwrap();
    let mut s = cfg.settings().clone();
    s.callsign = call("K1ABC");
    assert_eq!(cfg.save(&s), Ok(Saved::Written));
    assert_eq!(
        read(&path),
        HAND_WRITTEN.replace("\"W1AW\"   # club call", "\"K1ABC\"   # club call")
    );
    assert_eq!(cfg.settings().callsign, call("K1ABC"));
    assert_eq!(cfg.warnings().len(), 3);
}

#[test]
fn new_keys_go_at_the_end_of_their_table() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", HAND_WRITTEN);
    let mut cfg = ConfigFile::load(&path).unwrap();
    let mut s = cfg.settings().clone();
    s.macros.insert(1, "CQ DE {CALL} K".into());
    s.log_path = Some("/tmp/log.adi".into());
    cfg.save(&s).unwrap();
    assert_eq!(
        read(&path),
        "\
# my hamtools config
callsign = \"W1AW\"   # club call
watch = [\"ISS\"]

[station]
# where I live
locator = \"EN71DQ\"  # coarse
extra = 1

[passes]
min = 5

[macros]
F2 = \"{CALL}\"
F1 = \"CQ DE {CALL} K\"

[logbook]
path = \"/tmp/log.adi\"
"
    );
}

#[test]
fn new_top_level_key_stays_in_the_root_table() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", "[rig.a]\naddr = \"a:1\"\nkind = \"kx3\"\n");
    let mut cfg = ConfigFile::load(&path).unwrap();
    let mut s = cfg.settings().clone();
    s.callsign = call("W1AW");
    cfg.save(&s).unwrap();
    assert_eq!(
        read(&path),
        "callsign = \"W1AW\"\n[rig.a]\naddr = \"a:1\"\nkind = \"kx3\"\n"
    );
    assert_eq!(
        ConfigFile::load(&path).unwrap().settings().callsign,
        call("W1AW")
    );
}

#[test]
fn clearing_a_field_removes_its_key_only() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", HAND_WRITTEN);
    let mut cfg = ConfigFile::load(&path).unwrap();
    let mut s = cfg.settings().clone();
    s.macros.remove(&2);
    cfg.save(&s).unwrap();
    assert_eq!(read(&path), HAND_WRITTEN.replace("F2 = \"{CALL}\"\n", ""));
}

#[test]
fn a_profile_address_is_patched_in_place_and_nothing_else_moves() {
    let dir = TempDir::new();
    let text = "[rig.kx3]\naddr = \"127.0.0.1:4532\" # kx3\nkind = \"kx3\"\n[rig.ft847]\naddr = \"127.0.0.1:4533\"\nkind = \"ft847\"\n";
    let path = dir.write("config.toml", text);
    let mut cfg = ConfigFile::load(&path).unwrap();
    let mut s = cfg.settings().clone();
    s.rigs[0].addr = "127.0.0.1:5000".into();
    cfg.save(&s).unwrap();
    assert_eq!(
        read(&path),
        text.replace("127.0.0.1:4532\" # kx3", "127.0.0.1:5000\" # kx3")
    );
}

#[test]
fn the_default_rig_is_written_under_a_rig_header() {
    let dir = TempDir::new();
    let text = "[rig.kx3]\naddr = \"a:1\"\nkind = \"kx3\"\n[rig.ft847]\naddr = \"b:2\"\nkind = \"ft847\"\n";
    let path = dir.write("config.toml", text);
    let mut cfg = ConfigFile::load(&path).unwrap();
    let mut s = cfg.settings().clone();
    s.rig_default = Some("ft847".into());
    cfg.save(&s).unwrap();
    let saved = ConfigFile::load(&path).unwrap();
    assert_eq!(saved.settings().rig_default.as_deref(), Some("ft847"));
    assert_eq!(saved.settings().rigs.len(), 2);
}

#[test]
fn first_save_creates_directory_and_file() {
    let dir = TempDir::new();
    let path = dir.path().join("fresh/hamtools/config.toml");
    let mut cfg = ConfigFile::load(&path).unwrap();
    let s = Settings {
        callsign: call("W1AW"),
        locator: Some(Grid::new("en71dq").unwrap()),
        ..Settings::default()
    };
    cfg.save(&s).unwrap();
    assert_eq!(
        read(&path),
        "callsign = \"W1AW\"\n\n[station]\nlocator = \"EN71DQ\"\n"
    );
    assert!(!cfg.changed_on_disk());
}

#[test]
fn atomic_write_leaves_no_temp_file() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", HAND_WRITTEN);
    let mut cfg = ConfigFile::load(&path).unwrap();
    let mut s = cfg.settings().clone();
    s.callsign = call("K1ABC");
    cfg.save(&s).unwrap();
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["config.toml"]);
}

#[test]
fn a_failed_write_leaves_the_file_and_no_temp_file() {
    let dir = TempDir::new();
    let sub = dir.path().join("d");
    std::fs::create_dir(&sub).unwrap();
    let path = sub.join("config.toml");
    std::fs::write(&path, HAND_WRITTEN).unwrap();
    let mut cfg = ConfigFile::load(&path).unwrap();
    // A read-only directory refuses the temp file.
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o555)).unwrap();
    let mut s = cfg.settings().clone();
    s.callsign = call("K1ABC");
    let result = cfg.save(&s);
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).unwrap();
    if std::fs::write(sub.join("probe"), "").is_ok() {
        // Running as root: permissions are not enforced, nothing to assert about failure.
        return;
    }
    assert!(
        matches!(result, Err(SaveError::ReadOnly { .. })),
        "{result:?}"
    );
    assert_eq!(read(&path), HAND_WRITTEN);
    assert_eq!(std::fs::read_dir(&sub).unwrap().count(), 1);
}

#[test]
fn symlink_target_is_never_replaced() {
    let dir = TempDir::new();
    let real = dir.write("managed.toml", HAND_WRITTEN);
    let link = dir.path().join("config.toml");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let mut cfg = ConfigFile::load(&link).unwrap();
    let mut s = cfg.settings().clone();
    s.callsign = call("K1ABC");
    assert!(matches!(cfg.save(&s), Err(SaveError::ReadOnly { .. })));
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(read(&real), HAND_WRITTEN);
}

#[test]
fn read_only_target_is_never_replaced() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", HAND_WRITTEN);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    let mut cfg = ConfigFile::load(&path).unwrap();
    let mut s = cfg.settings().clone();
    s.callsign = call("K1ABC");
    let err = cfg.save(&s).unwrap_err();
    assert!(matches!(err, SaveError::ReadOnly { .. }), "{err:?}");
    assert!(err.to_string().contains("read-only"));
    assert_eq!(read(&path), HAND_WRITTEN);
}

#[test]
fn unparseable_file_on_disk_is_never_saved_over() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", HAND_WRITTEN);
    let mut cfg = ConfigFile::load(&path).unwrap();
    let broken = "callsign = \"W1AW\"\nnot toml at all\n";
    std::fs::write(&path, broken).unwrap();
    let mut s = cfg.settings().clone();
    s.callsign = call("K1ABC");
    assert!(matches!(
        cfg.save(&s),
        Err(SaveError::Unparseable { line: 2, .. })
    ));
    assert_eq!(read(&path), broken);
    assert_eq!(
        cfg.settings().callsign,
        call("W1AW"),
        "last good settings kept"
    );
}

#[test]
fn changed_on_disk_detects_edits_by_content() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", HAND_WRITTEN);
    let cfg = ConfigFile::load(&path).unwrap();
    assert!(!cfg.changed_on_disk());
    // Same size, different content.
    std::fs::write(&path, HAND_WRITTEN.replace("W1AW", "W1AX")).unwrap();
    let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(mtime + std::time::Duration::from_secs(5))
        .unwrap();
    assert!(cfg.changed_on_disk());
    std::fs::remove_file(&path).unwrap();
    assert!(cfg.changed_on_disk());
}

#[test]
fn touching_without_changing_content_is_not_a_change() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", HAND_WRITTEN);
    let cfg = ConfigFile::load(&path).unwrap();
    let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(mtime + std::time::Duration::from_secs(5))
        .unwrap();
    assert!(!cfg.changed_on_disk());
}

#[test]
fn every_soft_key_slot_saves() {
    let dir = TempDir::new();
    let path = dir.path().join("config.toml");
    let mut cfg = ConfigFile::load(&path).unwrap();
    let mut s = Settings::default();
    for n in 1..=6u8 {
        s.macros.insert(n, format!("MSG {n}"));
    }
    cfg.save(&s).unwrap();
    assert_eq!(ConfigFile::load(&path).unwrap().settings(), &s);
}

#[test]
fn unrelated_hand_edit_is_rebased() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", HAND_WRITTEN);
    let mut cfg = ConfigFile::load(&path).unwrap();
    let edited = HAND_WRITTEN.replace("min = 5", "min = 10\nmax = 90");
    std::fs::write(&path, &edited).unwrap();
    let mut s = cfg.settings().clone();
    s.callsign = call("K1ABC");
    assert_eq!(cfg.save(&s), Ok(Saved::Rebased));
    assert_eq!(
        read(&path),
        edited.replace("\"W1AW\"   # club call", "\"K1ABC\"   # club call")
    );
}

#[test]
fn same_value_on_both_sides_is_not_a_conflict() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", HAND_WRITTEN);
    let mut cfg = ConfigFile::load(&path).unwrap();
    std::fs::write(&path, HAND_WRITTEN.replace("W1AW", "K1ABC")).unwrap();
    let mut s = cfg.settings().clone();
    s.callsign = call("K1ABC");
    assert_eq!(cfg.save(&s), Ok(Saved::Rebased));
    assert_eq!(read(&path), HAND_WRITTEN.replace("W1AW", "K1ABC"));
}

#[test]
fn same_field_changed_differently_is_a_conflict_with_both_values() {
    let dir = TempDir::new();
    let path = dir.write("config.toml", HAND_WRITTEN);
    let mut cfg = ConfigFile::load(&path).unwrap();
    let theirs = HAND_WRITTEN.replace("W1AW", "N9XYZ");
    std::fs::write(&path, &theirs).unwrap();
    let mut s = cfg.settings().clone();
    s.callsign = call("K1ABC");
    s.log_path = Some("/h/l.adi".into());
    assert_eq!(
        cfg.save(&s),
        Err(SaveError::Conflict(vec![FieldConflict {
            key: "callsign".into(),
            file: Some("N9XYZ".into()),
            mine: Some("K1ABC".into()),
        }]))
    );
    assert_eq!(read(&path), theirs, "nothing written");

    // Resolve by keeping the file's callsign: reload, apply the rest, save again.
    cfg.reload().unwrap();
    let mut s = cfg.settings().clone();
    s.log_path = Some("/h/l.adi".into());
    assert_eq!(cfg.save(&s), Ok(Saved::Written));
    assert!(read(&path).contains("N9XYZ"));
    assert!(read(&path).contains("path = \"/h/l.adi\""));
}
