use super::*;

enum ProjectPickerData {
    Ssh {
        connection_string: SharedString,
        nickname: Option<SharedString>,
    },
    Wsl {
        distro_name: SharedString,
    },
}

pub(super) struct ProjectPicker {
    data: ProjectPickerData,
    picker: Entity<Picker<OpenPathDelegate>>,
    _path_task: Shared<Task<Option<()>>>,
}

impl Focusable for ProjectPicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl ProjectPicker {
    pub(super) fn new(
        create_new_window: bool,
        index: ServerIndex,
        connection: RemoteConnectionOptions,
        project: Entity<Project>,
        home_dir: RemotePathBuf,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<RemoteServerProjects>,
    ) -> Entity<Self> {
        let (tx, rx) = oneshot::channel();
        let lister = project::DirectoryLister::Project(project.clone());
        let delegate = open_path_prompt::OpenPathDelegate::new(tx, lister, false, cx).show_hidden();

        let picker = cx.new(|cx| {
            let picker = Picker::uniform_list(delegate, window, cx).embedded();
            picker.set_query(&home_dir.to_string(), window, cx);
            picker
        });

        let data = match &connection {
            RemoteConnectionOptions::Ssh(connection) => ProjectPickerData::Ssh {
                connection_string: connection.connection_string().into(),
                nickname: connection.nickname.clone().map(|nick| nick.into()),
            },
            RemoteConnectionOptions::Wsl(connection) => ProjectPickerData::Wsl {
                distro_name: connection.distro_name.clone().into(),
            },
            RemoteConnectionOptions::Docker(_) => ProjectPickerData::Ssh {
                // Not implemented as a project picker at this time
                connection_string: "".into(),
                nickname: None,
            },
            #[cfg(any(test, feature = "test-support"))]
            RemoteConnectionOptions::Mock(options) => ProjectPickerData::Ssh {
                connection_string: format!("mock-{}", options.id).into(),
                nickname: None,
            },
        };
        let _path_task = cx
            .spawn_in(window, {
                let workspace = workspace;
                async move |this, cx| {
                    let Ok(Some(paths)) = rx.await else {
                        workspace
                            .update_in(cx, |workspace, window, cx| {
                                let fs = workspace.project().read(cx).fs().clone();
                                let weak = cx.entity().downgrade();
                                workspace.toggle_modal(window, cx, |window, cx| {
                                    RemoteServerProjects::new(
                                        create_new_window,
                                        fs,
                                        window,
                                        weak,
                                        cx,
                                    )
                                });
                            })
                            .log_err()?;
                        return None;
                    };

                    let app_state = workspace
                        .read_with(cx, |workspace, _| workspace.app_state().clone())
                        .ok()?;

                    let remote_connection = project.read_with(cx, |project, cx| {
                        project.remote_client()?.read(cx).connection()
                    })?;

                    let (paths, paths_with_positions) =
                        determine_paths_with_positions(&remote_connection, paths).await;

                    cx.update(|_, cx| {
                        let fs = app_state.fs.clone();
                        update_settings_file(fs, cx, {
                            let paths = paths
                                .iter()
                                .map(|path| path.to_string_lossy().into_owned())
                                .collect();
                            move |settings, _| match index {
                                ServerIndex::Ssh(index) => {
                                    if let Some(server) = settings
                                        .remote
                                        .ssh_connections
                                        .as_mut()
                                        .and_then(|connections| connections.get_mut(index.0))
                                    {
                                        server.projects.insert(RemoteProject { paths });
                                    };
                                }
                                ServerIndex::Wsl(index) => {
                                    if let Some(server) = settings
                                        .remote
                                        .wsl_connections
                                        .as_mut()
                                        .and_then(|connections| connections.get_mut(index.0))
                                    {
                                        server.projects.insert(RemoteProject { paths });
                                    };
                                }
                            }
                        });
                    })
                    .log_err();

                    let window = if create_new_window {
                        let options = cx
                            .update(|_, cx| (app_state.build_window_options)(None, cx))
                            .log_err()?;
                        cx.open_window(options, |window, cx| {
                            let workspace = cx.new(|cx| {
                                telemetry::event!("SSH Project Created");
                                Workspace::new(None, project.clone(), app_state.clone(), window, cx)
                            });
                            cx.new(|cx| MultiWorkspace::new(workspace, window, cx))
                        })
                        .log_err()
                    } else {
                        cx.window_handle().downcast::<MultiWorkspace>()
                    }?;

                    let items = open_remote_project_with_existing_connection(
                        connection, project, paths, app_state, window, None, None, cx,
                    )
                    .await
                    .log_err();

                    if let Some(items) = items {
                        for (item, path) in items.into_iter().zip(paths_with_positions) {
                            let Some(item) = item else {
                                continue;
                            };
                            let Some(row) = path.row else {
                                continue;
                            };
                            if let Some(active_editor) = item.downcast::<Editor>() {
                                window
                                    .update(cx, |_, window, cx| {
                                        active_editor.update(cx, |editor, cx| {
                                            let row = row.saturating_sub(1);
                                            let col = path.column.unwrap_or(0).saturating_sub(1);
                                            let Some(buffer) =
                                                editor.buffer().read(cx).as_singleton()
                                            else {
                                                return;
                                            };
                                            let buffer_snapshot = buffer.read(cx).snapshot();
                                            let point =
                                                buffer_snapshot.point_from_external_input(row, col);
                                            editor.go_to_singleton_buffer_point(point, window, cx);
                                        });
                                    })
                                    .ok();
                            }
                        }
                    }

                    this.update(cx, |_, cx| {
                        cx.emit(DismissEvent);
                    })
                    .ok();
                    Some(())
                }
            })
            .shared();
        cx.new(|_| Self {
            _path_task,
            picker,
            data,
        })
    }
}

impl gpui::Render for ProjectPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .child(match &self.data {
                ProjectPickerData::Ssh {
                    connection_string,
                    nickname,
                } => SshConnectionHeader {
                    connection_string: connection_string.clone(),
                    paths: Default::default(),
                    nickname: nickname.clone(),
                    is_wsl: false,
                    is_devcontainer: false,
                }
                .render(window, cx),
                ProjectPickerData::Wsl { distro_name } => SshConnectionHeader {
                    connection_string: distro_name.clone(),
                    paths: Default::default(),
                    nickname: None,
                    is_wsl: true,
                    is_devcontainer: false,
                }
                .render(window, cx),
            })
            .child(
                div()
                    .border_t_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(self.picker.clone()),
            )
    }
}
