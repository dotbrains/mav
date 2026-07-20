use super::*;

#[gpui::test]
async fn test_pane_close_active_item(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| TestPanel::new(DockPosition::Right, 100, cx));
        workspace.add_panel(panel.clone(), window, cx);

        workspace
            .right_dock()
            .update(cx, |right_dock, cx| right_dock.set_open(true, window, cx));

        panel
    });

    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    let item_a = cx.new(TestItem::new);
    let item_b = cx.new(TestItem::new);
    let item_a_id = item_a.entity_id();
    let item_b_id = item_b.entity_id();

    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(Box::new(item_a.clone()), true, true, None, window, cx);
        pane.add_item(Box::new(item_b.clone()), true, true, None, window, cx);
    });

    pane.read_with(cx, |pane, _| {
        assert_eq!(pane.items_len(), 2);
        assert_eq!(pane.active_item().unwrap().item_id(), item_b_id);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_panel_focus::<TestPanel>(window, cx);
    });

    workspace.update_in(cx, |_, window, cx| {
        assert!(panel.read(cx).focus_handle(cx).contains_focused(window, cx));
    });

    // Assert that the `pane::CloseActiveItem` action is handled at the
    // workspace level when one of the dock panels is focused and, in that
    // case, the center pane's active item is closed but the focus is not
    // moved.
    cx.dispatch_action(pane::CloseActiveItem::default());
    cx.run_until_parked();

    pane.read_with(cx, |pane, _| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(pane.active_item().unwrap().item_id(), item_a_id);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(workspace.right_dock().read(cx).is_open());
        assert!(panel.read(cx).focus_handle(cx).contains_focused(window, cx));
    });
}

