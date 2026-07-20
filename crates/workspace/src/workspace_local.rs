use super::*;

impl Workspace {
    pub fn new_local(
        abs_paths: Vec<PathBuf>,
        app_state: Arc<AppState>,
        requesting_window: Option<WindowHandle<MultiWorkspace>>,
        env: Option<HashMap<String, String>>,
        init: Option<Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + Send>>,
        open_mode: OpenMode,
        cx: &mut App,
    ) -> Task<anyhow::Result<OpenResult>> {
        let project_handle = Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            env,
            Default::default(),
            cx,
        );

        let db = WorkspaceDb::global(cx);
        let kvp = db::kvp::KeyValueStore::global(cx);
        cx.spawn(async move |cx| {
            let mut paths_to_open = Vec::with_capacity(abs_paths.len());
            for path in abs_paths.into_iter() {
                if let Some(canonical) = app_state.fs.canonicalize(&path).await.ok() {
                    paths_to_open.push(canonical)
                } else {
                    paths_to_open.push(path)
                }
            }

            let serialized_workspace = db.workspace_for_roots(paths_to_open.as_slice());
            let restored_multi_workspace_state = if open_mode != OpenMode::Add
                && let Some(window_id) = serialized_workspace.as_ref().and_then(|ws| ws.window_id)
            {
                cx.update(move |cx| {
                    persistence::read_multi_workspace_state_if_present(
                        WindowId::from(window_id),
                        cx,
                    )
                })
            } else {
                None
            };
            let initial_sidebar_open = restored_multi_workspace_state
                .as_ref()
                .map(|state| state.sidebar_open);

            if let Some(paths) = serialized_workspace.as_ref().map(|ws| &ws.paths) {
                paths_to_open = paths.ordered_paths().cloned().collect();
            }

            // Get project paths for all of the abs_paths
            let mut project_paths: Vec<(PathBuf, Option<ProjectPath>)> =
                Vec::with_capacity(paths_to_open.len());

            for path in paths_to_open.into_iter() {
                if let Some((_, project_entry)) = cx
                    .update(|cx| {
                        Workspace::project_path_for_path(project_handle.clone(), &path, true, cx)
                    })
                    .await
                    .log_err()
                {
                    project_paths.push((path, Some(project_entry)));
                } else {
                    project_paths.push((path, None));
                }
            }

            let workspace_id = if let Some(serialized_workspace) = serialized_workspace.as_ref() {
                serialized_workspace.id
            } else {
                db.next_id().await.unwrap_or_else(|_| Default::default())
            };

            let toolchains = db.toolchains(workspace_id).await?;

            for (toolchain, worktree_path, path) in toolchains {
                let toolchain_path = PathBuf::from(toolchain.path.clone().to_string());
                let Some(worktree_id) = project_handle.read_with(cx, |this, cx| {
                    this.find_worktree(&worktree_path, cx)
                        .and_then(|(worktree, rel_path)| {
                            if rel_path.is_empty() {
                                Some(worktree.read(cx).id())
                            } else {
                                None
                            }
                        })
                }) else {
                    // We did not find a worktree with a given path, but that's whatever.
                    continue;
                };
                if !app_state.fs.is_file(toolchain_path.as_path()).await {
                    continue;
                }

                project_handle
                    .update(cx, |this, cx| {
                        this.activate_toolchain(ProjectPath { worktree_id, path }, toolchain, cx)
                    })
                    .await;
            }
            if let Some(workspace) = serialized_workspace.as_ref() {
                project_handle.update(cx, |this, cx| {
                    for (scope, toolchains) in &workspace.user_toolchains {
                        for toolchain in toolchains {
                            this.add_toolchain(toolchain.clone(), scope.clone(), cx);
                        }
                    }
                });
            }

            let window_to_replace = match open_mode {
                OpenMode::NewWindow => None,
                _ => requesting_window,
            };

            let (window, workspace): (WindowHandle<MultiWorkspace>, Entity<Workspace>) =
                if let Some(window) = window_to_replace {
                    let centered_layout = serialized_workspace
                        .as_ref()
                        .map(|w| w.centered_layout)
                        .unwrap_or(false);

                    let workspace = window.update(cx, |multi_workspace, window, cx| {
                        let workspace = cx.new(|cx| {
                            let mut workspace = Workspace::new(
                                Some(workspace_id),
                                project_handle.clone(),
                                app_state.clone(),
                                window,
                                cx,
                            );

                            workspace.centered_layout = centered_layout;

                            // Call init callback to add items before window renders
                            if let Some(init) = init {
                                init(&mut workspace, window, cx);
                            }

                            workspace
                        });
                        match open_mode {
                            OpenMode::Activate => {
                                multi_workspace.activate(workspace.clone(), None, window, cx);
                            }
                            OpenMode::Add => {
                                multi_workspace.add(workspace.clone(), &*window, cx);
                            }
                            OpenMode::NewWindow => {
                                unreachable!()
                            }
                        }
                        workspace
                    })?;
                    (window, workspace)
                } else {
                    let window_bounds_override = window_bounds_env_override();

                    let (window_bounds, display) = if let Some(bounds) = window_bounds_override {
                        (Some(WindowBounds::Windowed(bounds)), None)
                    } else if let Some(workspace) = serialized_workspace.as_ref()
                        && let Some(display) = workspace.display
                        && let Some(bounds) = workspace.window_bounds.as_ref()
                    {
                        // Reopening an existing workspace - restore its saved bounds
                        (Some(bounds.0), Some(display))
                    } else if let Some((display, bounds)) =
                        persistence::read_default_window_bounds(&kvp)
                    {
                        // New or empty workspace - use the last known window bounds
                        (Some(bounds), Some(display))
                    } else {
                        // New window - let GPUI's default_bounds() handle cascading
                        (None, None)
                    };

                    // Use the serialized workspace to construct the new window
                    let mut options = cx.update(|cx| (app_state.build_window_options)(display, cx));
                    options.window_bounds = window_bounds;
                    let centered_layout = serialized_workspace
                        .as_ref()
                        .map(|w| w.centered_layout)
                        .unwrap_or(false);
                    let window = cx.open_window(options, {
                        let app_state = app_state.clone();
                        let project_handle = project_handle.clone();
                        move |window, cx| {
                            let workspace = cx.new(|cx| {
                                let mut workspace = Workspace::new(
                                    Some(workspace_id),
                                    project_handle,
                                    app_state,
                                    window,
                                    cx,
                                );
                                workspace.centered_layout = centered_layout;

                                // Call init callback to add items before window renders
                                if let Some(init) = init {
                                    init(&mut workspace, window, cx);
                                }

                                workspace
                            });
                            cx.new(|cx| {
                                if let Some(sidebar_open) = initial_sidebar_open {
                                    MultiWorkspace::new_with_initial_sidebar_open(
                                        workspace,
                                        window,
                                        cx,
                                        sidebar_open,
                                    )
                                } else {
                                    MultiWorkspace::new(workspace, window, cx)
                                }
                            })
                        }
                    })?;
                    let workspace =
                        window.update(cx, |multi_workspace: &mut MultiWorkspace, _, _cx| {
                            multi_workspace.workspace().clone()
                        })?;
                    (window, workspace)
                };

            if let Some(state) = &restored_multi_workspace_state {
                apply_restored_sidebar_state(window, state, cx);
            }

            notify_if_database_failed(window, cx);
            // Check if this is an empty workspace (no paths to open)
            // An empty workspace is one where project_paths is empty
            let is_empty_workspace = project_paths.is_empty();
            // Check if serialized workspace has paths before it's moved
            let serialized_workspace_has_paths = serialized_workspace
                .as_ref()
                .map(|ws| !ws.paths.is_empty())
                .unwrap_or(false);

            let opened_items = window
                .update(cx, |_, window, cx| {
                    workspace.update(cx, |_workspace: &mut Workspace, cx| {
                        open_items(serialized_workspace, project_paths, window, cx)
                    })
                })?
                .await
                .unwrap_or_default();

            // Restore default dock state for empty workspaces
            // Only restore if:
            // 1. This is an empty workspace (no paths), AND
            // 2. The serialized workspace either doesn't exist or has no paths
            if is_empty_workspace && !serialized_workspace_has_paths {
                if let Some(default_docks) = persistence::read_default_dock_state(&kvp) {
                    window
                        .update(cx, |_, window, cx| {
                            workspace.update(cx, |workspace, cx| {
                                for (dock, serialized_dock) in [
                                    (&workspace.right_dock, &default_docks.right),
                                    (&workspace.left_dock, &default_docks.left),
                                ] {
                                    dock.update(cx, |dock, cx| {
                                        dock.serialized_dock = Some(serialized_dock.clone());
                                        dock.restore_state(window, cx);
                                    });
                                }
                                cx.notify();
                            });
                        })
                        .log_err();
                }
            }

            window
                .update(cx, |_, _window, cx| {
                    workspace.update(cx, |this: &mut Workspace, cx| {
                        this.update_history(cx);
                    });
                })
                .log_err();

            if open_mode == OpenMode::NewWindow || open_mode == OpenMode::Activate {
                window
                    .update(cx, |_, window, _cx| {
                        window.activate_window();
                    })
                    .log_err();
            }

            // Auto-show the security modal if the project has restricted worktrees
            window
                .update(cx, |_, window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace.show_worktree_trust_security_modal(false, window, cx);
                    });
                })
                .log_err();

            Ok(OpenResult {
                window,
                workspace,
                opened_items,
            })
        })
    }
}
