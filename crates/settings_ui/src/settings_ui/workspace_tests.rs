use super::*;

#[gpui::test]
async fn test_settings_window_shows_worktrees_from_multiple_workspaces(
    cx: &mut gpui::TestAppContext,
) {
    use project::Project;
    use serde_json::json;

    cx.update(|cx| {
        register_settings(cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = AppState::test(cx);
        AppState::set_global(app_state.clone(), cx);
        app_state
    });

    let fake_fs = app_state.fs.as_fake();

    fake_fs
        .insert_tree(
            "/workspace1",
            json!({
                "worktree_a": {
                    "file1.rs": "fn main() {}"
                },
                "worktree_b": {
                    "file2.rs": "fn test() {}"
                }
            }),
        )
        .await;

    fake_fs
        .insert_tree(
            "/workspace2",
            json!({
                "worktree_c": {
                    "file3.rs": "fn foo() {}"
                }
            }),
        )
        .await;

    let project1 = cx.update(|cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });

    project1
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/workspace1/worktree_a", true, cx)
        })
        .await
        .expect("Failed to create worktree_a");
    project1
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/workspace1/worktree_b", true, cx)
        })
        .await
        .expect("Failed to create worktree_b");

    let project2 = cx.update(|cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });

    project2
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/workspace2/worktree_c", true, cx)
        })
        .await
        .expect("Failed to create worktree_c");

    let (_multi_workspace1, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project1.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });

    let (_multi_workspace2, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project2.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });

    let workspace2_handle = cx.window_handle().downcast::<MultiWorkspace>().unwrap();

    cx.run_until_parked();

    let (settings_window, cx) =
        cx.add_window_view(|window, cx| SettingsWindow::new(Some(workspace2_handle), window, cx));

    cx.run_until_parked();

    settings_window.read_with(cx, |settings_window, _| {
        let worktree_names: Vec<_> = settings_window
            .worktree_root_dirs
            .values()
            .cloned()
            .collect();

        assert!(
            worktree_names.iter().any(|name| name == "worktree_a"),
            "Should contain worktree_a from workspace1, but found: {:?}",
            worktree_names
        );
        assert!(
            worktree_names.iter().any(|name| name == "worktree_b"),
            "Should contain worktree_b from workspace1, but found: {:?}",
            worktree_names
        );
        assert!(
            worktree_names.iter().any(|name| name == "worktree_c"),
            "Should contain worktree_c from workspace2, but found: {:?}",
            worktree_names
        );

        assert_eq!(
            worktree_names.len(),
            3,
            "Should have exactly 3 worktrees from both workspaces, but found: {:?}",
            worktree_names
        );

        let project_files: Vec<_> = settings_window
            .files
            .iter()
            .filter_map(|(f, _)| match f {
                SettingsUiFile::Project((worktree_id, _)) => Some(*worktree_id),
                _ => None,
            })
            .collect();

        let unique_project_files: std::collections::HashSet<_> = project_files.iter().collect();
        assert_eq!(
            project_files.len(),
            unique_project_files.len(),
            "Should have no duplicate project files, but found duplicates. All files: {:?}",
            project_files
        );
    });
}

#[gpui::test]
async fn test_settings_window_updates_when_new_workspace_created(cx: &mut gpui::TestAppContext) {
    use project::Project;
    use serde_json::json;

    cx.update(|cx| {
        register_settings(cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = AppState::test(cx);
        AppState::set_global(app_state.clone(), cx);
        app_state
    });

    let fake_fs = app_state.fs.as_fake();

    fake_fs
        .insert_tree(
            "/workspace1",
            json!({
                "worktree_a": {
                    "file1.rs": "fn main() {}"
                }
            }),
        )
        .await;

    fake_fs
        .insert_tree(
            "/workspace2",
            json!({
                "worktree_b": {
                    "file2.rs": "fn test() {}"
                }
            }),
        )
        .await;

    let project1 = cx.update(|cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });

    project1
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/workspace1/worktree_a", true, cx)
        })
        .await
        .expect("Failed to create worktree_a");

    let (_multi_workspace1, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project1.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });

    let workspace1_handle = cx.window_handle().downcast::<MultiWorkspace>().unwrap();

    cx.run_until_parked();

    let (settings_window, cx) =
        cx.add_window_view(|window, cx| SettingsWindow::new(Some(workspace1_handle), window, cx));

    cx.run_until_parked();

    settings_window.read_with(cx, |settings_window, _| {
        assert_eq!(
            settings_window.worktree_root_dirs.len(),
            1,
            "Should have 1 worktree initially"
        );
    });

    let project2 = cx.update(|_, cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });

    project2
        .update(&mut cx.cx, |project, cx| {
            project.find_or_create_worktree("/workspace2/worktree_b", true, cx)
        })
        .await
        .expect("Failed to create worktree_b");

    let (_multi_workspace2, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project2.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });

    cx.run_until_parked();

    settings_window.read_with(cx, |settings_window, _| {
        let worktree_names: Vec<_> = settings_window
            .worktree_root_dirs
            .values()
            .cloned()
            .collect();

        assert!(
            worktree_names.iter().any(|name| name == "worktree_a"),
            "Should contain worktree_a, but found: {:?}",
            worktree_names
        );
        assert!(
            worktree_names.iter().any(|name| name == "worktree_b"),
            "Should contain worktree_b from newly created workspace, but found: {:?}",
            worktree_names
        );

        assert_eq!(
            worktree_names.len(),
            2,
            "Should have 2 worktrees after new workspace created, but found: {:?}",
            worktree_names
        );

        let project_files: Vec<_> = settings_window
            .files
            .iter()
            .filter_map(|(f, _)| match f {
                SettingsUiFile::Project((worktree_id, _)) => Some(*worktree_id),
                _ => None,
            })
            .collect();

        let unique_project_files: std::collections::HashSet<_> = project_files.iter().collect();
        assert_eq!(
            project_files.len(),
            unique_project_files.len(),
            "Should have no duplicate project files, but found duplicates. All files: {:?}",
            project_files
        );
    });
}
