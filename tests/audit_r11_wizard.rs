use greentic_flow::wizard::{WizardPlan, WizardPlanStep, execute_plan};
use greentic_flow::wizard_state::{
    load_wizard_state, remove_wizard_step, update_wizard_state, wizard_state_path,
};
use tempfile::tempdir;

#[test]
fn corrupt_wizard_state_self_heals() {
    let dir = tempdir().expect("tempdir");
    let flow = dir.path().join("main.ygtc");
    let state_path = wizard_state_path(&flow, "main");
    std::fs::create_dir_all(state_path.parent().expect("parent")).expect("mkdir");
    std::fs::write(&state_path, [0xff, 0x00, 0x01]).expect("write corrupt");

    update_wizard_state(&flow, "main", "n1", "default", "en").expect("update self-heals");
    let state = load_wizard_state(&flow, "main")
        .expect("load")
        .expect("some");
    assert_eq!(state.steps.len(), 1);

    std::fs::write(&state_path, []).expect("truncate");
    remove_wizard_step(&flow, "main", "n1").expect("remove self-heals");
}

#[test]
fn wizard_state_write_leaves_no_temp_files() {
    let dir = tempdir().expect("tempdir");
    let flow = dir.path().join("main.ygtc");
    update_wizard_state(&flow, "main", "n1", "default", "en").expect("update");
    let cache = wizard_state_path(&flow, "main");
    let names: Vec<_> = std::fs::read_dir(cache.parent().expect("parent"))
        .expect("read_dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["main.cbor".to_string()]);
}

#[test]
fn execute_plan_refuses_to_overwrite_existing_flow() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("flows/main.ygtc");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(&path, "authored").expect("seed");
    let plan = WizardPlan {
        mode: "scaffold".to_string(),
        validate: false,
        steps: vec![WizardPlanStep::WriteFile {
            path: path.clone(),
            content: "scaffold".to_string(),
        }],
    };
    assert!(execute_plan(&plan).is_err());
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "authored");
}
