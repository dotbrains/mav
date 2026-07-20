mod disable_ai_settings_tests {
    use gpui::TestAppContext;
    use project::*;
    use settings::{Settings, SettingsStore};

    #[gpui::test]
    async fn test_disable_ai_settings_security(cx: &mut TestAppContext) {
        cx.update(|cx| {
            settings::init(cx);

            // Test 1: Default is false (AI enabled)
            assert!(
                !DisableAiSettings::get_global(cx).disable_ai,
                "Default should allow AI"
            );
        });

        let disable_true = serde_json::json!({
            "disable_ai": true
        })
        .to_string();
        let disable_false = serde_json::json!({
            "disable_ai": false
        })
        .to_string();

        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.set_user_settings(&disable_false, cx).unwrap();
            store.set_global_settings(&disable_true, cx).unwrap();
        });
        cx.update(|cx| {
            assert!(
                DisableAiSettings::get_global(cx).disable_ai,
                "Local false cannot override global true"
            );
        });

        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.set_global_settings(&disable_false, cx).unwrap();
            store.set_user_settings(&disable_true, cx).unwrap();
        });

        cx.update(|cx| {
            assert!(
                DisableAiSettings::get_global(cx).disable_ai,
                "Local false cannot override global true"
            );
        });
    }

    #[gpui::test]
    async fn test_disable_ai_project_level_settings(cx: &mut TestAppContext) {
        use settings::{LocalSettingsKind, LocalSettingsPath, SettingsLocation, SettingsStore};
        use worktree::WorktreeId;

        cx.update(|cx| {
            settings::init(cx);

            // Default should allow AI
            assert!(
                !DisableAiSettings::get_global(cx).disable_ai,
                "Default should allow AI"
            );
        });

        let worktree_id = WorktreeId::from_usize(1);
        let rel_path = |path: &str| -> std::sync::Arc<util::rel_path::RelPath> {
            std::sync::Arc::from(util::rel_path::RelPath::unix(path).unwrap())
        };
        let project_path = rel_path("project");
        let settings_location = SettingsLocation {
            worktree_id,
            path: project_path.as_ref(),
        };

        // Test: Project-level disable_ai=true should disable AI for files in that project
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store
                .set_local_settings(
                    worktree_id,
                    LocalSettingsPath::InWorktree(project_path.clone()),
                    LocalSettingsKind::Settings,
                    Some(r#"{ "disable_ai": true }"#),
                    cx,
                )
                .unwrap();
        });

        cx.update(|cx| {
            let settings = DisableAiSettings::get(Some(settings_location), cx);
            assert!(
                settings.disable_ai,
                "Project-level disable_ai=true should disable AI for files in that project"
            );
            // Global should now also be true since project-level disable_ai is merged into global
            assert!(
                DisableAiSettings::get_global(cx).disable_ai,
                "Global setting should be affected by project-level disable_ai=true"
            );
        });

        // Test: Setting project-level to false should allow AI for that project
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store
                .set_local_settings(
                    worktree_id,
                    LocalSettingsPath::InWorktree(project_path.clone()),
                    LocalSettingsKind::Settings,
                    Some(r#"{ "disable_ai": false }"#),
                    cx,
                )
                .unwrap();
        });

        cx.update(|cx| {
            let settings = DisableAiSettings::get(Some(settings_location), cx);
            assert!(
                !settings.disable_ai,
                "Project-level disable_ai=false should allow AI"
            );
            // Global should also be false now
            assert!(
                !DisableAiSettings::get_global(cx).disable_ai,
                "Global setting should be false when project-level is false"
            );
        });

        // Test: User-level true + project-level false = AI disabled (saturation)
        let disable_true = serde_json::json!({ "disable_ai": true }).to_string();
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.set_user_settings(&disable_true, cx).unwrap();
            store
                .set_local_settings(
                    worktree_id,
                    LocalSettingsPath::InWorktree(project_path.clone()),
                    LocalSettingsKind::Settings,
                    Some(r#"{ "disable_ai": false }"#),
                    cx,
                )
                .unwrap();
        });

        cx.update(|cx| {
            let settings = DisableAiSettings::get(Some(settings_location), cx);
            assert!(
                settings.disable_ai,
                "Project-level false cannot override user-level true (SaturatingBool)"
            );
        });
    }
}
