//! The config dialog, tested through `App`: key events in, commands and rendered buffer out, and
//! real files in a unique temp directory.
mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use common::*;
use hamtools_config::{RigKind, RigProfile};
use logbook::Grid;
use ratatui::crossterm::event::KeyCode;
use rig_tui::geocode::{Candidate, GeocodeError};
use rig_tui::{App, Command};

fn scratch(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rig-tui-dialog-{}-{}-{name}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn file_with(name: &str, text: &str) -> PathBuf {
    let path = scratch(name).join("config.toml");
    std::fs::write(&path, text).unwrap();
    path
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn kx3_profile(addr: &str) -> RigProfile {
    RigProfile {
        name: "kx3".into(),
        addr: addr.into(),
        kind: RigKind::Kx3,
        cw: None,
        split: None,
        sat_mode: None,
    }
}

fn app_for(path: &Path) -> App {
    App::new()
        .with_rigs(&[kx3_profile("127.0.0.1:4532")], None)
        .with_config_path(Some(path.to_path_buf()))
}

fn open(app: &mut App) {
    assert_eq!(app.on_key(key(KeyCode::Char('c'))), vec![]);
    assert!(app.has_overlay());
}

fn rows(app: &App) -> Vec<String> {
    screen(app, 100, 44)
}

fn shows(app: &App, needle: &str) -> bool {
    rows(app).iter().any(|r| r.contains(needle))
}

fn tabs(app: &mut App, n: usize) {
    for _ in 0..n {
        app.on_key(key(KeyCode::Tab));
    }
}

fn type_in(app: &mut App, text: &str) {
    for c in text.chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
}

fn candidate(name: &str, locator: &str) -> Candidate {
    Candidate {
        name: name.to_string(),
        locator: Grid::new(locator).unwrap(),
    }
}

// Field order: 0 Callsign, 1 Locator, 2 Address, 3 Log path, 4 Rig address, 5 Default rig,
// 6..=11 F1..F6.
const LOCATOR: usize = 1;
const ADDRESS: usize = 2;
const RIG: usize = 4;
const DEFAULT_RIG: usize = 5;
const F1: usize = 6;

const SAMPLE: &str = "# my station\ncallsign = \"W1AW\"\n\n[rig.kx3]\naddr = \"127.0.0.1:4533\"\nkind = \"kx3\"\n\n[macros]\nF1 = \"CQ DE {CALL}\"\n";

#[test]
fn c_opens_the_dialog_while_disconnected_and_shows_the_file_fresh() {
    let path = file_with("open", SAMPLE);
    let mut app = app_for(&path);

    open(&mut app);

    assert!(!app.connected());
    assert!(shows(&app, "W1AW"));
    assert!(shows(&app, "127.0.0.1:4533"));
    assert!(shows(&app, "CQ DE {CALL}"));
    for heading in ["Station", "Logging", "Rig", "Soft keys"] {
        assert!(shows(&app, heading), "{heading}");
    }
}

#[test]
fn the_address_row_always_says_what_leaves_the_machine() {
    let path = file_with("disclosure", SAMPLE);
    let mut app = app_for(&path);
    open(&mut app);

    let text = rows(&app).join(" ");

    assert!(text.contains("nominatim.openstreetmap.org"), "{text}");
    assert!(text.contains("it is not saved"), "{text}");
}

#[test]
fn a_bad_value_is_flagged_when_leaving_the_field() {
    let path = file_with("leave", SAMPLE);
    let mut app = app_for(&path);
    open(&mut app);
    tabs(&mut app, LOCATOR);

    type_in(&mut app, "en7");
    assert!(!shows(&app, "6 characters"));
    tabs(&mut app, 1);

    assert!(shows(&app, "6 characters"), "{:#?}", rows(&app));
}

#[test]
fn callsign_and_locator_are_uppercased_as_typed() {
    let path = file_with("upper", "");
    let mut app = app_for(&path);
    open(&mut app);

    type_in(&mut app, "kd0rhv");
    tabs(&mut app, 1);
    type_in(&mut app, "en71dq");

    assert!(shows(&app, "KD0RHV"));
    assert!(shows(&app, "EN71DQ"));
}

#[test]
fn save_is_blocked_while_a_field_is_invalid_and_focus_goes_to_the_first_one() {
    let path = file_with("blocked", SAMPLE);
    let mut app = app_for(&path);
    open(&mut app);
    tabs(&mut app, LOCATOR);
    type_in(&mut app, "XX");
    tabs(&mut app, RIG - LOCATOR);
    for _ in 0..30 {
        app.on_key(key(KeyCode::Backspace));
    }
    type_in(&mut app, "nonsense");

    app.on_key(ctrl('s'));

    assert_eq!(read(&path), SAMPLE, "nothing is written");
    assert!(app.has_overlay());
    let text = rows(&app).join("\n");
    assert!(text.contains("6 characters"), "{text}");
    assert!(text.contains("host:port"), "{text}");
    // Focus is on the Locator (the first invalid field): typing goes there.
    app.on_key(key(KeyCode::Backspace));
    app.on_key(key(KeyCode::Backspace));
    type_in(&mut app, "en71dq");
    app.on_key(ctrl('s'));
    assert_eq!(read(&path), SAMPLE, "the rig address is still invalid");
}

#[test]
fn rig_address_must_look_like_host_and_port() {
    for bad in [
        "",
        "localhost",
        "host:",
        ":4532",
        "host:99999",
        "ho st:4532",
    ] {
        let path = file_with("rigaddr", SAMPLE);
        let mut app = app_for(&path);
        open(&mut app);
        tabs(&mut app, RIG);
        for _ in 0..30 {
            app.on_key(key(KeyCode::Backspace));
        }
        type_in(&mut app, bad);

        app.on_key(ctrl('s'));

        assert_eq!(read(&path), SAMPLE, "{bad:?} must not save");
    }
}

#[test]
fn save_patches_the_file_applies_live_and_closes() {
    let path = file_with("save", SAMPLE);
    let mut app = app_for(&path);
    open(&mut app);
    tabs(&mut app, LOCATOR);
    type_in(&mut app, "en71dq");
    tabs(&mut app, F1 - LOCATOR);
    for _ in 0..30 {
        app.on_key(key(KeyCode::Backspace));
    }
    type_in(&mut app, "73 de {call}");

    let commands = app.on_key(ctrl('s'));

    assert!(!app.has_overlay());
    assert_eq!(app.callsign(), Some("W1AW"));
    assert_eq!(app.locator(), Some("EN71DQ"));
    assert_eq!(
        app.soft_keys().get(&1).map(String::as_str),
        Some("73 DE {CALL}")
    );
    assert_eq!(
        commands,
        vec![
            Command::SetRigAddr {
                rig: 0,
                addr: "127.0.0.1:4533".into()
            },
            Command::ClearGeocodeCache
        ]
    );
    assert_eq!(
        read(&path),
        "# my station\ncallsign = \"W1AW\"\n\n[rig.kx3]\naddr = \"127.0.0.1:4533\"\nkind = \"kx3\"\n\n[macros]\nF1 = \"73 DE {CALL}\"\n\n[station]\nlocator = \"EN71DQ\"\n"
    );
}

const TWO_RIGS: &str = "[rig.kx3]\naddr = \"127.0.0.1:4532\"\nkind = \"kx3\"\n\n[rig.ft847]\naddr = \"127.0.0.1:4533\"\nkind = \"ft847\"\n";

#[test]
fn the_default_rig_is_edited_by_profile_name_and_must_name_an_existing_profile() {
    let path = file_with("defaultrig", TWO_RIGS);
    let mut app = app_for(&path);
    open(&mut app);
    tabs(&mut app, DEFAULT_RIG);
    type_in(&mut app, "nope");

    app.on_key(ctrl('s'));

    assert_eq!(read(&path), TWO_RIGS, "an unknown profile must not save");
    assert!(shows(&app, "[rig.NAME]"));
    for _ in 0..4 {
        app.on_key(key(KeyCode::Backspace));
    }
    type_in(&mut app, "ft847");

    app.on_key(ctrl('s'));

    assert!(!app.has_overlay());
    assert_eq!(
        read(&path),
        format!("[rig]\ndefault = \"ft847\"\n{TWO_RIGS}")
    );
}

#[test]
fn a_new_rig_address_drops_the_connection_so_the_retry_loop_reconnects() {
    let path = file_with("newaddr", SAMPLE);
    let mut app = app_for(&path).with_rig_addr("127.0.0.1:4533");
    open(&mut app);
    tabs(&mut app, RIG);
    for _ in 0..30 {
        app.on_key(key(KeyCode::Backspace));
    }
    type_in(&mut app, "127.0.0.1:4532");

    let commands = app.on_key(ctrl('s'));

    assert_eq!(
        commands,
        vec![
            Command::SetRigAddr {
                rig: 0,
                addr: "127.0.0.1:4532".into()
            },
            Command::ClearGeocodeCache
        ]
    );
    assert_eq!(app.rig_addr(), "127.0.0.1:4532");
}

#[test]
fn a_blank_soft_key_removes_the_macro_and_a_blank_log_path_means_default() {
    let path = file_with("blank", SAMPLE);
    let mut app = app_for(&path);
    open(&mut app);
    tabs(&mut app, F1);
    for _ in 0..30 {
        app.on_key(key(KeyCode::Backspace));
    }

    app.on_key(ctrl('s'));

    assert!(app.soft_keys().is_empty());
    assert!(!read(&path).contains("F1"));
    assert_eq!(app.log_path(), None);
}

#[test]
fn a_soft_key_shows_a_dim_preview_of_the_expansion_when_focused() {
    let path = file_with("preview", SAMPLE);
    let mut app = app_for(&path);
    open(&mut app);
    assert!(!shows(&app, "CQ DE W1AW"));

    tabs(&mut app, F1);

    assert!(shows(&app, "CQ DE W1AW"), "{:#?}", rows(&app));
}

#[test]
fn log_path_is_applied_live() {
    let path = file_with("logpath", SAMPLE);
    let mut app = app_for(&path);
    open(&mut app);
    tabs(&mut app, 3);
    type_in(&mut app, "/tmp/mine.adi");

    app.on_key(ctrl('s'));

    assert_eq!(app.log_path(), Some(Path::new("/tmp/mine.adi")));
    assert!(read(&path).contains("path = \"/tmp/mine.adi\""));
}

mod leaving {
    use super::*;

    #[test]
    fn esc_with_nothing_changed_closes_at_once() {
        let path = file_with("esc-clean", SAMPLE);
        let mut app = app_for(&path);
        open(&mut app);

        app.on_key(key(KeyCode::Esc));

        assert!(!app.has_overlay());
    }

    #[test]
    fn esc_after_an_edit_asks_before_discarding() {
        let path = file_with("esc-dirty", SAMPLE);
        let mut app = app_for(&path);
        open(&mut app);
        type_in(&mut app, "x");

        app.on_key(key(KeyCode::Esc));
        assert!(app.has_overlay());
        assert!(shows(&app, "Discard changes? (y/N)"));

        app.on_key(key(KeyCode::Char('n')));
        assert!(app.has_overlay());
        assert!(!shows(&app, "Discard changes?"));

        app.on_key(key(KeyCode::Esc));
        app.on_key(key(KeyCode::Enter));
        assert!(app.has_overlay(), "Enter means the default, no");

        app.on_key(key(KeyCode::Esc));
        app.on_key(key(KeyCode::Char('y')));
        assert!(!app.has_overlay());
        assert_eq!(read(&path), SAMPLE);
        assert_eq!(app.callsign(), None, "discarded edits are not applied");
    }

    #[test]
    fn the_dialog_gets_every_key_so_q_does_not_quit() {
        let path = file_with("noquit", SAMPLE);
        let mut app = app_for(&path);
        open(&mut app);

        assert_eq!(app.on_key(key(KeyCode::Char('q'))), vec![]);
        assert!(app.has_overlay());
    }
}

mod disk_changes {
    use super::*;

    #[test]
    fn an_unrelated_hand_edit_after_opening_is_kept_by_the_save() {
        let path = file_with("rebase", SAMPLE);
        let mut app = app_for(&path);
        open(&mut app);
        std::fs::write(
            &path,
            format!("{SAMPLE}# added by hand\n[logbook]\npath = \"/x.adi\"\n"),
        )
        .unwrap();
        tabs(&mut app, LOCATOR);
        type_in(&mut app, "en71dq");

        app.on_key(ctrl('s'));

        assert!(!app.has_overlay());
        let text = read(&path);
        assert!(text.contains("# added by hand"), "{text}");
        assert!(text.contains("locator = \"EN71DQ\""), "{text}");
    }

    #[test]
    fn a_conflict_lists_both_values_and_f_keeps_the_files_y_keeps_mine() {
        let path = file_with("conflict", SAMPLE);
        let mut app = app_for(&path);
        open(&mut app);
        std::fs::write(&path, SAMPLE.replace("W1AW", "N0FILE")).unwrap();
        for _ in 0..8 {
            app.on_key(key(KeyCode::Backspace));
        }
        type_in(&mut app, "n0mine");

        app.on_key(ctrl('s'));

        assert!(app.has_overlay());
        let text = rows(&app).join("\n");
        assert!(text.contains("callsign"), "{text}");
        assert!(text.contains("N0FILE"), "{text}");
        assert!(text.contains("N0MINE"), "{text}");
        assert!(read(&path).contains("N0FILE"), "nothing written yet");

        // Keep mine, then save again.
        app.on_key(key(KeyCode::Char('y')));
        app.on_key(ctrl('s'));
        assert!(!app.has_overlay());
        assert_eq!(app.callsign(), Some("N0MINE"));
        assert!(read(&path).contains("callsign = \"N0MINE\""));
    }

    #[test]
    fn f_keeps_the_files_value() {
        let path = file_with("conflict-f", SAMPLE);
        let mut app = app_for(&path);
        open(&mut app);
        std::fs::write(&path, SAMPLE.replace("W1AW", "N0FILE")).unwrap();
        for _ in 0..8 {
            app.on_key(key(KeyCode::Backspace));
        }
        type_in(&mut app, "n0mine");
        app.on_key(ctrl('s'));

        app.on_key(key(KeyCode::Char('f')));
        assert!(shows(&app, "N0FILE"));
        app.on_key(ctrl('s'));

        assert!(!app.has_overlay());
        assert_eq!(app.callsign(), Some("N0FILE"));
        assert!(read(&path).contains("callsign = \"N0FILE\""));
    }
}

mod file_states {
    use super::*;

    #[test]
    fn an_unparseable_file_opens_with_the_error_and_is_never_overwritten() {
        let broken = "callsign = \n[rig\n";
        let path = file_with("broken", broken);
        let mut app = app_for(&path);

        open(&mut app);

        assert!(shows(&app, "line"), "{:#?}", rows(&app));
        assert!(shows(&app, "Ctrl-R"));
        app.on_key(ctrl('s'));
        assert_eq!(read(&path), broken);
        assert!(app.has_overlay());

        // Fix the file by hand, then Ctrl-R.
        std::fs::write(&path, SAMPLE).unwrap();
        app.on_key(ctrl('r'));
        assert!(!shows(&app, "Ctrl-R"));
        assert!(shows(&app, "W1AW"));
        app.on_key(ctrl('s'));
        assert!(!app.has_overlay());
    }

    #[test]
    fn ctrl_r_on_a_still_broken_file_stays_in_the_error_state() {
        let path = file_with("still-broken", "callsign = \n");
        let mut app = app_for(&path);
        open(&mut app);

        app.on_key(ctrl('r'));

        assert!(shows(&app, "Ctrl-R"));
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_file_opens_with_a_banner_and_ctrl_a_applies_for_this_run_only() {
        let dir = scratch("readonly");
        let real = dir.join("managed.toml");
        std::fs::write(&real, SAMPLE).unwrap();
        let link = dir.join("config.toml");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let mut app = app_for(&link);
        open(&mut app);
        assert!(shows(&app, "Read-only"), "{:#?}", rows(&app));
        assert!(shows(&app, "Ctrl-A"));
        tabs(&mut app, LOCATOR);
        type_in(&mut app, "en71dq");

        app.on_key(ctrl('s'));
        assert!(app.has_overlay(), "Save is disabled");
        assert_eq!(read(&real), SAMPLE);
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(app.locator(), None);

        app.on_key(ctrl('a'));
        assert!(!app.has_overlay());
        assert_eq!(app.locator(), Some("EN71DQ"));
        assert_eq!(read(&real), SAMPLE, "nothing written");
    }

    #[test]
    fn unknown_keys_are_kept_and_counted() {
        let path = file_with("unknown", "watch = [\"AO-7\"]\n[extra]\nthing = 1\n");
        let mut app = app_for(&path);
        open(&mut app);

        assert!(shows(&app, "2 unknown keys kept"), "{:#?}", rows(&app));
    }

    #[test]
    fn a_missing_file_opens_empty_and_is_created_on_save() {
        let path = scratch("missing").join("sub").join("config.toml");
        let mut app = app_for(&path);
        open(&mut app);
        type_in(&mut app, "w1aw");

        app.on_key(ctrl('s'));

        assert_eq!(read(&path), "callsign = \"W1AW\"\n");
    }
}

mod lookup {
    use super::*;

    fn at_address(path: &Path) -> App {
        let mut app = app_for(path);
        open(&mut app);
        tabs(&mut app, ADDRESS);
        app
    }

    #[test]
    fn enter_on_the_address_row_asks_for_a_lookup_and_ignores_enter_while_in_flight() {
        let path = file_with("lookup", SAMPLE);
        let mut app = at_address(&path);
        type_in(&mut app, "Middlebury, IN");

        let first = app.on_key(key(KeyCode::Enter));
        let second = app.on_key(key(KeyCode::Enter));

        assert_eq!(first, vec![Command::Lookup("Middlebury, IN".into())]);
        assert_eq!(second, vec![]);
        assert!(shows(&app, "looking up"));
    }

    #[test]
    fn candidates_are_listed_with_their_locator_and_enter_fills_the_locator_field() {
        let path = file_with("candidates", SAMPLE);
        let mut app = at_address(&path);
        type_in(&mut app, "Middlebury");
        app.on_key(key(KeyCode::Enter));

        app.on_geocode(Ok(vec![
            candidate("Middlebury, Indiana", "EN71DQ"),
            candidate("Middlebury, Vermont", "FN34XA"),
        ]));

        assert!(shows(&app, "Middlebury, Indiana"));
        assert!(shows(&app, "FN34XA"));
        assert!(shows(&app, "OpenStreetMap contributors"));
        assert!(!shows(&app, "looking up"));

        app.on_key(key(KeyCode::Down));
        app.on_key(key(KeyCode::Enter));

        assert!(shows(&app, "FN34XA"));
        assert_eq!(read(&path), SAMPLE, "nothing saved until Save");
        app.on_key(ctrl('s'));
        let text = read(&path);
        assert!(text.contains("locator = \"FN34XA\""), "{text}");
        assert!(
            !text.contains("Middlebury"),
            "the address is never written: {text}"
        );
    }

    #[test]
    fn a_hand_edit_after_a_lookup_wins() {
        let path = file_with("handedit", SAMPLE);
        let mut app = at_address(&path);
        type_in(&mut app, "Middlebury");
        app.on_key(key(KeyCode::Enter));
        app.on_geocode(Ok(vec![candidate("Middlebury, Indiana", "EN71DQ")]));
        app.on_key(key(KeyCode::Enter));
        // Selecting a candidate leaves focus on the Locator field.
        app.on_key(key(KeyCode::Backspace));
        app.on_key(key(KeyCode::Backspace));
        type_in(&mut app, "aa");

        app.on_key(ctrl('s'));

        assert!(read(&path).contains("locator = \"EN71AA\""));
    }

    #[test]
    fn esc_closes_the_candidate_list_without_changing_the_locator() {
        let path = file_with("candidates-esc", SAMPLE);
        let mut app = at_address(&path);
        type_in(&mut app, "Middlebury");
        app.on_key(key(KeyCode::Enter));
        app.on_geocode(Ok(vec![candidate("Middlebury, Indiana", "EN71DQ")]));

        app.on_key(key(KeyCode::Esc));

        assert!(app.has_overlay());
        assert!(!shows(&app, "Middlebury, Indiana"));
        assert!(!shows(&app, "EN71DQ"));
    }

    #[test]
    fn failures_are_one_line_messages_and_the_dialog_stays_usable() {
        let cases: [(Result<Vec<Candidate>, GeocodeError>, &str); 3] = [
            (Ok(vec![]), "no match"),
            (Err(GeocodeError::RateLimited), "rate limited"),
            (Err(GeocodeError::Offline("down".into())), "no network"),
        ];
        for (result, message) in cases {
            let path = file_with("failure", SAMPLE);
            let mut app = at_address(&path);
            type_in(&mut app, "Nowhere");
            app.on_key(key(KeyCode::Enter));

            app.on_geocode(result);

            assert!(shows(&app, message), "{message}: {:#?}", rows(&app));
            assert!(!shows(&app, "looking up"));
            // Another lookup is allowed, and editing still works.
            assert_eq!(
                app.on_key(key(KeyCode::Enter)),
                vec![Command::Lookup("Nowhere".into())]
            );
        }
    }

    #[test]
    fn a_late_answer_after_the_dialog_closed_is_ignored() {
        let path = file_with("late", SAMPLE);
        let mut app = at_address(&path);
        type_in(&mut app, "Middlebury");
        app.on_key(key(KeyCode::Enter));
        app.on_key(key(KeyCode::Esc));
        app.on_key(key(KeyCode::Char('y')));
        assert!(!app.has_overlay());

        app.on_geocode(Ok(vec![candidate("Middlebury, Indiana", "EN71DQ")]));

        assert!(!app.has_overlay());
    }

    #[test]
    fn closing_the_dialog_clears_the_session_cache() {
        let path = file_with("cache-clear", SAMPLE);
        let mut app = at_address(&path);

        let commands = app.on_key(key(KeyCode::Esc));

        assert_eq!(commands, vec![Command::ClearGeocodeCache]);
    }

    #[test]
    fn a_blank_address_is_not_sent() {
        let path = file_with("blank-address", SAMPLE);
        let mut app = at_address(&path);

        assert_eq!(app.on_key(key(KeyCode::Enter)), vec![]);
        assert!(!shows(&app, "looking up"));
    }
}
