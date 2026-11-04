mod common;

use common::TempDir;
use hamtools_config::{
    ConfigError, Plan, PlanActivity, PlansFile, RefSnapshot, SaveError, Saved, valid_date,
};

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

const HAND_WRITTEN: &str = "\
# my plans
mystery = 1

[plan.pota-us-4571]
activity = \"pota\"      # park
references = [\"US-4571\", \"US-4572\"]
date = \"2026-11-05\"
bands = [\"40m\"]
modes = [\"CW\"]
status = \"active\"
colour = \"blue\"        # unknown to the app
[plan.pota-us-4571.softkeys]
F1 = \"cq pota de {MYCALL}\"

[plan.sota-w4c-cm-001]
activity = \"sota\"
references = [\"W4C/CM-001\"]
date = \"2026-12-01\"
notes = \"cold\"
";

fn new_plan(id: &str) -> Plan {
    Plan {
        id: id.to_string(),
        activity: PlanActivity::Pota,
        references: vec!["US-1234".into()],
        date: "2027-01-02".into(),
        bands: vec!["20M".into()],
        modes: vec![],
        softkey_set: Some("pota".into()),
        softkeys: Default::default(),
        notes: None,
        done: false,
        snapshot: Some(RefSnapshot {
            reference: "US-1234".into(),
            name: "Pigeon River".into(),
            lat: Some(41.65),
            alt_m: Some(300),
            ..Default::default()
        }),
    }
}

#[test]
fn reads_plans_and_reads_a_stored_active_as_planned() {
    let dir = TempDir::new();
    let file = PlansFile::load(&dir.write("plans.toml", HAND_WRITTEN)).unwrap();
    let plans = file.plans();
    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].references, ["US-4571", "US-4572"]);
    assert_eq!(plans[0].bands, ["40M"]);
    assert_eq!(plans[0].softkeys[&1], "CQ POTA DE {MYCALL}");
    assert!(!plans[0].done);
    assert_eq!(plans[1].activity, PlanActivity::Sota);
    let names: Vec<&str> = file.warnings().iter().map(|w| w.name.as_str()).collect();
    assert_eq!(names, ["mystery", "plan.pota-us-4571.colour"]);
}

#[test]
fn a_missing_file_is_no_plans() {
    let dir = TempDir::new();
    let file = PlansFile::load(&dir.path().join("plans.toml")).unwrap();
    assert!(file.plans().is_empty());
}

#[test]
fn save_keeps_comments_and_unknown_fields_and_patches_only_the_change() {
    let dir = TempDir::new();
    let path = dir.write("plans.toml", HAND_WRITTEN);
    let mut file = PlansFile::load(&path).unwrap();
    let mut plans = file.plans().to_vec();
    plans[0].done = true;
    assert_eq!(file.save(&plans), Ok(Saved::Written));
    assert_eq!(
        read(&path),
        HAND_WRITTEN.replace("status = \"active\"", "status = \"done\"")
    );
    assert!(PlansFile::load(&path).unwrap().plans()[0].done);
}

#[test]
fn a_new_plan_round_trips_with_its_snapshot() {
    let dir = TempDir::new();
    let path = dir.write("plans.toml", HAND_WRITTEN);
    let mut file = PlansFile::load(&path).unwrap();
    let mut plans = file.plans().to_vec();
    plans.push(new_plan("pota-us-1234"));
    file.save(&plans).unwrap();
    let text = read(&path);
    assert!(text.starts_with(HAND_WRITTEN.trim_end()), "{text}");
    let back = PlansFile::load(&path).unwrap();
    assert_eq!(back.plans()[2], new_plan("pota-us-1234"));
}

#[test]
fn deleting_a_plan_removes_only_its_tables() {
    let dir = TempDir::new();
    let path = dir.write("plans.toml", HAND_WRITTEN);
    let mut file = PlansFile::load(&path).unwrap();
    let plans = vec![file.plans()[1].clone()];
    file.save(&plans).unwrap();
    let text = read(&path);
    assert!(!text.contains("us-4571"), "{text}");
    assert!(text.contains("# my plans") && text.contains("mystery = 1"));
    assert!(text.contains("[plan.sota-w4c-cm-001]"));
}

#[test]
fn removing_the_last_override_drops_its_table() {
    let dir = TempDir::new();
    let path = dir.write("plans.toml", HAND_WRITTEN);
    let mut file = PlansFile::load(&path).unwrap();
    let mut plans = file.plans().to_vec();
    plans[0].softkeys.clear();
    file.save(&plans).unwrap();
    assert!(!read(&path).contains("softkeys"));
}

#[test]
fn a_change_on_disk_to_another_field_is_kept() {
    let dir = TempDir::new();
    let path = dir.write("plans.toml", HAND_WRITTEN);
    let mut file = PlansFile::load(&path).unwrap();
    std::fs::write(&path, HAND_WRITTEN.replace("cold", "windy")).unwrap();
    let mut plans = file.plans().to_vec();
    plans[0].done = true;
    assert_eq!(file.save(&plans), Ok(Saved::Rebased));
    let text = read(&path);
    assert!(text.contains("windy") && text.contains("status = \"done\""));
}

#[test]
fn the_same_field_changed_both_ways_is_a_conflict_and_writes_nothing() {
    let dir = TempDir::new();
    let path = dir.write("plans.toml", HAND_WRITTEN);
    let mut file = PlansFile::load(&path).unwrap();
    let on_disk = HAND_WRITTEN.replace("cold", "windy");
    std::fs::write(&path, &on_disk).unwrap();
    let mut plans = file.plans().to_vec();
    plans[1].notes = Some("hot".into());
    let Err(SaveError::Conflict(c)) = file.save(&plans) else {
        panic!("expected a conflict");
    };
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].key, "plan.sota-w4c-cm-001.notes");
    assert_eq!(read(&path), on_disk);
}

#[test]
fn bad_values_name_the_key_and_a_broken_file_is_never_saved_over() {
    let dir = TempDir::new();
    let two = HAND_WRITTEN.replace(
        "references = [\"W4C/CM-001\"]",
        "references = [\"A/B-1\", \"A/B-2\"]",
    );
    let e = PlansFile::load(&dir.write("a.toml", &two)).unwrap_err();
    assert!(
        matches!(&e, ConfigError::Invalid { key, .. } if key == "plan.sota-w4c-cm-001.references")
    );
    let bad_date = HAND_WRITTEN.replace("2026-12-01", "2026-13-01");
    assert!(PlansFile::load(&dir.write("b.toml", &bad_date)).is_err());
    assert!(PlansFile::load(&dir.write("c.toml", "[plan")).is_err());

    let path = dir.write("plans.toml", HAND_WRITTEN);
    let mut file = PlansFile::load(&path).unwrap();
    std::fs::write(&path, "[plan").unwrap();
    let mut plans = file.plans().to_vec();
    plans[0].done = true;
    assert!(matches!(
        file.save(&plans),
        Err(SaveError::Unparseable { .. })
    ));
    assert_eq!(read(&path), "[plan");
}

#[test]
fn dates_are_checked() {
    assert!(valid_date("2028-02-29"));
    assert!(!valid_date("2027-02-29"));
    assert!(!valid_date("2027-1-05"));
    assert!(!valid_date("tomorrow"));
}
