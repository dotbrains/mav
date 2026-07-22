use super::tests_common::*;
use super::*;

#[test]
fn test_file_ord() {
    let wt0_root = SettingsFile::Project((WorktreeId::from_usize(0), RelPath::empty_arc()));
    let wt0_child1 =
        SettingsFile::Project((WorktreeId::from_usize(0), rel_path("child1").into_arc()));
    let wt0_child2 =
        SettingsFile::Project((WorktreeId::from_usize(0), rel_path("child2").into_arc()));

    let wt1_root = SettingsFile::Project((WorktreeId::from_usize(1), RelPath::empty_arc()));
    let wt1_subdir =
        SettingsFile::Project((WorktreeId::from_usize(1), rel_path("subdir").into_arc()));

    let mut files = vec![
        &wt1_root,
        &SettingsFile::Default,
        &wt0_root,
        &wt1_subdir,
        &wt0_child2,
        &SettingsFile::Server,
        &wt0_child1,
        &SettingsFile::User,
    ];

    files.sort();
    pretty_assertions::assert_eq!(
        files,
        vec![
            &wt0_child2,
            &wt0_child1,
            &wt0_root,
            &wt1_subdir,
            &wt1_root,
            &SettingsFile::Server,
            &SettingsFile::User,
            &SettingsFile::Default,
        ]
    )
}

#[gpui::test]
fn test_lsp_settings_schema_generation(cx: &mut App) {
    SettingsStore::test(cx);

    let schema = SettingsStore::json_schema(&SettingsJsonSchemaParams {
        language_names: &["Rust".to_string(), "TypeScript".to_string()],
        font_names: &["Mav Mono".to_string()],
        theme_names: &["One Dark".into()],
        icon_theme_names: &["Mav Icons".into()],
        lsp_adapter_names: &[
            "rust-analyzer".to_string(),
            "typescript-language-server".to_string(),
        ],
        action_names: &[],
        action_documentation: &HashMap::default(),
        deprecations: &HashMap::default(),
        deprecation_messages: &HashMap::default(),
    });

    let properties = schema
        .pointer("/$defs/LspSettingsMap/properties")
        .expect("LspSettingsMap should have properties")
        .as_object()
        .unwrap();

    assert!(properties.contains_key("rust-analyzer"));
    assert!(properties.contains_key("typescript-language-server"));

    let init_options_ref = properties
        .get("rust-analyzer")
        .unwrap()
        .pointer("/properties/initialization_options/$ref")
        .expect("initialization_options should have a $ref")
        .as_str()
        .unwrap();

    assert_eq!(
        init_options_ref,
        "mav://schemas/settings/lsp/rust-analyzer/initialization_options"
    );

    let settings_ref = properties
        .get("rust-analyzer")
        .unwrap()
        .pointer("/properties/settings/$ref")
        .expect("settings should have a $ref")
        .as_str()
        .unwrap();

    assert_eq!(
        settings_ref,
        "mav://schemas/settings/lsp/rust-analyzer/settings"
    );
}

#[gpui::test]
fn test_lsp_project_settings_schema_generation(cx: &mut App) {
    SettingsStore::test(cx);

    let schema = SettingsStore::project_json_schema(&SettingsJsonSchemaParams {
        language_names: &["Rust".to_string(), "TypeScript".to_string()],
        font_names: &["Mav Mono".to_string()],
        theme_names: &["One Dark".into()],
        icon_theme_names: &["Mav Icons".into()],
        lsp_adapter_names: &[
            "rust-analyzer".to_string(),
            "typescript-language-server".to_string(),
        ],
        action_names: &[],
        action_documentation: &HashMap::default(),
        deprecations: &HashMap::default(),
        deprecation_messages: &HashMap::default(),
    });

    let properties = schema
        .pointer("/$defs/LspSettingsMap/properties")
        .expect("LspSettingsMap should have properties")
        .as_object()
        .unwrap();

    assert!(properties.contains_key("rust-analyzer"));
    assert!(properties.contains_key("typescript-language-server"));

    let init_options_ref = properties
        .get("rust-analyzer")
        .unwrap()
        .pointer("/properties/initialization_options/$ref")
        .expect("initialization_options should have a $ref")
        .as_str()
        .unwrap();

    assert_eq!(
        init_options_ref,
        "mav://schemas/settings/lsp/rust-analyzer/initialization_options"
    );

    let settings_ref = properties
        .get("rust-analyzer")
        .unwrap()
        .pointer("/properties/settings/$ref")
        .expect("settings should have a $ref")
        .as_str()
        .unwrap();

    assert_eq!(
        settings_ref,
        "mav://schemas/settings/lsp/rust-analyzer/settings"
    );
}

#[gpui::test]
fn test_project_json_schema_differs_from_user_schema(cx: &mut App) {
    SettingsStore::test(cx);

    let params = SettingsJsonSchemaParams {
        language_names: &["Rust".to_string()],
        font_names: &["Mav Mono".to_string()],
        theme_names: &["One Dark".into()],
        icon_theme_names: &["Mav Icons".into()],
        lsp_adapter_names: &["rust-analyzer".to_string()],
        action_names: &[],
        action_documentation: &HashMap::default(),
        deprecations: &HashMap::default(),
        deprecation_messages: &HashMap::default(),
    };

    let user_schema = SettingsStore::json_schema(&params);
    let project_schema = SettingsStore::project_json_schema(&params);

    assert_ne!(user_schema, project_schema);

    let user_schema_str = serde_json::to_string(&user_schema).unwrap();
    let project_schema_str = serde_json::to_string(&project_schema).unwrap();

    assert!(user_schema_str.contains("\"auto_update\""));
    assert!(!project_schema_str.contains("\"auto_update\""));
}