#[gpui::test]
async fn test_panel_zoom_preserved_across_workspace_switch(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project_a = Project::test(fs.clone(), [], cx).await;
    let project_b = Project::test(fs, [], cx).await;

    let multi_workspace_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    cx.run_until_parked();

    multi_workspace_handle
        .update(cx, |mw, _window, cx| {
            mw.open_sidebar(cx);
        })
        .unwrap();

    let workspace_a = multi_workspace_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let _workspace_b = multi_workspace_handle
        .update(cx, |mw, window, cx| {
            mw.test_add_workspace(project_b, window, cx)
        })
        .unwrap();

    // Switch to workspace A
    multi_workspace_handle
        .update(cx, |mw, window, cx| {
            let workspace = mw.workspaces().next().unwrap().clone();
            mw.activate(workspace, None, window, cx);
        })
        .unwrap();

    let cx = &mut VisualTestContext::from_window(multi_workspace_handle.into(), cx);

    // Add a panel to workspace A's right dock and open the dock
    let panel = workspace_a.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| TestPanel::new(DockPosition::Right, 100, cx));
        workspace.add_panel(panel.clone(), window, cx);
        workspace
            .right_dock()
            .update(cx, |dock, cx| dock.set_open(true, window, cx));
        panel
    });

    // Focus the panel through the workspace (matching existing test pattern)
    workspace_a.update_in(cx, |workspace, window, cx| {
        workspace.toggle_panel_focus::<TestPanel>(window, cx);
    });

    // Zoom the panel
    panel.update_in(cx, |panel, window, cx| {
        panel.set_zoomed(true, window, cx);
    });

    // Verify the panel is zoomed and the dock is open
    workspace_a.update_in(cx, |workspace, window, cx| {
        assert!(
            workspace.right_dock().read(cx).is_open(),
            "dock should be open before switch"
        );
        assert!(
            panel.is_zoomed(window, cx),
            "panel should be zoomed before switch"
        );
        assert!(
            panel.read(cx).focus_handle(cx).contains_focused(window, cx),
            "panel should be focused before switch"
        );
    });

    // Switch to workspace B
    multi_workspace_handle
        .update(cx, |mw, window, cx| {
            let workspace = mw.workspaces().nth(1).unwrap().clone();
            mw.activate(workspace, None, window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    // Switch back to workspace A
    multi_workspace_handle
        .update(cx, |mw, window, cx| {
            let workspace = mw.workspaces().next().unwrap().clone();
            mw.activate(workspace, None, window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    // Verify the panel is still zoomed and the dock is still open
    workspace_a.update_in(cx, |workspace, window, cx| {
        assert!(
            workspace.right_dock().read(cx).is_open(),
            "dock should still be open after switching back"
        );
        assert!(
            panel.is_zoomed(window, cx),
            "panel should still be zoomed after switching back"
        );
    });
}

#[gpui::test]
async fn test_window_title_follows_active_workspace(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root1"), json!({ "a.txt": "" }))
        .await;
    fs.insert_tree(path!("/root2"), json!({ "b.txt": "" }))
        .await;

    let project_a = Project::test(fs.clone(), [path!("/root1").as_ref()], cx).await;
    let project_b = Project::test(fs, [path!("/root2").as_ref()], cx).await;

    let multi_workspace_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let workspace_a = multi_workspace_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let cx = &mut VisualTestContext::from_window(multi_workspace_handle.into(), cx);
    cx.run_until_parked();
    assert_eq!(cx.window_title().as_deref(), Some("root1"));

    // Activating a second workspace must update the shared window's title.
    multi_workspace_handle
        .update(cx, |mw, window, cx| {
            mw.test_add_workspace(project_b.clone(), window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.window_title().as_deref(), Some("root2"));

    // Switching back must update the title again, even though workspace A's
    // own computed title hasn't changed since it was last active. This is
    // the regression: the per-workspace title cache was stale relative to
    // the shared window title.
    multi_workspace_handle
        .update(cx, |mw, window, cx| {
            mw.activate(workspace_a.clone(), None, window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.window_title().as_deref(), Some("root1"));
}

#[gpui::test]
async fn test_background_workspace_does_not_clobber_window_title(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root1"), json!({ "a.txt": "" }))
        .await;
    fs.insert_tree(path!("/root2"), json!({ "b.txt": "" }))
        .await;
    fs.insert_tree(path!("/root3"), json!({ "c.txt": "" }))
        .await;

    let project_a = Project::test(fs.clone(), [path!("/root1").as_ref()], cx).await;
    let project_b = Project::test(fs.clone(), [path!("/root2").as_ref()], cx).await;

    let multi_workspace_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

    let cx = &mut VisualTestContext::from_window(multi_workspace_handle.into(), cx);
    cx.run_until_parked();
    assert_eq!(cx.window_title().as_deref(), Some("root1"));

    // Switch to workspace B; workspace A becomes a background workspace whose
    // event subscriptions are still live.
    multi_workspace_handle
        .update(cx, |mw, window, cx| {
            mw.test_add_workspace(project_b.clone(), window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.window_title().as_deref(), Some("root2"));

    // A title-affecting change in the background workspace A must not touch
    // the shared window's title, which belongs to the active workspace B.
    project_a
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/root3"), true, cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.window_title().as_deref(), Some("root2"));
}

fn pane_items_paths(pane: &Entity<Pane>, cx: &App) -> Vec<String> {
    pane.read(cx)
        .items()
        .flat_map(|item| {
            item.project_paths(cx)
                .into_iter()
                .map(|path| path.path.display(PathStyle::local()).into_owned())
        })
        .collect()
}

pub fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        cx.set_global(db::AppDatabase::test_new());
        theme_settings::init(theme::LoadThemes::JustBase, cx);
    });
}

#[gpui::test]
async fn test_toggle_theme_mode_persists_and_updates_active_theme(cx: &mut TestAppContext) {
    use mav_actions::theme::ToggleMode;
    use settings::{ThemeName, ThemeSelection};
    use theme::SystemAppearance;

    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let settings_fs: Arc<dyn fs::Fs> = fs.clone();

    fs.insert_tree(path!("/root"), json!({ "file.rs": "fn main() {}\n" }))
        .await;

    // Build a test project and workspace view so the test can invoke
    // the workspace action handler the same way the UI would.
    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    // Seed the settings file with a plain static light theme so the
    // first toggle always starts from a known persisted state.
    workspace.update_in(cx, |_workspace, _window, cx| {
        *SystemAppearance::global_mut(cx) = SystemAppearance(theme::Appearance::Light);
        settings::update_settings_file(settings_fs.clone(), cx, |settings, _cx| {
            settings.theme.theme = Some(ThemeSelection::Static(ThemeName("One Light".into())));
        });
    });
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();

    // Confirm the initial persisted settings contain the static theme
    // we just wrote before any toggling happens.
    let settings_text = SettingsStore::load_settings(&settings_fs).await.unwrap();
    assert!(settings_text.contains(r#""theme": "One Light""#));

    // Toggle once. This should migrate the persisted theme settings
    // into light/dark slots and enable system mode.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_theme_mode(&ToggleMode, window, cx);
    });
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();

    // 1. Static -> Dynamic
    // this assertion checks theme changed from static to dynamic.
    let settings_text = SettingsStore::load_settings(&settings_fs).await.unwrap();
    let parsed: serde_json::Value = settings::parse_json_with_comments(&settings_text).unwrap();
    assert_eq!(
        parsed["theme"],
        serde_json::json!({
            "mode": "system",
            "light": "One Light",
            "dark": "One Dark"
        })
    );

    // 2. Toggle again, suppose it will change the mode to light
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_theme_mode(&ToggleMode, window, cx);
    });
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();

    let settings_text = SettingsStore::load_settings(&settings_fs).await.unwrap();
    assert!(settings_text.contains(r#""mode": "light""#));
}

fn dirty_project_item(id: u64, path: &str, cx: &mut App) -> Entity<TestProjectItem> {
    let item = TestProjectItem::new(id, path, cx);
    item.update(cx, |item, _| {
        item.is_dirty = true;
    });
    item
}

fn new_test_project_item(
    id: u64,
    path: &str,
    worktree_id: WorktreeId,
    cx: &mut App,
) -> Entity<TestProjectItem> {
    let item = TestProjectItem::new(id, path, cx);
    item.update(cx, |item, _| {
        if let Some(ref mut project_path) = item.project_path {
            project_path.worktree_id = worktree_id;
        }
    });
    item
}

#[gpui::test]
async fn test_zoomed_panel_without_pane_preserved_on_center_focus(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| TestPanel::new(DockPosition::Right, 100, cx));
        workspace.add_panel(panel.clone(), window, cx);
        workspace
            .right_dock()
            .update(cx, |dock, cx| dock.set_open(true, window, cx));
        panel
    });

    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    pane.update_in(cx, |pane, window, cx| {
        let item = cx.new(TestItem::new);
        pane.add_item(Box::new(item), true, true, None, window, cx);
    });

    // Transfer focus to the panel, then zoom it. Using toggle_panel_focus
    // mirrors the real-world flow and avoids side effects from directly
    // focusing the panel while the center pane is active.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_panel_focus::<TestPanel>(window, cx);
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.set_zoomed(true, window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(workspace.right_dock().read(cx).is_open());
        assert!(panel.is_zoomed(window, cx));
        assert!(panel.read(cx).focus_handle(cx).contains_focused(window, cx));
    });

    // Simulate a spurious pane::Event::Focus on the center pane while the
    // panel still has focus. This mirrors what happens during macOS window
    // activation: the center pane fires a focus event even though actual
    // focus remains on the dock panel.
    pane.update_in(cx, |_, _, cx| {
        cx.emit(pane::Event::Focus);
    });

    // The dock must remain open because the panel had focus at the time the
    // event was processed. Before the fix, dock_to_preserve was None for
    // panels that don't implement pane(), causing the dock to close.
    workspace.update_in(cx, |workspace, window, cx| {
        assert!(
            workspace.right_dock().read(cx).is_open(),
            "Dock should stay open when its zoomed panel (without pane()) still has focus"
        );
        assert!(panel.is_zoomed(window, cx));
    });
}

#[gpui::test]
async fn test_panels_stay_open_after_position_change_and_settings_update(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    // Add two panels to the left dock and open it.
    let (panel_a, panel_b) = workspace.update_in(cx, |workspace, window, cx| {
        let panel_a = cx.new(|cx| TestPanel::new(DockPosition::Left, 100, cx));
        let panel_b = cx.new(|cx| TestPanel::new(DockPosition::Left, 101, cx));
        workspace.add_panel(panel_a.clone(), window, cx);
        workspace.add_panel(panel_b.clone(), window, cx);
        workspace.left_dock().update(cx, |dock, cx| {
            dock.set_open(true, window, cx);
            dock.activate_panel(0, window, cx);
        });
        (panel_a, panel_b)
    });

    workspace.update_in(cx, |workspace, _, cx| {
        assert!(workspace.left_dock().read(cx).is_open());
    });

    // Simulate a feature flag changing default dock positions: both panels
    // move from Left to Right.
    workspace.update_in(cx, |_workspace, _window, cx| {
        panel_a.update(cx, |p, _cx| p.position = DockPosition::Right);
        panel_b.update(cx, |p, _cx| p.position = DockPosition::Right);
        cx.update_global::<SettingsStore, _>(|_, _| {});
    });

    // Both panels should now be in the right dock.
    workspace.update_in(cx, |workspace, _, cx| {
        let right_dock = workspace.right_dock().read(cx);
        assert_eq!(right_dock.panels_len(), 2);
    });

    // Open the right dock and activate panel_b (simulating the user
    // opening the panel after it moved).
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.right_dock().update(cx, |dock, cx| {
            dock.set_open(true, window, cx);
            dock.activate_panel(1, window, cx);
        });
    });

    // Now trigger another SettingsStore change
    workspace.update_in(cx, |_workspace, _window, cx| {
        cx.update_global::<SettingsStore, _>(|_, _| {});
    });

    workspace.update_in(cx, |workspace, _, cx| {
        assert!(
            workspace.right_dock().read(cx).is_open(),
            "Right dock should still be open after a settings change"
        );
        assert_eq!(
            workspace.right_dock().read(cx).panels_len(),
            2,
            "Both panels should still be in the right dock"
        );
    });
}
