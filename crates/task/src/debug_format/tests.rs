use crate::DebugScenario;
use serde_json::json;

#[test]
fn test_just_build_args() {
    let json = r#"{
        "label": "Build & debug rust",
        "adapter": "CodeLLDB",
        "build": {
            "command": "rust",
            "args": ["build"]
        }
    }"#;

    let deserialized: DebugScenario = serde_json::from_str(json).unwrap();
    assert!(deserialized.build.is_some());
    match deserialized.build.as_ref().unwrap() {
        crate::BuildTaskDefinition::Template { task_template, .. } => {
            assert_eq!("debug-build", task_template.label);
            assert_eq!("rust", task_template.command);
            assert_eq!(vec!["build"], task_template.args);
        }
        _ => panic!("Expected Template variant"),
    }
    assert_eq!(json!({}), deserialized.config);
    assert_eq!("CodeLLDB", deserialized.adapter.as_ref());
    assert_eq!("Build & debug rust", deserialized.label.as_ref());
}

#[test]
fn test_empty_scenario_has_none_request() {
    let json = r#"{
        "label": "Build & debug rust",
        "build": "rust",
        "adapter": "CodeLLDB"
    }"#;

    let deserialized: DebugScenario = serde_json::from_str(json).unwrap();

    assert_eq!(json!({}), deserialized.config);
    assert_eq!("CodeLLDB", deserialized.adapter.as_ref());
    assert_eq!("Build & debug rust", deserialized.label.as_ref());
}

#[test]
fn test_launch_scenario_deserialization() {
    let json = r#"{
        "label": "Launch program",
        "adapter": "CodeLLDB",
        "request": "launch",
        "program": "target/debug/myapp",
        "args": ["--test"]
    }"#;

    let deserialized: DebugScenario = serde_json::from_str(json).unwrap();

    assert_eq!(
        json!({ "request": "launch", "program": "target/debug/myapp", "args": ["--test"] }),
        deserialized.config
    );
    assert_eq!("CodeLLDB", deserialized.adapter.as_ref());
    assert_eq!("Launch program", deserialized.label.as_ref());
}

#[test]
fn test_attach_scenario_deserialization() {
    let json = r#"{
        "label": "Attach to process",
        "adapter": "CodeLLDB",
        "process_id": 1234,
        "request": "attach"
    }"#;

    let deserialized: DebugScenario = serde_json::from_str(json).unwrap();

    assert_eq!(
        json!({ "request": "attach", "process_id": 1234 }),
        deserialized.config
    );
    assert_eq!("CodeLLDB", deserialized.adapter.as_ref());
    assert_eq!("Attach to process", deserialized.label.as_ref());
}

#[test]
fn test_build_task_definition_without_label() {
    use crate::BuildTaskDefinition;

    let json = r#""my_build_task""#;
    let deserialized: BuildTaskDefinition = serde_json::from_str(json).unwrap();
    match deserialized {
        BuildTaskDefinition::ByName(name) => assert_eq!("my_build_task", name.as_ref()),
        _ => panic!("Expected ByName variant"),
    }

    let json = r#"{
        "command": "cargo",
        "args": ["build", "--release"]
    }"#;
    let deserialized: BuildTaskDefinition = serde_json::from_str(json).unwrap();
    match deserialized {
        BuildTaskDefinition::Template { task_template, .. } => {
            assert_eq!("debug-build", task_template.label);
            assert_eq!("cargo", task_template.command);
            assert_eq!(vec!["build", "--release"], task_template.args);
        }
        _ => panic!("Expected Template variant"),
    }

    let json = r#"{
        "label": "Build Release",
        "command": "cargo",
        "args": ["build", "--release"]
    }"#;
    let deserialized: BuildTaskDefinition = serde_json::from_str(json).unwrap();
    match deserialized {
        BuildTaskDefinition::Template { task_template, .. } => {
            assert_eq!("Build Release", task_template.label);
            assert_eq!("cargo", task_template.command);
            assert_eq!(vec!["build", "--release"], task_template.args);
        }
        _ => panic!("Expected Template variant"),
    }
}
