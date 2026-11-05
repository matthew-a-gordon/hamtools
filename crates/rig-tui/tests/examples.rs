//! The documented examples stay valid: `config.example.toml` loads cleanly and every soft key
//! message in it and in the user guide passes the one shared validator.

use std::path::{Path, PathBuf};

use rig_tui::softkeys::{SLOTS, validate};

fn repo_file(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name)
}

fn assert_sendable(origin: &str, msg: &str) {
    let report = validate(msg);
    assert!(
        report.problems.is_empty() && report.warnings.is_empty(),
        "{origin}: {msg:?}: {report:?}"
    );
}

#[test]
fn the_example_config_loads_without_errors_and_its_soft_keys_validate() {
    let (settings, errors) =
        hamtools_config::load_lenient(&repo_file("config.example.toml")).expect("example loads");
    assert!(errors.is_empty(), "{errors:?}");

    let mut sets = vec![("macros".to_string(), settings.macros)];
    sets.extend(settings.softkey_sets);
    for name in ["pota", "sota"] {
        assert!(sets.iter().any(|(n, _)| n == name), "missing set {name}");
    }
    for (name, keys) in sets {
        assert_eq!(
            keys.len(),
            usize::from(SLOTS),
            "{name} should fill F1 to F6"
        );
        for (n, msg) in keys {
            assert_sendable(&format!("config.example.toml {name} F{n}"), &msg);
        }
    }
}

#[test]
fn the_soft_key_examples_in_the_user_guide_validate() {
    let guide = std::fs::read_to_string(repo_file("docs/user-guide.md")).expect("guide reads");
    let mut checked = 0;
    for line in guide.lines() {
        let Some(rest) = line.strip_prefix('F') else {
            continue;
        };
        let Some((n, msg)) = rest.split_once(" = \"") else {
            continue;
        };
        let Some(msg) = msg.strip_suffix('"') else {
            continue;
        };
        if n.parse::<u8>().is_ok() {
            assert_sendable(&format!("user-guide.md F{n}"), msg);
            checked += 1;
        }
    }
    assert!(checked >= 18, "found only {checked} soft key lines");
}
