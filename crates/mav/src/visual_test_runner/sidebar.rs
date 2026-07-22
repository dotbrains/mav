use super::*;

#[cfg(target_os = "macos")]
fn run_sidebar_visual_tests(
    app_state: Arc<AppState>,
    cx: &mut VisualTestAppContext,
    update_baseline: bool,
) -> Result<TestResult> {
    // Create temporary directories to act as worktrees for active workspaces
    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.keep();
    let canonical_temp = temp_path.canonicalize()?;

    let workspace1_dir = canonical_temp.join("private-test-remote");
    let workspace2_dir = canonical_temp.join("mav");
    std::fs::create_dir_all(&workspace1_dir)?;
    std::fs::create_dir_all(&workspace2_dir)?;

    // Create both projects upfront so we can build both workspaces during
    // window creation, before the MultiWorkspace entity exists.
    // This avoids a re-entrant read panic that occurs when Workspace::new
    // tries to access the window root (MultiWorkspace) while it's being updated.
    let project1 = cx.update(|cx| {
        project::Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags {
                init_worktree_trust: false,
                ..Default::default()
            },
            cx,
        )
    });

    let project2 = cx.update(|cx| {
        project::Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags {
                init_worktree_trust: false,
                ..Default::default()
            },
            cx,
        )
    });

    let window_size = size(px(1280.0), px(800.0));
    let bounds = Bounds {
        origin: point(px(0.0), px(0.0)),
        size: window_size,
    };

    // Open a MultiWorkspace window with both workspaces created at construction time
    let multi_workspace_window: WindowHandle<MultiWorkspace> = cx
        .update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    focus: false,
                    show: false,
                    ..Default::default()
                },
                |window, cx| {
                    let workspace1 = cx.new(|cx| {
                        Workspace::new(None, project1.clone(), app_state.clone(), window, cx)
                    });
                    let workspace2 = cx.new(|cx| {
                        Workspace::new(None, project2.clone(), app_state.clone(), window, cx)
                    });
                    cx.new(|cx| {
                        let mut multi_workspace = MultiWorkspace::new(workspace1, window, cx);
                        multi_workspace.activate(workspace2, None, window, cx);
                        multi_workspace
                    })
                },
            )
        })
        .context("Failed to open MultiWorkspace window")?;

    cx.run_until_parked();

    // Add worktree to workspace 1 (index 0) so it shows as "private-test-remote"
    let add_worktree1_task = multi_workspace_window
        .update(cx, |multi_workspace, _window, cx| {
            let workspace1 = multi_workspace.workspaces().next().unwrap();
            let project = workspace1.read(cx).project().clone();
            project.update(cx, |project, cx| {
                project.find_or_create_worktree(&workspace1_dir, true, cx)
            })
        })
        .context("Failed to start adding worktree 1")?;

    cx.background_executor.allow_parking();
    cx.foreground_executor
        .block_test(add_worktree1_task)
        .context("Failed to add worktree 1")?;
    cx.background_executor.forbid_parking();

    cx.run_until_parked();

    // Add worktree to workspace 2 (index 1) so it shows as "mav"
    let add_worktree2_task = multi_workspace_window
        .update(cx, |multi_workspace, _window, cx| {
            let workspace2 = multi_workspace.workspaces().nth(1).unwrap();
            let project = workspace2.read(cx).project().clone();
            project.update(cx, |project, cx| {
                project.find_or_create_worktree(&workspace2_dir, true, cx)
            })
        })
        .context("Failed to start adding worktree 2")?;

    cx.background_executor.allow_parking();
    cx.foreground_executor
        .block_test(add_worktree2_task)
        .context("Failed to add worktree 2")?;
    cx.background_executor.forbid_parking();

    cx.run_until_parked();

    // Switch to workspace 1 so it's highlighted as active (index 0)
    multi_workspace_window
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspaces().next().unwrap().clone();
            multi_workspace.activate(workspace, None, window, cx);
        })
        .context("Failed to activate workspace 1")?;

    cx.run_until_parked();

    // Create the sidebar outside the MultiWorkspace update to avoid a
    // re-entrant read panic (Sidebar::new reads the MultiWorkspace).
    let sidebar = cx
        .update_window(multi_workspace_window.into(), |root_view, window, cx| {
            let multi_workspace_handle: Entity<MultiWorkspace> = root_view.downcast().unwrap();
            cx.new(|cx| sidebar::Sidebar::new(multi_workspace_handle, window, cx))
        })
        .context("Failed to create sidebar")?;

    multi_workspace_window
        .update(cx, |multi_workspace, _window, cx| {
            multi_workspace.register_sidebar(sidebar.clone(), window, cx);
        })
        .context("Failed to register sidebar")?;

    cx.run_until_parked();

    // Save test threads to the ThreadStore for each workspace
    let save_tasks = multi_workspace_window
        .update(cx, |multi_workspace, _window, cx| {
            let thread_store = agent::ThreadStore::global(cx);
            let workspaces: Vec<_> = multi_workspace.workspaces().cloned().collect();
            let mut tasks = Vec::new();

            for (index, workspace) in workspaces.iter().enumerate() {
                let workspace_ref = workspace.read(cx);
                let mut paths = Vec::new();
                for worktree in workspace_ref.worktrees(cx) {
                    let worktree_ref = worktree.read(cx);
                    if worktree_ref.is_visible() {
                        paths.push(worktree_ref.abs_path().to_path_buf());
                    }
                }
                let path_list = util::path_list::PathList::new(&paths);

                let (session_id, title, updated_at) = match index {
                    0 => (
                        "visual-test-thread-0",
                        "Refine thread view scrolling behavior",
                        chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 6, 15, 10, 30, 0)
                            .unwrap(),
                    ),
                    1 => (
                        "visual-test-thread-1",
                        "Add line numbers option to FileEditBlock",
                        chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 6, 15, 11, 0, 0)
                            .unwrap(),
                    ),
                    _ => continue,
                };

                let task = thread_store.update(cx, |store, cx| {
                    store.save_thread(
                        acp::SessionId::new(Arc::from(session_id)),
                        agent::DbThread {
                            title: title.to_string().into(),
                            messages: Vec::new(),
                            updated_at,
                            detailed_summary: None,
                            initial_project_snapshot: None,
                            cumulative_token_usage: Default::default(),
                            request_token_usage: Default::default(),
                            model: None,
                            profile: None,
                            subagent_context: None,
                            speed: None,
                            thinking_enabled: false,
                            thinking_effort: None,
                            ui_scroll_position: None,
                            draft_prompt: None,
                            sandboxed_terminal_temp_dir: None,
                            sandbox_grants: Default::default(),
                        },
                        path_list,
                        cx,
                    )
                });
                tasks.push(task);
            }
            tasks
        })
        .context("Failed to create test threads")?;

    cx.background_executor.allow_parking();
    for task in save_tasks {
        cx.foreground_executor
            .block_test(task)
            .context("Failed to save test thread")?;
    }
    cx.background_executor.forbid_parking();

    cx.run_until_parked();

    // Open the sidebar
    multi_workspace_window
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.toggle_sidebar(window, cx);
        })
        .context("Failed to toggle sidebar")?;

    // Let rendering settle
    for _ in 0..10 {
        cx.advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
    }

    // Refresh the window
    cx.update_window(multi_workspace_window.into(), |_, window, _cx| {
        window.refresh();
    })?;

    cx.run_until_parked();

    // Capture: sidebar open with active workspaces and recent projects
    let test_result = run_visual_test(
        "sidebar_open",
        multi_workspace_window.into(),
        cx,
        update_baseline,
    )?;

    // Clean up worktrees
    multi_workspace_window
        .update(cx, |multi_workspace, _window, cx| {
            for workspace in multi_workspace.workspaces() {
                let project = workspace.read(cx).project().clone();
                project.update(cx, |project, cx| {
                    let worktree_ids: Vec<_> =
                        project.worktrees(cx).map(|wt| wt.read(cx).id()).collect();
                    for id in worktree_ids {
                        project.remove_worktree(id, cx);
                    }
                });
            }
        })
        .log_err();

    cx.run_until_parked();

    // Close the window
    cx.update_window(multi_workspace_window.into(), |_, window, _cx| {
        window.remove_window();
    })
    .log_err();

    cx.run_until_parked();

    for _ in 0..15 {
        cx.advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
    }

    Ok(test_result)
}
