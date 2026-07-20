use super::*;

pub async fn find_existing_workspace(
    abs_paths: &[PathBuf],
    open_options: &OpenOptions,
    location: &SerializedWorkspaceLocation,
    cx: &mut AsyncApp,
) -> (
    Option<(WindowHandle<MultiWorkspace>, Entity<Workspace>)>,
    OpenVisible,
) {
    let mut existing: Option<(WindowHandle<MultiWorkspace>, Entity<Workspace>)> = None;
    let mut open_visible = OpenVisible::All;
    let mut best_match = None;

    if open_options.workspace_matching != WorkspaceMatching::None {
        cx.update(|cx| {
            for window in workspace_windows_for_location(location, cx) {
                if let Ok(multi_workspace) = window.read(cx) {
                    for workspace in multi_workspace.workspaces() {
                        let project = workspace.read(cx).project.read(cx);
                        let m = project.visibility_for_paths(
                            abs_paths,
                            open_options.workspace_matching != WorkspaceMatching::MatchSubdirectory,
                            cx,
                        );
                        if m > best_match {
                            existing = Some((window, workspace.clone()));
                            best_match = m;
                        } else if best_match.is_none()
                            && open_options.workspace_matching
                                == WorkspaceMatching::MatchSubdirectory
                        {
                            existing = Some((window, workspace.clone()))
                        }
                    }
                }
            }
        });

        let all_paths_are_files = existing
            .as_ref()
            .and_then(|(_, target_workspace)| {
                cx.update(|cx| {
                    let workspace = target_workspace.read(cx);
                    let project = workspace.project.read(cx);
                    let path_style = workspace.path_style(cx);
                    Some(!abs_paths.iter().any(|path| {
                        let path = util::paths::SanitizedPath::new(path);
                        project.worktrees(cx).any(|worktree| {
                            let worktree = worktree.read(cx);
                            let abs_path = worktree.abs_path();
                            path_style
                                .strip_prefix(path.as_ref(), abs_path.as_ref())
                                .and_then(|rel| worktree.entry_for_path(&rel))
                                .is_some_and(|e| e.is_dir())
                        })
                    }))
                })
            })
            .unwrap_or(false);

        if open_options.wait && existing.is_some() && all_paths_are_files {
            cx.update(|cx| {
                let windows = workspace_windows_for_location(location, cx);
                let window = cx
                    .active_window()
                    .and_then(|window| window.downcast::<MultiWorkspace>())
                    .filter(|window| windows.contains(window))
                    .or_else(|| windows.into_iter().next());
                if let Some(window) = window {
                    if let Ok(multi_workspace) = window.read(cx) {
                        let active_workspace = multi_workspace.workspace().clone();
                        existing = Some((window, active_workspace));
                        open_visible = OpenVisible::None;
                    }
                }
            });
        }
    }
    (existing, open_visible)
}

/// Opens a workspace by its database ID, used for restoring empty workspaces with unsaved content.
pub fn open_workspace_by_id(
    workspace_id: WorkspaceId,
    app_state: Arc<AppState>,
    requesting_window: Option<WindowHandle<MultiWorkspace>>,
    cx: &mut App,
) -> Task<anyhow::Result<WindowHandle<MultiWorkspace>>> {
    let project_handle = Project::local(
        app_state.client.clone(),
        app_state.node_runtime.clone(),
        app_state.user_store.clone(),
        app_state.languages.clone(),
        app_state.fs.clone(),
        None,
        project::LocalProjectFlags {
            init_worktree_trust: true,
            ..project::LocalProjectFlags::default()
        },
        cx,
    );

    let db = WorkspaceDb::global(cx);
    let kvp = db::kvp::KeyValueStore::global(cx);
    cx.spawn(async move |cx| {
        let serialized_workspace = db
            .workspace_for_id(workspace_id)
            .with_context(|| format!("Workspace {workspace_id:?} not found"))?;

        let centered_layout = serialized_workspace.centered_layout;

        let (window, workspace) = if let Some(window) = requesting_window {
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
                    workspace
                });
                multi_workspace.add(workspace.clone(), &*window, cx);
                workspace
            })?;
            (window, workspace)
        } else {
            let window_bounds_override = window_bounds_env_override();

            let (window_bounds, display) = if let Some(bounds) = window_bounds_override {
                (Some(WindowBounds::Windowed(bounds)), None)
            } else if let Some(display) = serialized_workspace.display
                && let Some(bounds) = serialized_workspace.window_bounds.as_ref()
            {
                (Some(bounds.0), Some(display))
            } else if let Some((display, bounds)) = persistence::read_default_window_bounds(&kvp) {
                (Some(bounds), Some(display))
            } else {
                (None, None)
            };

            let options = cx.update(|cx| {
                let mut options = (app_state.build_window_options)(display, cx);
                options.window_bounds = window_bounds;
                options
            });

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
                        workspace
                    });
                    cx.new(|cx| MultiWorkspace::new(workspace, window, cx))
                }
            })?;

            let workspace = window.update(cx, |multi_workspace: &mut MultiWorkspace, _, _cx| {
                multi_workspace.workspace().clone()
            })?;

            (window, workspace)
        };

        notify_if_database_failed(window, cx);

        // Restore items from the serialized workspace
        window
            .update(cx, |_, window, cx| {
                workspace.update(cx, |_workspace, cx| {
                    open_items(Some(serialized_workspace), vec![], window, cx)
                })
            })?
            .await?;

        window.update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.serialize_workspace(window, cx);
            });
        })?;

        Ok(window)
    })
}

