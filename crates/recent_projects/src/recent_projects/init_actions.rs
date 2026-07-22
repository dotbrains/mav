use super::*;

pub fn init(cx: &mut App) {
    #[cfg(target_os = "windows")]
    cx.on_action(|open_wsl: &mav_actions::wsl_actions::OpenFolderInWsl, cx| {
        let create_new_window = open_wsl
            .create_new_window
            .unwrap_or_else(|| default_open_in_new_window(cx));
        with_active_or_new_workspace(cx, move |workspace, window, cx| {
            use gpui::PathPromptOptions;
            use project::DirectoryLister;

            let paths = workspace.prompt_for_open_path(
                PathPromptOptions {
                    files: true,
                    directories: true,
                    multiple: false,
                    prompt: None,
                },
                DirectoryLister::Local(
                    workspace.project().clone(),
                    workspace.app_state().fs.clone(),
                ),
                window,
                cx,
            );

            let app_state = workspace.app_state().clone();
            let window_handle = window.window_handle().downcast::<MultiWorkspace>();

            cx.spawn_in(window, async move |workspace, cx| {
                use util::paths::SanitizedPath;

                let Some(paths) = paths.await.log_err().flatten() else {
                    return;
                };

                let wsl_path = paths
                    .iter()
                    .find_map(util::paths::WslPath::from_path);

                if let Some(util::paths::WslPath { distro, path }) = wsl_path {
                    use remote::WslConnectionOptions;

                    let connection_options = RemoteConnectionOptions::Wsl(WslConnectionOptions {
                        distro_name: distro.to_string(),
                        user: None,
                    });

                    let requesting_window = match create_new_window {
                        false => window_handle,
                        true => None,
                    };

                    let open_options = workspace::OpenOptions {
                        requesting_window,
                        ..Default::default()
                    };

                    open_remote_project(connection_options, vec![path.into()], app_state, open_options, cx).await.log_err();
                    return;
                }

                let paths = paths
                    .into_iter()
                    .filter_map(|path| SanitizedPath::new(&path).local_to_wsl())
                    .collect::<Vec<_>>();

                if paths.is_empty() {
                    let message = indoc::indoc! { r#"
                        Invalid path specified when trying to open a folder inside WSL.

                        Please note that Mav currently does not support opening network share folders inside wsl.
                    "#};

                    let _ = cx.prompt(gpui::PromptLevel::Critical, "Invalid path", Some(&message), &["OK"]).await;
                    return;
                }

                workspace.update_in(cx, |workspace, window, cx| {
                    workspace.toggle_modal(window, cx, |window, cx| {
                        crate::wsl_picker::WslOpenModal::new(paths, create_new_window, window, cx)
                    });
                }).log_err();
            })
            .detach();
        });
    });

    #[cfg(target_os = "windows")]
    cx.on_action(|open_wsl: &mav_actions::wsl_actions::OpenWsl, cx| {
        let create_new_window = open_wsl
            .create_new_window
            .unwrap_or_else(|| default_open_in_new_window(cx));
        with_active_or_new_workspace(cx, move |workspace, window, cx| {
            let handle = cx.entity().downgrade();
            let fs = workspace.project().read(cx).fs().clone();
            workspace.toggle_modal(window, cx, |window, cx| {
                RemoteServerProjects::wsl(create_new_window, fs, window, handle, cx)
            });
        });
    });

    #[cfg(target_os = "windows")]
    cx.on_action(|open_wsl: &remote::OpenWslPath, cx| {
        let open_wsl = open_wsl.clone();
        with_active_or_new_workspace(cx, move |workspace, window, cx| {
            let fs = workspace.project().read(cx).fs().clone();
            add_wsl_distro(fs, &open_wsl.distro, cx);
            let requesting_window =
                match workspace::WorkspaceSettings::get_global(cx).default_open_behavior {
                    DefaultOpenBehavior::ExistingWindow => {
                        window.window_handle().downcast::<MultiWorkspace>()
                    }
                    DefaultOpenBehavior::NewWindow => None,
                };
            let open_options = OpenOptions {
                requesting_window,
                ..Default::default()
            };

            let app_state = workspace.app_state().clone();

            cx.spawn_in(window, async move |_, cx| {
                open_remote_project(
                    RemoteConnectionOptions::Wsl(open_wsl.distro.clone()),
                    open_wsl.paths,
                    app_state,
                    open_options,
                    cx,
                )
                .await
            })
            .detach();
        });
    });

    cx.on_action(|open_recent: &OpenRecent, cx| {
        let create_new_window = open_recent.create_new_window;

        match cx
            .active_window()
            .and_then(|w| w.downcast::<MultiWorkspace>())
        {
            Some(multi_workspace) => {
                cx.defer(move |cx| {
                    multi_workspace
                        .update(cx, |multi_workspace, window, cx| {
                            let window_project_groups: Vec<ProjectGroupKey> =
                                multi_workspace.project_group_keys();

                            let workspace = multi_workspace.workspace().clone();
                            workspace.update(cx, |workspace, cx| {
                                let Some(recent_projects) =
                                    workspace.active_modal::<RecentProjects>(cx)
                                else {
                                    let focus_handle = workspace.focus_handle(cx);
                                    RecentProjects::open(
                                        workspace,
                                        create_new_window,
                                        window_project_groups,
                                        window,
                                        focus_handle,
                                        cx,
                                    );
                                    return;
                                };

                                recent_projects.update(cx, |recent_projects, cx| {
                                    recent_projects
                                        .picker
                                        .update(cx, |picker, cx| picker.cycle_selection(window, cx))
                                });
                            });
                        })
                        .log_err();
                });
            }
            None => {
                with_active_or_new_workspace(cx, move |workspace, window, cx| {
                    let Some(recent_projects) = workspace.active_modal::<RecentProjects>(cx) else {
                        let focus_handle = workspace.focus_handle(cx);
                        RecentProjects::open(
                            workspace,
                            create_new_window,
                            Vec::new(),
                            window,
                            focus_handle,
                            cx,
                        );
                        return;
                    };

                    recent_projects.update(cx, |recent_projects, cx| {
                        recent_projects
                            .picker
                            .update(cx, |picker, cx| picker.cycle_selection(window, cx))
                    });
                });
            }
        }
    });
    cx.on_action(|open_remote: &OpenRemote, cx| {
        let from_existing_connection = open_remote.from_existing_connection;
        let create_new_window = open_remote
            .create_new_window
            .unwrap_or_else(|| default_open_in_new_window(cx));
        with_active_or_new_workspace(cx, move |workspace, window, cx| {
            if from_existing_connection {
                cx.propagate();
                return;
            }
            let handle = cx.entity().downgrade();
            let fs = workspace.project().read(cx).fs().clone();
            workspace.toggle_modal(window, cx, |window, cx| {
                RemoteServerProjects::new(create_new_window, fs, window, handle, cx)
            })
        });
    });

    cx.observe_new(DisconnectedOverlay::register).detach();

    cx.on_action(|_: &OpenDevContainer, cx| {
        with_active_or_new_workspace(cx, move |workspace, window, cx| {
            if !workspace.project().read(cx).is_local() {
                cx.spawn_in(window, async move |_, cx| {
                    cx.prompt(
                        gpui::PromptLevel::Critical,
                        "Cannot open Dev Container from remote project",
                        None,
                        &["OK"],
                    )
                    .await
                    .ok();
                })
                .detach();
                return;
            }

            let fs = workspace.project().read(cx).fs().clone();
            let configs = find_devcontainer_configs(workspace, cx);
            let app_state = workspace.app_state().clone();
            let dev_container_context = DevContainerContext::from_workspace(workspace, cx);
            let handle = cx.entity().downgrade();
            workspace.toggle_modal(window, cx, |window, cx| {
                RemoteServerProjects::new_dev_container(
                    fs,
                    configs,
                    app_state,
                    dev_container_context,
                    window,
                    handle,
                    cx,
                )
            });
        });
    });

    // Subscribe to worktree additions to suggest opening the project in a dev container
    cx.observe_new(
        |workspace: &mut Workspace, window: Option<&mut Window>, cx: &mut Context<Workspace>| {
            let Some(window) = window else {
                return;
            };
            cx.subscribe_in(
                workspace.project(),
                window,
                move |workspace, project, event, window, cx| {
                    if let project::Event::WorktreeUpdatedEntries(worktree_id, updated_entries) =
                        event
                    {
                        dev_container_suggest::suggest_on_worktree_updated(
                            workspace,
                            *worktree_id,
                            updated_entries,
                            project,
                            window,
                            cx,
                        );
                    }
                },
            )
            .detach();
        },
    )
    .detach();
}

#[cfg(target_os = "windows")]
pub fn add_wsl_distro(
    fs: Arc<dyn project::Fs>,
    connection_options: &remote::WslConnectionOptions,
    cx: &App,
) {
    use gpui::ReadGlobal;
    use settings::SettingsStore;

    let distro_name = connection_options.distro_name.clone();
    let user = connection_options.user.clone();
    SettingsStore::global(cx).update_settings_file(fs, move |setting, _| {
        let connections = setting
            .remote
            .wsl_connections
            .get_or_insert(Default::default());

        if !connections
            .iter()
            .any(|conn| conn.distro_name == distro_name && conn.user == user)
        {
            use std::collections::BTreeSet;

            connections.push(settings::WslConnection {
                distro_name,
                user,
                projects: BTreeSet::new(),
            })
        }
    });
}
