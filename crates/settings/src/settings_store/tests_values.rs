use super::tests_common::*;
use super::*;

#[gpui::test]
fn test_update_git_settings(cx: &mut App) {
    let store = SettingsStore::new(cx, &test_settings());

    let actual = store
        .new_text_for_update("{}".to_string(), |current| {
            current
                .git
                .get_or_insert_default()
                .inline_blame
                .get_or_insert_default()
                .enabled = Some(true);
        })
        .unwrap();
    pretty_assertions::assert_str_eq!(
        actual,
        r#"{
              "git": {
                "inline_blame": {
                  "enabled": true
                }
              }
            }
            "#
        .unindent()
    );
}

#[gpui::test]
fn test_global_settings(cx: &mut App) {
    let mut store = SettingsStore::new(cx, &test_settings());
    store.register_setting::<ItemSettings>();

    // Set global settings - these should override defaults but not user settings
    store
        .set_global_settings(
            r#"{
                    "tabs": {
                        "close_position": "right",
                        "git_status": true,
                    }
                }"#,
            cx,
        )
        .unwrap();

    // Before user settings, global settings should apply
    assert_eq!(
        store.get::<ItemSettings>(None),
        &ItemSettings {
            close_position: ClosePosition::Right,
            git_status: true,
        }
    );

    // Set user settings - these should override both defaults and global
    store
        .set_user_settings(
            r#"{
                    "tabs": {
                        "close_position": "left"
                    }
                }"#,
            cx,
        )
        .unwrap();

    // User settings should override global settings
    assert_eq!(
        store.get::<ItemSettings>(None),
        &ItemSettings {
            close_position: ClosePosition::Left,
            git_status: true, // Staff from global settings
        }
    );
}

