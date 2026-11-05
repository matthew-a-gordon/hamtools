//! The command line, run as a real process. Every case fails before the terminal is touched.

use std::process::{Command, Output};

const CONFIG: &str =
    "[rig]\ndefault = \"kx3\"\n[rig.kx3]\naddr = \"127.0.0.1:1\"\nkind = \"kx3\"\n";

fn run(args: &[&str]) -> Output {
    let dir = std::env::temp_dir().join(format!("rig-tui-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, CONFIG).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rig-tui"))
        .arg("--config")
        .arg(&path)
        .args(args)
        .output()
        .unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn a_positional_address_is_rejected_with_a_pointer_to_rig() {
    let o = run(&["127.0.0.1:4533"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("--rig"), "{}", stderr(&o));
}

#[test]
fn an_unknown_rig_name_is_an_error() {
    let o = run(&["--rig", "nope"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("nope"), "{}", stderr(&o));
}
