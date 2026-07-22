use gpui::{TestAppContext, UpdateGlobal};
use settings::{DockPosition, DockSide, Settings, SettingsStore};

use crate::{AgentSettings, PanelLayout, WindowLayout};

#[gpui::test]
fn test_get_layout(cx: &mut gpui::App) {
    let store = SettingsStore::test(cx);
    cx.set_global(store);
    project::DisableAiSettings::register(cx);
    AgentSettings::register(cx);

    // Should be Agent with an empty user layout (user hasn't customized).
    let layout = AgentSettings::get_layout(cx);
    let WindowLayout::Agent(Some(user_layout)) = layout else {
        panic!("expected Agent(Some), got {:?}", layout);
    };
    assert_eq!(user_layout, PanelLayout::default());

    // User explicitly sets agent dock to left (matching the default).
    // The merged result is still agent, but the user layout captures
    // only what the user wrote.
    SettingsStore::update_global(cx, |store, cx| {
        store
            .set_user_settings(r#"{ "agent": { "dock": "left" } }"#, cx)
            .unwrap();
    });

    let layout = AgentSettings::get_layout(cx);
    let WindowLayout::Agent(Some(user_layout)) = layout else {
        panic!("expected Agent(Some), got {:?}", layout);
    };
    assert_eq!(user_layout.agent_dock, Some(DockPosition::Left));
    assert_eq!(user_layout.project_panel_dock, None);
    assert_eq!(user_layout.outline_panel_dock, None);
    assert_eq!(user_layout.collaboration_panel_dock, None);
    assert_eq!(user_layout.git_panel_dock, None);

    // User sets a combination that doesn't match either preset:
    // agent on the left but project panel also on the left.
    SettingsStore::update_global(cx, |store, cx| {
        store
            .set_user_settings(
                r#"{
                    "agent": { "dock": "left" },
                    "project_panel": { "dock": "left" }
                }"#,
                cx,
            )
            .unwrap();
    });

    let layout = AgentSettings::get_layout(cx);
    let WindowLayout::Custom(user_layout) = layout else {
        panic!("expected Custom, got {:?}", layout);
    };
    assert_eq!(user_layout.agent_dock, Some(DockPosition::Left));
    assert_eq!(user_layout.project_panel_dock, Some(DockSide::Left));
}

#[gpui::test]
fn test_set_layout_round_trip(cx: &mut gpui::App) {
    let store = SettingsStore::test(cx);
    cx.set_global(store);
    project::DisableAiSettings::register(cx);
    AgentSettings::register(cx);

    // User has a custom layout: agent on the right with project panel
    // also on the right. This doesn't match either preset.
    SettingsStore::update_global(cx, |store, cx| {
        store
            .set_user_settings(
                r#"{
                    "agent": { "dock": "right" },
                    "project_panel": { "dock": "right" }
                }"#,
                cx,
            )
            .unwrap();
    });

    let original = AgentSettings::get_layout(cx);
    let WindowLayout::Custom(ref original_user_layout) = original else {
        panic!("expected Custom, got {:?}", original);
    };
    assert_eq!(original_user_layout.agent_dock, Some(DockPosition::Right));
    assert_eq!(
        original_user_layout.project_panel_dock,
        Some(DockSide::Right)
    );
    assert_eq!(original_user_layout.outline_panel_dock, None);

    // Switch to the agent layout. This overwrites the user settings.
    SettingsStore::update_global(cx, |store, cx| {
        store.update_user_settings(cx, |settings| {
            PanelLayout::AGENT.write_to(settings);
        });
    });

    let layout = AgentSettings::get_layout(cx);
    assert!(matches!(layout, WindowLayout::Agent(_)));

    // Restore the original custom layout.
    SettingsStore::update_global(cx, |store, cx| {
        store.update_user_settings(cx, |settings| {
            original_user_layout.write_to(settings);
        });
    });

    // Should be back to the same custom layout.
    let restored = AgentSettings::get_layout(cx);
    let WindowLayout::Custom(restored_user_layout) = restored else {
        panic!("expected Custom, got {:?}", restored);
    };
    assert_eq!(restored_user_layout.agent_dock, Some(DockPosition::Right));
    assert_eq!(
        restored_user_layout.project_panel_dock,
        Some(DockSide::Right)
    );
    assert_eq!(restored_user_layout.outline_panel_dock, None);
}

