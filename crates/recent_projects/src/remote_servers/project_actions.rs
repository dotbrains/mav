use super::*;

impl RemoteServerProjects {
    fn create_remote_project(
        &mut self,
        index: ServerIndex,
        connection_options: RemoteConnectionOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        let create_new_window = self.create_new_window;
        workspace.update(cx, |_, cx| {
            cx.defer_in(window, move |workspace, window, cx| {
                let app_state = workspace.app_state().clone();
                workspace.toggle_modal(window, cx, |window, cx| {
                    RemoteConnectionModal::new(&connection_options, Vec::new(), window, cx)
                });
                // can be None if another copy of this modal opened in the meantime
                let Some(modal) = workspace.active_modal::<RemoteConnectionModal>(cx) else {
                    return;
                };
                let prompt = modal.read(cx).prompt.clone();

                let connect = connect(
                    ConnectionIdentifier::setup(),
                    connection_options.clone(),
                    prompt,
                    window,
                    cx,
                )
                .prompt_err("Failed to connect", window, cx, |_, _, _| None);

                cx.spawn_in(window, async move |workspace, cx| {
                    let session = connect.await;

                    workspace.update(cx, |workspace, cx| {
                        if let Some(prompt) = workspace.active_modal::<RemoteConnectionModal>(cx) {
                            prompt.update(cx, |prompt, cx| prompt.finished(cx))
                        }
                    })?;

                    let Some(Some(session)) = session else {
                        return workspace.update_in(cx, |workspace, window, cx| {
                            let weak = cx.entity().downgrade();
                            let fs = workspace.project().read(cx).fs().clone();
                            workspace.toggle_modal(window, cx, |window, cx| {
                                RemoteServerProjects::new(create_new_window, fs, window, weak, cx)
                            });
                        });
                    };

                    let (path_style, project) = cx.update(|_, cx| {
                        (
                            session.read(cx).path_style(),
                            project::Project::remote(
                                session,
                                app_state.client.clone(),
                                app_state.node_runtime.clone(),
                                app_state.user_store.clone(),
                                app_state.languages.clone(),
                                app_state.fs.clone(),
                                true,
                                cx,
                            ),
                        )
                    })?;

                    let home_dir = project
                        .read_with(cx, |project, cx| project.resolve_abs_path("~", cx))
                        .await
                        .and_then(|path| path.into_abs_path())
                        .map(|path| RemotePathBuf::new(path, path_style))
                        .unwrap_or_else(|| match path_style {
                            PathStyle::Posix => RemotePathBuf::from_str("/", PathStyle::Posix),
                            PathStyle::Windows => {
                                RemotePathBuf::from_str("C:\\", PathStyle::Windows)
                            }
                        });

                    workspace
                        .update_in(cx, |workspace, window, cx| {
                            let weak = cx.entity().downgrade();
                            workspace.toggle_modal(window, cx, |window, cx| {
                                RemoteServerProjects::project_picker(
                                    create_new_window,
                                    index,
                                    connection_options,
                                    project,
                                    home_dir,
                                    window,
                                    cx,
                                    weak,
                                )
                            });
                        })
                        .ok();
                    Ok(())
                })
                .detach();
            })
        })
    }

    fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        match &self.mode {
            Mode::Default | Mode::ViewServerOptions(_) => {}
            Mode::ProjectPicker(_) => {}
            Mode::CreateRemoteServer(state) => {
                if let Some(prompt) = state.ssh_prompt.as_ref() {
                    prompt.update(cx, |prompt, cx| {
                        prompt.confirm(window, cx);
                    });
                    return;
                }

                self.create_ssh_server(state.address_editor.clone(), window, cx);
            }
            Mode::CreateRemoteDevContainer(_) => {}
            Mode::EditNickname(state) => {
                let text = Some(state.editor.read(cx).text(cx)).filter(|text| !text.is_empty());
                let index = state.index;
                self.update_settings_file(cx, move |setting, _| {
                    if let Some(connections) = setting.ssh_connections.as_mut()
                        && let Some(connection) = connections.get_mut(index.0)
                    {
                        connection.nickname = text;
                    }
                });
                self.mode = Mode::default_mode(&self.ssh_config_servers, cx);
                self.focus_handle.focus(window, cx);
            }
            #[cfg(target_os = "windows")]
            Mode::AddWslDistro(state) => {
                let delegate = &state.picker.read(cx).delegate;
                let distro = delegate.selected_distro().unwrap();
                self.connect_wsl_distro(state.picker.clone(), distro, window, cx);
            }
        }
    }

    fn cancel(&mut self, _: &menu::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        match &self.mode {
            Mode::Default => {
                cx.emit(DismissEvent);
            }
            Mode::CreateRemoteServer(state) if state.ssh_prompt.is_some() => {
                let new_state = CreateRemoteServer::new(window, cx);
                let old_prompt = state.address_editor.read(cx).text(cx);
                new_state.address_editor.update(cx, |this, cx| {
                    this.set_text(old_prompt, window, cx);
                });

                self.mode = Mode::CreateRemoteServer(new_state);
                cx.notify();
            }
            Mode::CreateRemoteDevContainer(CreateRemoteDevContainer {
                progress: DevContainerCreationProgress::Error(_),
                ..
            }) => {
                cx.emit(DismissEvent);
            }
            _ => {
                self.allow_dismissal = true;
                self.mode = Mode::default_mode(&self.ssh_config_servers, cx);
                self.focus_handle(cx).focus(window, cx);
                cx.notify();
            }
        }
    }

    /// Rebuilds the default picker's data from the latest settings/ssh-config
    /// and re-applies the current filter query.
    fn workspace_flags(workspace: &WeakEntity<Workspace>, cx: &App) -> (bool, bool) {
        let has_open_project = workspace
            .upgrade()
            .map(|workspace| {
                workspace
                    .read(cx)
                    .project()
                    .read(cx)
                    .visible_worktrees(cx)
                    .next()
                    .is_some()
            })
            .unwrap_or(false);
        let is_local = workspace
            .upgrade()
            .map(|workspace| workspace.read(cx).project().read(cx).is_local())
            .unwrap_or(true);
        (has_open_project, is_local)
    }

    fn refresh_default_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ssh_config_servers = self.ssh_config_servers.clone();
        let (has_open_project, is_local) = Self::workspace_flags(&self.workspace, cx);
        self.default_picker.update(cx, |picker, cx| {
            picker
                .delegate
                .reload(&ssh_config_servers, has_open_project, is_local, cx);
            picker.refresh(window, cx);
        });
    }

    /// Opens a saved remote project, mirroring whether a new window should be
    /// created based on the modal's `create_new_window` preference and whether
    /// the confirm was a secondary (platform-modifier) confirm.
    fn open_remote_project_entry(
        &mut self,
        _index: ServerIndex,
        project: RemoteProject,
        connection: Connection,
        secondary_confirm: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(app_state) = self
            .workspace
            .read_with(cx, |workspace, _| workspace.app_state().clone())
            .log_err()
        else {
            return;
        };
        let create_new_window = self.create_new_window;
        cx.emit(DismissEvent);

        let replace_window = match (create_new_window, secondary_confirm) {
            (true, false) | (false, true) => None,
            (true, true) | (false, false) => window.window_handle().downcast::<MultiWorkspace>(),
        };

        cx.spawn_in(window, async move |_, cx| {
            let result = open_remote_project(
                connection.into(),
                project.paths.into_iter().map(PathBuf::from).collect(),
                app_state,
                OpenOptions {
                    requesting_window: replace_window,
                    ..OpenOptions::default()
                },
                cx,
            )
            .await;
            if let Err(e) = result {
                log::error!("Failed to connect: {e:#}");
                cx.prompt(
                    gpui::PromptLevel::Critical,
                    "Failed to connect",
                    Some(&e.to_string()),
                    &["OK"],
                )
                .await
                .ok();
            }
        })
        .detach();
    }
}