#[allow(clippy::type_complexity)]
pub fn open_paths(
    abs_paths: &[PathBuf],
    app_state: Arc<AppState>,
    mut open_options: OpenOptions,
    cx: &mut App,
) -> Task<anyhow::Result<OpenResult>> {
    let abs_paths = abs_paths.to_vec();
    #[cfg(target_os = "windows")]
    let wsl_path = abs_paths
        .iter()
        .find_map(|p| util::paths::WslPath::from_path(p));

    cx.spawn(async move |cx| {
        let (mut existing, mut open_visible) = find_existing_workspace(
            &abs_paths,
            &open_options,
            &SerializedWorkspaceLocation::Local,
            cx,
        )
        .await;

        // Fallback: if no workspace contains the paths and all paths are files,
        // prefer an existing local workspace window (active window first).
        if open_options.should_reuse_existing_window() && existing.is_none() {
            let all_paths = abs_paths.iter().map(|path| app_state.fs.metadata(path));
            let all_metadatas = futures::future::join_all(all_paths)
                .await
                .into_iter()
                .filter_map(|result| result.ok().flatten());

            if all_metadatas.into_iter().all(|file| !file.is_dir) {
                cx.update(|cx| {
                    let windows = workspace_windows_for_location(
                        &SerializedWorkspaceLocation::Local,
                        cx,
                    );
                    let window = cx
                        .active_window()
                        .and_then(|window| window.downcast::<MultiWorkspace>())
                        .filter(|window| windows.contains(window))
                        .or_else(|| windows.into_iter().next());
                    if let Some(window) = window {
                        if let Ok(multi_workspace) = window.read(cx) {
                            let active_workspace = multi_workspace.workspace().clone();
                            existing = Some((window, active_workspace));
                            open_visible = OpenVisible::None;
                        }
                    }
                });
            }
        }

        // Fallback for directories: when no flag is specified and no existing
        // workspace matched, check the user's setting to decide whether to add
        // the directory as a new workspace in the active window's MultiWorkspace
        // or open a new window.
        // Skip when requesting_window is already set: the caller (e.g.
        // open_workspace_for_paths reusing an empty window) already chose the
        // target window, so we must not open the sidebar as a side-effect.
        if open_options.should_reuse_existing_window()
            && existing.is_none()
            && open_options.requesting_window.is_none()
        {
            let use_existing_window = open_options.add_dirs_to_sidebar;

            if use_existing_window {
                let target_window = cx.update(|cx| {
                    let windows = workspace_windows_for_location(
                        &SerializedWorkspaceLocation::Local,
                        cx,
                    );
                    let window = cx
                        .active_window()
                        .and_then(|window| window.downcast::<MultiWorkspace>())
                        .filter(|window| windows.contains(window))
                        .or_else(|| windows.into_iter().next());
                    window.filter(|window| {
                        window
                            .read(cx)
                            .is_ok_and(|mw| mw.multi_workspace_enabled(cx))
                    })
                });

                if let Some(window) = target_window {
                    open_options.requesting_window = Some(window);
                    window
                        .update(cx, |multi_workspace, _, cx| {
                            multi_workspace.open_sidebar(cx);
                        })
                        .log_err();
                }
            }
        }

        let open_in_dev_container = open_options.open_in_dev_container;

        let result = if let Some((existing, target_workspace)) = existing {
            let open_task = existing
                .update(cx, |multi_workspace, window, cx| {
                    window.activate_window();
                    multi_workspace.activate(target_workspace.clone(), None, window, cx);
                    target_workspace.update(cx, |workspace, cx| {
                        if open_in_dev_container {
                            workspace.set_open_in_dev_container(true);
                        }
                        workspace.open_paths(
                            abs_paths,
                            OpenOptions {
                                visible: Some(open_visible),
                                ..Default::default()
                            },
                            None,
                            window,
                            cx,
                        )
                    })
                })?
                .await;

            _ = existing.update(cx, |multi_workspace, _, cx| {
                let workspace = multi_workspace.workspace().clone();
                workspace.update(cx, |workspace, cx| {
                    for item in open_task.iter().flatten() {
                        if let Err(e) = item {
                            workspace.show_error(format!("Error: {e}"), cx);
                        }
                    }
                });
            });

            Ok(OpenResult { window: existing, workspace: target_workspace, opened_items: open_task })
        } else {
            let init = if open_in_dev_container {
                Some(Box::new(|workspace: &mut Workspace, _window: &mut Window, _cx: &mut Context<Workspace>| {
                    workspace.set_open_in_dev_container(true);
                }) as Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + Send>)
            } else {
                None
            };
            let result = cx
                .update(move |cx| {
                    Workspace::new_local(
                        abs_paths,
                        app_state.clone(),
                        open_options.requesting_window,
                        open_options.env,
                        init,
                        open_options.open_mode,
                        cx,
                    )
                })
                .await;

            if let Ok(ref result) = result {
                result.window
                    .update(cx, |_, window, _cx| {
                        window.activate_window();
                    })
                    .log_err();
            }

            result
        };

        #[cfg(target_os = "windows")]
        if let Some(util::paths::WslPath{distro, path}) = wsl_path
            && let Ok(ref result) = result
        {
            result.window
                .update(cx, move |multi_workspace, _window, cx| {
                    struct OpenInWsl;
                    let workspace = multi_workspace.workspace().clone();
                    workspace.update(cx, |workspace, cx| {
                        workspace.show_notification(NotificationId::unique::<OpenInWsl>(), cx, move |cx| {
                            let display_path = util::markdown::MarkdownInlineCode(&path.to_string_lossy());
                            let msg = format!("{display_path} is inside a WSL filesystem, some features may not work unless you open it with WSL remote");
                            cx.new(move |cx| {
                                MessageNotification::new(msg, cx)
                                    .primary_message("Open in WSL")
                                    .primary_icon(IconName::FolderOpen)
                                    .primary_on_click(move |window, cx| {
                                        window.dispatch_action(Box::new(remote::OpenWslPath {
                                                distro: remote::WslConnectionOptions {
                                                        distro_name: distro.clone(),
                                                    user: None,
                                                },
                                                paths: vec![path.clone().into()],
                                            }), cx)
                                    })
                            })
                        });
                    });
                })
                .unwrap();
        };
        result
    })
}

