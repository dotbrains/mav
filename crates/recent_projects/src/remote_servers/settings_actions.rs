use super::*;

impl RemoteServerProjects {
    fn update_settings_file(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut RemoteSettingsContent, &App) + Send + Sync + 'static,
    ) {
        let Some(fs) = self
            .workspace
            .read_with(cx, |workspace, _| workspace.app_state().fs.clone())
            .log_err()
        else {
            return;
        };
        update_settings_file(fs, cx, move |setting, cx| f(&mut setting.remote, cx));
    }

    fn delete_ssh_server(&mut self, server: SshServerIndex, cx: &mut Context<Self>) {
        self.update_settings_file(cx, move |setting, _| {
            if let Some(connections) = setting.ssh_connections.as_mut()
                && connections.get(server.0).is_some()
            {
                connections.remove(server.0);
            }
        });
    }

    fn delete_remote_project(
        &mut self,
        server: ServerIndex,
        project: &RemoteProject,
        cx: &mut Context<Self>,
    ) {
        match server {
            ServerIndex::Ssh(server) => {
                self.delete_ssh_project(server, project, cx);
            }
            ServerIndex::Wsl(server) => {
                self.delete_wsl_project(server, project, cx);
            }
        }
    }

    fn delete_ssh_project(
        &mut self,
        server: SshServerIndex,
        project: &RemoteProject,
        cx: &mut Context<Self>,
    ) {
        let project = project.clone();
        self.update_settings_file(cx, move |setting, _| {
            if let Some(server) = setting
                .ssh_connections
                .as_mut()
                .and_then(|connections| connections.get_mut(server.0))
            {
                server.projects.remove(&project);
            }
        });
    }

    fn delete_wsl_project(
        &mut self,
        server: WslServerIndex,
        project: &RemoteProject,
        cx: &mut Context<Self>,
    ) {
        let project = project.clone();
        self.update_settings_file(cx, move |setting, _| {
            if let Some(server) = setting
                .wsl_connections
                .as_mut()
                .and_then(|connections| connections.get_mut(server.0))
            {
                server.projects.remove(&project);
            }
        });
    }

    fn delete_wsl_distro(&mut self, server: WslServerIndex, cx: &mut Context<Self>) {
        self.update_settings_file(cx, move |setting, _| {
            if let Some(connections) = setting.wsl_connections.as_mut() {
                connections.remove(server.0);
            }
        });
    }

    fn add_ssh_server(
        &mut self,
        connection_options: remote::SshConnectionOptions,
        cx: &mut Context<Self>,
    ) {
        self.update_settings_file(cx, move |setting, _| {
            setting
                .ssh_connections
                .get_or_insert(Default::default())
                .push(SshConnection {
                    host: connection_options.host.to_string(),
                    username: connection_options.username,
                    port: connection_options.port,
                    projects: BTreeSet::new(),
                    nickname: None,
                    args: connection_options.args.unwrap_or_default(),
                    upload_binary_over_ssh: None,
                    port_forwards: connection_options.port_forwards,
                    connection_timeout: connection_options.connection_timeout,
                })
        });
    }

    fn edit_in_dev_container_json(
        &mut self,
        config: Option<DevContainerConfig>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            cx.emit(DismissEvent);
            cx.notify();
            return;
        };

        let config_path = config
            .map(|c| c.config_path)
            .unwrap_or_else(|| PathBuf::from(".devcontainer/devcontainer.json"));

        workspace.update(cx, |workspace, cx| {
            let project = workspace.project().clone();

            let worktree = project
                .read(cx)
                .visible_worktrees(cx)
                .find_map(|tree| tree.read(cx).root_entry()?.is_dir().then_some(tree));

            if let Some(worktree) = worktree {
                let tree_id = worktree.read(cx).id();
                let devcontainer_path =
                    match RelPath::new(&config_path, util::paths::PathStyle::Posix) {
                        Ok(path) => path.into_owned(),
                        Err(error) => {
                            log::error!(
                                "Invalid devcontainer path: {} - {}",
                                config_path.display(),
                                error
                            );
                            return;
                        }
                    };
                cx.spawn_in(window, async move |workspace, cx| {
                    workspace
                        .update_in(cx, |workspace, window, cx| {
                            workspace.open_path(
                                (tree_id, devcontainer_path),
                                None,
                                true,
                                window,
                                cx,
                            )
                        })?
                        .await
                })
                .detach();
            } else {
                return;
            }
        });
        cx.emit(DismissEvent);
        cx.notify();
    }
}