#[gpui::test]
fn test_get_value_for_field_basic(cx: &mut App) {
    let mut store = SettingsStore::new(cx, &test_settings());
    store.register_setting::<DefaultLanguageSettings>();

    store
        .set_user_settings(r#"{"preferred_line_length": 0}"#, cx)
        .unwrap();
    let local = (WorktreeId::from_usize(0), RelPath::empty_arc());
    store
        .set_local_settings(
            local.0,
            LocalSettingsPath::InWorktree(local.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{}"#),
            cx,
        )
        .unwrap();

    fn get(content: &SettingsContent) -> Option<&u32> {
        content
            .project
            .all_languages
            .defaults
            .preferred_line_length
            .as_ref()
    }

    let default_value = *get(&store.default_settings).unwrap();

    assert_eq!(
        store.get_value_from_file(SettingsFile::Project(local.clone()), get),
        (SettingsFile::User, Some(&0))
    );
    assert_eq!(
        store.get_value_from_file(SettingsFile::User, get),
        (SettingsFile::User, Some(&0))
    );
    store.set_user_settings(r#"{}"#, cx).unwrap();
    assert_eq!(
        store.get_value_from_file(SettingsFile::Project(local.clone()), get),
        (SettingsFile::Default, Some(&default_value))
    );
    store
        .set_local_settings(
            local.0,
            LocalSettingsPath::InWorktree(local.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{"preferred_line_length": 80}"#),
            cx,
        )
        .unwrap();
    assert_eq!(
        store.get_value_from_file(SettingsFile::Project(local.clone()), get),
        (SettingsFile::Project(local), Some(&80))
    );
    assert_eq!(
        store.get_value_from_file(SettingsFile::User, get),
        (SettingsFile::Default, Some(&default_value))
    );
}

#[gpui::test]
fn test_get_value_for_field_local_worktrees_dont_interfere(cx: &mut App) {
    let mut store = SettingsStore::new(cx, &test_settings());
    store.register_setting::<DefaultLanguageSettings>();
    store.register_setting::<AutoUpdateSetting>();

    let local_1 = (WorktreeId::from_usize(0), RelPath::empty_arc());

    let local_1_child = (
        WorktreeId::from_usize(0),
        RelPath::new(
            std::path::Path::new("child1"),
            util::paths::PathStyle::Posix,
        )
        .unwrap()
        .into_arc(),
    );

    let local_2 = (WorktreeId::from_usize(1), RelPath::empty_arc());
    let local_2_child = (
        WorktreeId::from_usize(1),
        RelPath::new(
            std::path::Path::new("child2"),
            util::paths::PathStyle::Posix,
        )
        .unwrap()
        .into_arc(),
    );

    fn get(content: &SettingsContent) -> Option<&u32> {
        content
            .project
            .all_languages
            .defaults
            .preferred_line_length
            .as_ref()
    }

    store
        .set_local_settings(
            local_1.0,
            LocalSettingsPath::InWorktree(local_1.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{"preferred_line_length": 1}"#),
            cx,
        )
        .unwrap();
    store
        .set_local_settings(
            local_1_child.0,
            LocalSettingsPath::InWorktree(local_1_child.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{}"#),
            cx,
        )
        .unwrap();
    store
        .set_local_settings(
            local_2.0,
            LocalSettingsPath::InWorktree(local_2.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{"preferred_line_length": 2}"#),
            cx,
        )
        .unwrap();
    store
        .set_local_settings(
            local_2_child.0,
            LocalSettingsPath::InWorktree(local_2_child.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{}"#),
            cx,
        )
        .unwrap();

    // each local child should only inherit from it's parent
    assert_eq!(
        store.get_value_from_file(SettingsFile::Project(local_2_child), get),
        (SettingsFile::Project(local_2), Some(&2))
    );
    assert_eq!(
        store.get_value_from_file(SettingsFile::Project(local_1_child.clone()), get),
        (SettingsFile::Project(local_1.clone()), Some(&1))
    );

    // adjacent children should be treated as siblings not inherit from each other
    let local_1_adjacent_child = (local_1.0, rel_path("adjacent_child").into_arc());
    store
        .set_local_settings(
            local_1_adjacent_child.0,
            LocalSettingsPath::InWorktree(local_1_adjacent_child.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{}"#),
            cx,
        )
        .unwrap();
    store
        .set_local_settings(
            local_1_child.0,
            LocalSettingsPath::InWorktree(local_1_child.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{"preferred_line_length": 3}"#),
            cx,
        )
        .unwrap();

    assert_eq!(
        store.get_value_from_file(SettingsFile::Project(local_1_adjacent_child.clone()), get),
        (SettingsFile::Project(local_1.clone()), Some(&1))
    );
    store
        .set_local_settings(
            local_1_adjacent_child.0,
            LocalSettingsPath::InWorktree(local_1_adjacent_child.1),
            LocalSettingsKind::Settings,
            Some(r#"{"preferred_line_length": 3}"#),
            cx,
        )
        .unwrap();
    store
        .set_local_settings(
            local_1_child.0,
            LocalSettingsPath::InWorktree(local_1_child.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{}"#),
            cx,
        )
        .unwrap();
    assert_eq!(
        store.get_value_from_file(SettingsFile::Project(local_1_child), get),
        (SettingsFile::Project(local_1), Some(&1))
    );
}

#[gpui::test]
fn test_get_overrides_for_field(cx: &mut App) {
    let mut store = SettingsStore::new(cx, &test_settings());
    store.register_setting::<DefaultLanguageSettings>();

    let wt0_root = (WorktreeId::from_usize(0), RelPath::empty_arc());
    let wt0_child1 = (WorktreeId::from_usize(0), rel_path("child1").into_arc());
    let wt0_child2 = (WorktreeId::from_usize(0), rel_path("child2").into_arc());

    let wt1_root = (WorktreeId::from_usize(1), RelPath::empty_arc());
    let wt1_subdir = (WorktreeId::from_usize(1), rel_path("subdir").into_arc());

    fn get(content: &SettingsContent) -> &Option<u32> {
        &content.project.all_languages.defaults.preferred_line_length
    }

    store
        .set_user_settings(r#"{"preferred_line_length": 100}"#, cx)
        .unwrap();

    store
        .set_local_settings(
            wt0_root.0,
            LocalSettingsPath::InWorktree(wt0_root.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{"preferred_line_length": 80}"#),
            cx,
        )
        .unwrap();
    store
        .set_local_settings(
            wt0_child1.0,
            LocalSettingsPath::InWorktree(wt0_child1.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{"preferred_line_length": 120}"#),
            cx,
        )
        .unwrap();
    store
        .set_local_settings(
            wt0_child2.0,
            LocalSettingsPath::InWorktree(wt0_child2.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{}"#),
            cx,
        )
        .unwrap();

    store
        .set_local_settings(
            wt1_root.0,
            LocalSettingsPath::InWorktree(wt1_root.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{"preferred_line_length": 90}"#),
            cx,
        )
        .unwrap();
    store
        .set_local_settings(
            wt1_subdir.0,
            LocalSettingsPath::InWorktree(wt1_subdir.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{}"#),
            cx,
        )
        .unwrap();

    let overrides = store.get_overrides_for_field(SettingsFile::Default, get);
    assert_eq!(
        overrides,
        vec![
            SettingsFile::User,
            SettingsFile::Project(wt0_root.clone()),
            SettingsFile::Project(wt0_child1.clone()),
            SettingsFile::Project(wt1_root.clone()),
        ]
    );

    let overrides = store.get_overrides_for_field(SettingsFile::User, get);
    assert_eq!(
        overrides,
        vec![
            SettingsFile::Project(wt0_root.clone()),
            SettingsFile::Project(wt0_child1.clone()),
            SettingsFile::Project(wt1_root.clone()),
        ]
    );

    let overrides = store.get_overrides_for_field(SettingsFile::Project(wt0_root), get);
    assert_eq!(overrides, vec![]);

    let overrides = store.get_overrides_for_field(SettingsFile::Project(wt0_child1.clone()), get);
    assert_eq!(overrides, vec![]);

    let overrides = store.get_overrides_for_field(SettingsFile::Project(wt0_child2), get);
    assert_eq!(overrides, vec![]);

    let overrides = store.get_overrides_for_field(SettingsFile::Project(wt1_root), get);
    assert_eq!(overrides, vec![]);

    let overrides = store.get_overrides_for_field(SettingsFile::Project(wt1_subdir), get);
    assert_eq!(overrides, vec![]);

    let wt0_deep_child = (
        WorktreeId::from_usize(0),
        rel_path("child1/subdir").into_arc(),
    );
    store
        .set_local_settings(
            wt0_deep_child.0,
            LocalSettingsPath::InWorktree(wt0_deep_child.1.clone()),
            LocalSettingsKind::Settings,
            Some(r#"{"preferred_line_length": 140}"#),
            cx,
        )
        .unwrap();

    let overrides = store.get_overrides_for_field(SettingsFile::Project(wt0_deep_child), get);
    assert_eq!(overrides, vec![]);

    let overrides = store.get_overrides_for_field(SettingsFile::Project(wt0_child1), get);
    assert_eq!(overrides, vec![]);
}
