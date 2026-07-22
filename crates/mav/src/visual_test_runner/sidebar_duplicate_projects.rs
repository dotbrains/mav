use super::*;

#[cfg(target_os = "macos")]
/// Helper to create a project, add a worktree at the given path, and return the project.
fn create_project_with_worktree(
    worktree_dir: &Path,
    app_state: &Arc<AppState>,
    cx: &mut VisualTestAppContext,
) -> Result<Entity<Project>> {
    let project = cx.update(|cx| {
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

    let add_task = cx.update(|cx| {
        project.update(cx, |project, cx| {
            project.find_or_create_worktree(worktree_dir, true, cx)
        })
    });

    cx.background_executor.allow_parking();
    cx.foreground_executor
        .block_test(add_task)
        .context("Failed to add worktree")?;
    cx.background_executor.forbid_parking();

    cx.run_until_parked();
    Ok(project)
}

#[cfg(target_os = "macos")]
fn open_sidebar_test_window(
    projects: Vec<Entity<Project>>,
    app_state: &Arc<AppState>,
    cx: &mut VisualTestAppContext,
) -> Result<WindowHandle<MultiWorkspace>> {
    anyhow::ensure!(!projects.is_empty(), "need at least one project");

    let window_size = size(px(400.0), px(600.0));
    let bounds = Bounds {
        origin: point(px(0.0), px(0.0)),
        size: window_size,
    };

    let mut projects_iter = projects.into_iter();
    let first_project = projects_iter
        .next()
        .ok_or_else(|| anyhow::anyhow!("need at least one project"))?;
    let remaining: Vec<_> = projects_iter.collect();

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
                    let first_ws = cx.new(|cx| {
                        Workspace::new(None, first_project.clone(), app_state.clone(), window, cx)
                    });
                    cx.new(|cx| {
                        let mut mw = MultiWorkspace::new(first_ws, window, cx);
                        for project in remaining {
                            let ws = cx.new(|cx| {
                                Workspace::new(None, project, app_state.clone(), window, cx)
                            });
                            mw.activate(ws, None, window, cx);
                        }
                        mw
                    })
                },
            )
        })
        .context("Failed to open MultiWorkspace window")?;

    cx.run_until_parked();

    // Create the sidebar outside the MultiWorkspace update to avoid a
    // re-entrant read panic (Sidebar::new reads the MultiWorkspace).
    let sidebar = cx
        .update_window(multi_workspace_window.into(), |root_view, window, cx| {
            let mw_handle: Entity<MultiWorkspace> = root_view
                .downcast()
                .map_err(|_| anyhow::anyhow!("Failed to downcast root view to MultiWorkspace"))?;
            Ok::<_, anyhow::Error>(cx.new(|cx| sidebar::Sidebar::new(mw_handle, window, cx)))
        })
        .context("Failed to create sidebar")??;

    multi_workspace_window
        .update(cx, |mw, _window, cx| {
            mw.register_sidebar(sidebar.clone(), window, cx);
        })
        .context("Failed to register sidebar")?;

    cx.run_until_parked();

    // Open the sidebar
    multi_workspace_window
        .update(cx, |mw, window, cx| {
            mw.toggle_sidebar(window, cx);
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

    Ok(multi_workspace_window)
}

#[cfg(target_os = "macos")]
fn cleanup_sidebar_test_window(
    window: WindowHandle<MultiWorkspace>,
    cx: &mut VisualTestAppContext,
) -> Result<()> {
    window.update(cx, |mw, _window, cx| {
        for workspace in mw.workspaces() {
            let project = workspace.read(cx).project().clone();
            project.update(cx, |project, cx| {
                let ids: Vec<_> = project.worktrees(cx).map(|wt| wt.read(cx).id()).collect();
                for id in ids {
                    project.remove_worktree(id, cx);
                }
            });
        }
    })?;

    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, _cx| {
        window.remove_window();
    })?;

    cx.run_until_parked();

    for _ in 0..15 {
        cx.advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn run_sidebar_duplicate_project_names_visual_tests(
    app_state: Arc<AppState>,
    cx: &mut VisualTestAppContext,
    update_baseline: bool,
) -> Result<TestResult> {
    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.keep();
    let canonical_temp = temp_path.canonicalize()?;

    // Create directory structure where every leaf directory is named "mav" but
    // lives at a distinct path. This lets us test that the sidebar correctly
    // disambiguates projects whose names would otherwise collide.
    //
    //   code/mav/       — project1 (single worktree)
    //   code/foo/mav/   — project2 (single worktree)
    //   code/bar/mav/   — project3, first worktree
    //   code/baz/mav/   — project3, second worktree
    //
    // No two projects share a worktree path, so ProjectGroupBuilder will
    // place each in its own group.
    let code_mav = canonical_temp.join("code").join("mav");
    let foo_mav = canonical_temp.join("code").join("foo").join("mav");
    let bar_mav = canonical_temp.join("code").join("bar").join("mav");
    let baz_mav = canonical_temp.join("code").join("baz").join("mav");
    std::fs::create_dir_all(&code_mav)?;
    std::fs::create_dir_all(&foo_mav)?;
    std::fs::create_dir_all(&bar_mav)?;
    std::fs::create_dir_all(&baz_mav)?;

    cx.update(|cx| {
        cx.update_flags(true, vec!["agent-v2".to_string()]);
    });

    let mut has_baseline_update = None;

    // Two single-worktree projects whose leaf name is "mav"
    {
        let project1 = create_project_with_worktree(&code_mav, &app_state, cx)?;
        let project2 = create_project_with_worktree(&foo_mav, &app_state, cx)?;

        let window = open_sidebar_test_window(vec![project1, project2], &app_state, cx)?;

        let result = run_visual_test(
            "sidebar_two_projects_same_leaf_name",
            window.into(),
            cx,
            update_baseline,
        );

        cleanup_sidebar_test_window(window, cx)?;
        match result? {
            TestResult::Passed => {}
            TestResult::BaselineUpdated(path) => {
                has_baseline_update = Some(path);
            }
        }
    }

    // Three projects, third has two worktrees (all leaf names "mav")
    //
    // project1: code/mav
    // project2: code/foo/mav
    // project3: code/bar/mav + code/baz/mav
    //
    // Each project has a unique set of worktree paths, so they form
    // separate groups. The sidebar must disambiguate all three.
    {
        let project1 = create_project_with_worktree(&code_mav, &app_state, cx)?;
        let project2 = create_project_with_worktree(&foo_mav, &app_state, cx)?;

        let project3 = create_project_with_worktree(&bar_mav, &app_state, cx)?;
        let add_second_worktree = cx.update(|cx| {
            project3.update(cx, |project, cx| {
                project.find_or_create_worktree(&baz_mav, true, cx)
            })
        });
        cx.background_executor.allow_parking();
        cx.foreground_executor
            .block_test(add_second_worktree)
            .context("Failed to add second worktree to project 3")?;
        cx.background_executor.forbid_parking();
        cx.run_until_parked();

        let window = open_sidebar_test_window(vec![project1, project2, project3], &app_state, cx)?;

        let result = run_visual_test(
            "sidebar_three_projects_with_multi_worktree",
            window.into(),
            cx,
            update_baseline,
        );

        cleanup_sidebar_test_window(window, cx)?;
        match result? {
            TestResult::Passed => {}
            TestResult::BaselineUpdated(path) => {
                has_baseline_update = Some(path);
            }
        }
    }

    if let Some(path) = has_baseline_update {
        Ok(TestResult::BaselineUpdated(path))
    } else {
        Ok(TestResult::Passed)
    }
}