pub fn open_new(
    open_options: OpenOptions,
    app_state: Arc<AppState>,
    cx: &mut App,
    init: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static + Send,
) -> Task<anyhow::Result<()>> {
    let addition = open_options.open_mode;
    let task = Workspace::new_local(
        Vec::new(),
        app_state,
        open_options.requesting_window,
        open_options.env,
        Some(Box::new(init)),
        addition,
        cx,
    );
    cx.spawn(async move |cx| {
        let OpenResult { window, .. } = task.await?;
        window
            .update(cx, |_, window, _cx| {
                window.activate_window();
            })
            .ok();
        Ok(())
    })
}

pub fn create_and_open_local_file(
    path: &'static Path,
    window: &mut Window,
    cx: &mut Context<Workspace>,
    default_content: impl 'static + Send + FnOnce() -> Rope,
) -> Task<Result<Box<dyn ItemHandle>>> {
    cx.spawn_in(window, async move |workspace, cx| {
        let fs = workspace.read_with(cx, |workspace, _| workspace.app_state().fs.clone())?;
        if !fs.is_file(path).await {
            fs.create_file(path, Default::default()).await?;
            fs.save(path, &default_content(), Default::default())
                .await?;
        }

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.with_local_or_wsl_workspace(window, cx, |workspace, window, cx| {
                    let path = workspace
                        .project
                        .read_with(cx, |project, cx| project.try_windows_path_to_wsl(path, cx));
                    cx.spawn_in(window, async move |workspace, cx| {
                        let path = path.await?;

                        let path = fs.canonicalize(&path).await.unwrap_or(path);

                        let mut items = workspace
                            .update_in(cx, |workspace, window, cx| {
                                workspace.open_paths(
                                    vec![path.to_path_buf()],
                                    OpenOptions {
                                        visible: Some(OpenVisible::None),
                                        ..Default::default()
                                    },
                                    None,
                                    window,
                                    cx,
                                )
                            })?
                            .await;
                        let item = items.pop().flatten();
                        item.with_context(|| format!("path {path:?} is not a file"))?
                    })
                })
            })?
            .await?
            .await
    })
}