#[gpui::test]
async fn test_set_layout_minimal_diff(cx: &mut TestAppContext) {
    let fs = fs::FakeFs::new(cx.background_executor.clone());
    fs.save(
        paths::settings_file().as_path(),
        &serde_json::json!({
            "agent": { "dock": "left" },
            "project_panel": { "dock": "left" }
        })
        .to_string()
        .into(),
        Default::default(),
    )
    .await
    .unwrap();

    cx.update(|cx| {
        let store = SettingsStore::test(cx);
        cx.set_global(store);
        project::DisableAiSettings::register(cx);
        AgentSettings::register(cx);

        // User has agent=left (matches preset) and project_panel=left (does not)
        SettingsStore::update_global(cx, |store, cx| {
            store
                .set_user_settings(
                    r#"{
                        "agent": { "dock": "left" },
                        "project_panel": { "dock": "left" }
                    }"#,
                    cx,
                )
                .unwrap();
        });

        let layout = AgentSettings::get_layout(cx);
        assert!(matches!(layout, WindowLayout::Custom(_)));

        AgentSettings::set_layout(WindowLayout::agent(), fs.clone(), cx)
    })
    .await
    .ok();

    cx.run_until_parked();

    let written = fs.load(paths::settings_file().as_path()).await.unwrap();
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.set_user_settings(&written, cx).unwrap();
        });

        // The user settings should still have agent=left (preserved)
        // and now project_panel=right (changed to match preset).
        let store = cx.global::<SettingsStore>();
        let user_layout = store
            .raw_user_settings()
            .map(|u| PanelLayout::read_from(u.content.as_ref()))
            .unwrap_or_default();

        assert_eq!(user_layout.agent_dock, Some(DockPosition::Left));
        assert_eq!(user_layout.project_panel_dock, Some(DockSide::Right));
        // Other fields weren't in user settings and didn't need changing.
        assert_eq!(user_layout.outline_panel_dock, None);

        // And the merged result should now match agent.
        let layout = AgentSettings::get_layout(cx);
        assert!(matches!(layout, WindowLayout::Agent(_)));
    });
}

#[gpui::test]
async fn test_backfill_editor_layout(cx: &mut TestAppContext) {
    let fs = fs::FakeFs::new(cx.background_executor.clone());
    // User has only customized project_panel to "right".
    fs.save(
        paths::settings_file().as_path(),
        &serde_json::json!({
            "project_panel": { "dock": "right" }
        })
        .to_string()
        .into(),
        Default::default(),
    )
    .await
    .unwrap();

    cx.update(|cx| {
        let store = SettingsStore::test(cx);
        cx.set_global(store);
        project::DisableAiSettings::register(cx);
        AgentSettings::register(cx);

        // Simulate pre-migration state: editor defaults (the old world).
        SettingsStore::update_global(cx, |store, cx| {
            store.update_default_settings(cx, |defaults| {
                PanelLayout::EDITOR.write_to(defaults);
            });
        });

        // User has only customized project_panel to "right".
        SettingsStore::update_global(cx, |store, cx| {
            store
                .set_user_settings(r#"{ "project_panel": { "dock": "right" } }"#, cx)
                .unwrap();
        });

        // Run the one-time backfill while still on old defaults.
        AgentSettings::backfill_editor_layout(fs.clone(), cx);
    });

    cx.run_until_parked();

    // Read back the file and apply it.
    let written = fs.load(paths::settings_file().as_path()).await.unwrap();
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.set_user_settings(&written, cx).unwrap();
        });

        // The user's project_panel=right should be preserved (they set it).
        // All other fields should now have the editor preset values
        // written into user settings.
        let store = cx.global::<SettingsStore>();
        let user_layout = store
            .raw_user_settings()
            .map(|u| PanelLayout::read_from(u.content.as_ref()))
            .unwrap_or_default();

        assert_eq!(user_layout.agent_dock, Some(DockPosition::Right));
        assert_eq!(user_layout.project_panel_dock, Some(DockSide::Right));
        assert_eq!(user_layout.outline_panel_dock, Some(DockSide::Left));
        assert_eq!(
            user_layout.collaboration_panel_dock,
            Some(DockPosition::Left)
        );
        assert_eq!(user_layout.git_panel_dock, Some(DockPosition::Left));

        // Even though defaults are now agent, the backfilled user settings
        // keep everything in the editor layout. The user's experience
        // hasn't changed.
        let layout = AgentSettings::get_layout(cx);
        let WindowLayout::Custom(user_layout) = layout else {
            panic!(
                "expected Custom (editor values override agent defaults), got {:?}",
                layout
            );
        };
        assert_eq!(user_layout.agent_dock, Some(DockPosition::Right));
        assert_eq!(user_layout.project_panel_dock, Some(DockSide::Right));
    });
}
