use super::*;

impl MultiWorkspace {
    pub fn workspace_for_paths(
        &self,
        path_list: &PathList,
        host: Option<&RemoteConnectionOptions>,
        cx: &App,
    ) -> Option<Entity<Workspace>> {
        self.workspace_for_paths_excluding(path_list, host, &[], cx)
    }

    fn workspace_for_paths_excluding(
        &self,
        path_list: &PathList,
        host: Option<&RemoteConnectionOptions>,
        excluding: &[Entity<Workspace>],
        cx: &App,
    ) -> Option<Entity<Workspace>> {
        for workspace in self.workspaces() {
            if excluding.contains(workspace) {
                continue;
            }
            let root_paths = PathList::new(&workspace.read(cx).root_paths(cx));
            let key = workspace.read(cx).project_group_key(cx);
            let host_matches = key.host().as_ref() == host;
            let paths_match = root_paths == *path_list;
            if host_matches && paths_match {
                return Some(workspace.clone());
            }
        }

        None
    }

    /// Finds an existing workspace whose paths match, or creates a new one.
    ///
    /// For local projects (`host` is `None`), this delegates to
    /// [`Self::find_or_create_local_workspace`]. For remote projects, it
    /// tries an exact path match and, if no existing workspace is found,
    /// calls `connect_remote` to establish a connection and creates a new
    /// remote workspace.
    ///
    /// The `connect_remote` closure is responsible for any user-facing
    /// connection UI (e.g. password prompts). It receives the connection
    /// options and should return a [`Task`] that resolves to the
    /// [`RemoteClient`] session, or `None` if the connection was
    /// cancelled.
    pub fn find_or_create_workspace(
        &mut self,
        paths: PathList,
        host: Option<RemoteConnectionOptions>,
        provisional_project_group_key: Option<ProjectGroupKey>,
        connect_remote: impl FnOnce(
            RemoteConnectionOptions,
            &mut Window,
            &mut Context<Self>,
        ) -> Task<Result<Option<Entity<remote::RemoteClient>>>>
        + 'static,
        excluding: &[Entity<Workspace>],
        init: Option<Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + Send>>,
        open_mode: OpenMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Workspace>>> {
        self.find_or_create_workspace_with_source_workspace(
            paths,
            host,
            provisional_project_group_key,
            connect_remote,
            excluding,
            init,
            open_mode,
            None,
            window,
            cx,
        )
    }

    pub fn find_or_create_workspace_with_source_workspace(
        &mut self,
        paths: PathList,
        host: Option<RemoteConnectionOptions>,
        provisional_project_group_key: Option<ProjectGroupKey>,
        connect_remote: impl FnOnce(
            RemoteConnectionOptions,
            &mut Window,
            &mut Context<Self>,
        ) -> Task<Result<Option<Entity<remote::RemoteClient>>>>
        + 'static,
        excluding: &[Entity<Workspace>],
        init: Option<Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + Send>>,
        open_mode: OpenMode,
        source_workspace: Option<WeakEntity<Workspace>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Workspace>>> {
        if let Some(workspace) = self.workspace_for_paths(&paths, host.as_ref(), cx) {
            self.activate(workspace.clone(), source_workspace, window, cx);
            return Task::ready(Ok(workspace));
        }

        let Some(connection_options) = host else {
            return self.find_or_create_local_workspace_with_source_workspace(
                paths,
                provisional_project_group_key,
                excluding,
                init,
                open_mode,
                source_workspace,
                window,
                cx,
            );
        };

        let app_state = self.workspace().read(cx).app_state().clone();
        let window_handle = window.window_handle().downcast::<MultiWorkspace>();
        let connect_task = connect_remote(connection_options.clone(), window, cx);
        let paths_vec = paths.paths().to_vec();

        cx.spawn(async move |_this, cx| {
            let session = connect_task
                .await?
                .ok_or_else(|| anyhow::anyhow!("Remote connection was cancelled"))?;

            let new_project = cx.update(|cx| {
                Project::remote(
                    session,
                    app_state.client.clone(),
                    app_state.node_runtime.clone(),
                    app_state.user_store.clone(),
                    app_state.languages.clone(),
                    app_state.fs.clone(),
                    true,
                    cx,
                )
            });

            let effective_paths_vec =
                if let Some(project_group) = provisional_project_group_key.as_ref() {
                    let resolve_tasks = cx.update(|cx| {
                        let project = new_project.read(cx);
                        paths_vec
                            .iter()
                            .map(|path| project.resolve_abs_path(&path.to_string_lossy(), cx))
                            .collect::<Vec<_>>()
                    });
                    let resolved = futures::future::join_all(resolve_tasks).await;
                    // `resolve_abs_path` returns `None` for both "definitely
                    // absent" and transport errors (it swallows the error via
                    // `log_err`). This is a weaker guarantee than the local
                    // `Ok(None)` check, but it matches how the rest of the
                    // codebase consumes this API.
                    let all_paths_missing =
                        !paths_vec.is_empty() && resolved.iter().all(|resolved| resolved.is_none());

                    if all_paths_missing {
                        project_group.path_list().paths().to_vec()
                    } else {
                        paths_vec
                    }
                } else {
                    paths_vec
                };

            let window_handle =
                window_handle.ok_or_else(|| anyhow::anyhow!("Window is not a MultiWorkspace"))?;

            open_remote_project_with_existing_connection(
                connection_options,
                new_project,
                effective_paths_vec,
                app_state,
                window_handle,
                provisional_project_group_key,
                source_workspace,
                cx,
            )
            .await?;

            window_handle.update(cx, |multi_workspace, window, cx| {
                let workspace = multi_workspace.workspace().clone();
                multi_workspace.add(workspace.clone(), window, cx);
                workspace
            })
        })
    }

    /// Finds an existing workspace in this multi-workspace whose paths match,
    /// or creates a new one (deserializing its saved state from the database).
    /// Never searches other windows or matches workspaces with a superset of
    /// the requested paths.
    ///
    /// `excluding` lists workspaces that should be skipped during the search
    /// (e.g. workspaces that are about to be removed).
    pub fn find_or_create_local_workspace(
        &mut self,
        path_list: PathList,
        project_group: Option<ProjectGroupKey>,
        excluding: &[Entity<Workspace>],
        init: Option<Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + Send>>,
        open_mode: OpenMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Workspace>>> {
        self.find_or_create_local_workspace_with_source_workspace(
            path_list,
            project_group,
            excluding,
            init,
            open_mode,
            None,
            window,
            cx,
        )
    }

    pub fn find_or_create_local_workspace_with_source_workspace(
        &mut self,
        path_list: PathList,
        project_group: Option<ProjectGroupKey>,
        excluding: &[Entity<Workspace>],
        init: Option<Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + Send>>,
        open_mode: OpenMode,
        source_workspace: Option<WeakEntity<Workspace>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Workspace>>> {
        if let Some(workspace) = self.workspace_for_paths_excluding(&path_list, None, excluding, cx)
        {
            self.activate(workspace.clone(), source_workspace, window, cx);
            return Task::ready(Ok(workspace));
        }

        let paths = path_list.paths().to_vec();
        let app_state = self.workspace().read(cx).app_state().clone();
        let requesting_window = window.window_handle().downcast::<MultiWorkspace>();
        let fs = <dyn Fs>::global(cx);
        let excluding = excluding.to_vec();

        cx.spawn(async move |_this, cx| {
            let effective_path_list = if let Some(project_group) = project_group {
                let metadata_tasks: Vec<_> = paths
                    .iter()
                    .map(|path| fs.metadata(path.as_path()))
                    .collect();
                let metadata_results = futures::future::join_all(metadata_tasks).await;
                // Only fall back when every path is definitely absent; real
                // filesystem errors should not be treated as "missing".
                let all_paths_missing = !paths.is_empty()
                    && metadata_results
                        .into_iter()
                        // Ok(None) means the path is definitely absent
                        .all(|result| matches!(result, Ok(None)));

                if all_paths_missing {
                    project_group.path_list().clone()
                } else {
                    PathList::new(&paths)
                }
            } else {
                PathList::new(&paths)
            };

            if let Some(requesting_window) = requesting_window
                && let Some(workspace) = requesting_window
                    .update(cx, |multi_workspace, window, cx| {
                        multi_workspace
                            .workspace_for_paths_excluding(
                                &effective_path_list,
                                None,
                                &excluding,
                                cx,
                            )
                            .inspect(|workspace| {
                                multi_workspace.activate(
                                    workspace.clone(),
                                    source_workspace.clone(),
                                    window,
                                    cx,
                                );
                            })
                    })
                    .ok()
                    .flatten()
            {
                return Ok(workspace);
            }

            let result = cx
                .update(|cx| {
                    Workspace::new_local(
                        effective_path_list.paths().to_vec(),
                        app_state,
                        requesting_window,
                        None,
                        init,
                        open_mode,
                        cx,
                    )
                })
                .await?;
            Ok(result.workspace)
        })
    }
}
