use super::tests_common::*;
use super::*;

#[gpui::test]
fn test_default_settings_release_channel_overrides(cx: &mut App) {
    // The test deals with overrides and should ignore the other set-ups (Preview and Stable runs)
    if *release_channel::RELEASE_CHANNEL != release_channel::ReleaseChannel::Dev {
        return;
    }

    let mut defaults: serde_json::Value =
        crate::parse_json_with_comments(&default_settings()).unwrap();
    let root = defaults
        .as_object_mut()
        .expect("default settings must be a JSON object");
    root.insert("dev".into(), serde_json::json!({ "auto_update": false }));
    root.insert("stable".into(), serde_json::json!({ "auto_update": true }));
    let defaults_with_overrides = serde_json::to_string(&defaults).unwrap();

    let mut store = SettingsStore::new(cx, &defaults_with_overrides);
    store.register_setting::<AutoUpdateSetting>();

    assert_eq!(
        store.get::<AutoUpdateSetting>(None),
        &AutoUpdateSetting { auto_update: false },
        "dev override from default settings should apply",
    );
}

#[gpui::test]
fn test_settings_store_basic(cx: &mut App) {
    let mut store = SettingsStore::new(cx, &default_settings());
    store.register_setting::<AutoUpdateSetting>();
    store.register_setting::<ItemSettings>();
    store.register_setting::<DefaultLanguageSettings>();

    assert_eq!(
        store.get::<AutoUpdateSetting>(None),
        &AutoUpdateSetting { auto_update: true }
    );
    assert_eq!(
        store.get::<ItemSettings>(None).close_position,
        ClosePosition::Right
    );

    store
        .set_user_settings(
            r#"{
                    "auto_update": false,
                    "tabs": {
                      "close_position": "left"
                    }
                }"#,
            cx,
        )
        .unwrap();

    assert_eq!(
        store.get::<AutoUpdateSetting>(None),
        &AutoUpdateSetting { auto_update: false }
    );
    assert_eq!(
        store.get::<ItemSettings>(None).close_position,
        ClosePosition::Left
    );

    store
        .set_local_settings(
            WorktreeId::from_usize(1),
            LocalSettingsPath::InWorktree(rel_path("root1").into()),
            LocalSettingsKind::Settings,
            Some(r#"{ "tab_size": 5 }"#),
            cx,
        )
        .unwrap();
    store
        .set_local_settings(
            WorktreeId::from_usize(1),
            LocalSettingsPath::InWorktree(rel_path("root1/subdir").into()),
            LocalSettingsKind::Settings,
            Some(r#"{ "preferred_line_length": 50 }"#),
            cx,
        )
        .unwrap();

    store
        .set_local_settings(
            WorktreeId::from_usize(1),
            LocalSettingsPath::InWorktree(rel_path("root2").into()),
            LocalSettingsKind::Settings,
            Some(r#"{ "tab_size": 9, "auto_update": true}"#),
            cx,
        )
        .unwrap();

    assert_eq!(
        store.get::<DefaultLanguageSettings>(Some(SettingsLocation {
            worktree_id: WorktreeId::from_usize(1),
            path: rel_path("root1/something"),
        })),
        &DefaultLanguageSettings {
            preferred_line_length: 80,
            tab_size: 5.try_into().unwrap(),
        }
    );
    assert_eq!(
        store.get::<DefaultLanguageSettings>(Some(SettingsLocation {
            worktree_id: WorktreeId::from_usize(1),
            path: rel_path("root1/subdir/something"),
        })),
        &DefaultLanguageSettings {
            preferred_line_length: 50,
            tab_size: 5.try_into().unwrap(),
        }
    );
    assert_eq!(
        store.get::<DefaultLanguageSettings>(Some(SettingsLocation {
            worktree_id: WorktreeId::from_usize(1),
            path: rel_path("root2/something"),
        })),
        &DefaultLanguageSettings {
            preferred_line_length: 80,
            tab_size: 9.try_into().unwrap(),
        }
    );
    assert_eq!(
        store.get::<AutoUpdateSetting>(Some(SettingsLocation {
            worktree_id: WorktreeId::from_usize(1),
            path: rel_path("root2/something")
        })),
        &AutoUpdateSetting { auto_update: false }
    );
}

#[gpui::test]
fn test_setting_store_assign_json_before_register(cx: &mut App) {
    let mut store = SettingsStore::new(cx, &test_settings());
    store
        .set_user_settings(r#"{ "auto_update": false }"#, cx)
        .unwrap();
    store.register_setting::<AutoUpdateSetting>();

    assert_eq!(
        store.get::<AutoUpdateSetting>(None),
        &AutoUpdateSetting { auto_update: false }
    );
}
