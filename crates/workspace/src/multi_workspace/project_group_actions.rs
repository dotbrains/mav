use super::*;

impl MultiWorkspace {
    fn derived_project_groups(&self, cx: &App) -> Vec<ProjectGroup> {
        self.project_groups
            .iter()
            .map(|group| ProjectGroup {
                key: group.key.clone(),
                workspaces: self
                    .retained_workspaces
                    .iter()
                    .filter(|workspace| workspace.read(cx).project_group_key(cx) == group.key)
                    .cloned()
                    .collect(),
                expanded: group.expanded,
            })
            .collect()
    }

    pub fn project_groups(&self, cx: &App) -> Vec<ProjectGroup> {
        self.derived_project_groups(cx)
    }

    pub fn last_active_workspace_for_group(
        &self,
        key: &ProjectGroupKey,
        cx: &App,
    ) -> Option<Entity<Workspace>> {
        let group = self.project_groups.iter().find(|g| g.key == *key)?;
        let weak = group.last_active_workspace.as_ref()?;
        let workspace = weak.upgrade()?;
        (workspace.read(cx).project_group_key(cx) == *key).then_some(workspace)
    }

    pub fn group_state_by_key(&self, key: &ProjectGroupKey) -> Option<&ProjectGroupState> {
        self.project_groups.iter().find(|group| group.key == *key)
    }

    pub fn group_state_by_key_mut(
        &mut self,
        key: &ProjectGroupKey,
    ) -> Option<&mut ProjectGroupState> {
        self.project_groups
            .iter_mut()
            .find(|group| group.key == *key)
    }

    pub fn set_all_groups_expanded(&mut self, expanded: bool) {
        for group in &mut self.project_groups {
            group.expanded = expanded;
        }
    }

    pub fn move_project_group_up(&mut self, key: &ProjectGroupKey, cx: &mut Context<Self>) -> bool {
        let Some(index) = self
            .project_groups
            .iter()
            .position(|group| group.key == *key)
        else {
            return false;
        };
        if index == 0 {
            return false;
        }
        self.project_groups.swap(index - 1, index);
        cx.emit(MultiWorkspaceEvent::ProjectGroupsChanged);
        self.serialize(cx);
        cx.notify();
        true
    }

    pub fn move_project_group_down(
        &mut self,
        key: &ProjectGroupKey,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self
            .project_groups
            .iter()
            .position(|group| group.key == *key)
        else {
            return false;
        };
        if index + 1 >= self.project_groups.len() {
            return false;
        }
        self.project_groups.swap(index, index + 1);
        cx.emit(MultiWorkspaceEvent::ProjectGroupsChanged);
        self.serialize(cx);
        cx.notify();
        true
    }

    pub fn workspaces_for_project_group(
        &self,
        key: &ProjectGroupKey,
        cx: &App,
    ) -> Option<Vec<Entity<Workspace>>> {
        let has_group = self.project_groups.iter().any(|group| group.key == *key)
            || self
                .retained_workspaces
                .iter()
                .any(|workspace| workspace.read(cx).project_group_key(cx) == *key);

        has_group.then(|| {
            self.retained_workspaces
                .iter()
                .filter(|workspace| workspace.read(cx).project_group_key(cx) == *key)
                .cloned()
                .collect()
        })
    }

    pub fn close_workspace(
        &mut self,
        workspace: &Entity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        let group_key = workspace.read(cx).project_group_key(cx);
        let excluded_workspace = workspace.clone();

        self.remove(
            [workspace.clone()],
            move |this, window, cx| {
                if let Some(workspace) = this
                    .workspaces_for_project_group(&group_key, cx)
                    .unwrap_or_default()
                    .into_iter()
                    .find(|candidate| candidate != &excluded_workspace)
                {
                    return Task::ready(Ok(workspace));
                }

                let current_group_index = this
                    .project_groups
                    .iter()
                    .position(|group| group.key == group_key);

                if let Some(current_group_index) = current_group_index {
                    for distance in 1..this.project_groups.len() {
                        for neighboring_index in [
                            current_group_index.checked_add(distance),
                            current_group_index.checked_sub(distance),
                        ]
                        .into_iter()
                        .flatten()
                        {
                            let Some(neighboring_group) =
                                this.project_groups.get(neighboring_index)
                            else {
                                continue;
                            };

                            if let Some(workspace) = this
                                .last_active_workspace_for_group(&neighboring_group.key, cx)
                                .or_else(|| {
                                    this.workspaces_for_project_group(&neighboring_group.key, cx)
                                        .unwrap_or_default()
                                        .into_iter()
                                        .find(|candidate| candidate != &excluded_workspace)
                                })
                            {
                                return Task::ready(Ok(workspace));
                            }
                        }
                    }
                }

                let neighboring_group_key = current_group_index.and_then(|index| {
                    this.project_groups
                        .get(index + 1)
                        .or_else(|| {
                            index
                                .checked_sub(1)
                                .and_then(|previous| this.project_groups.get(previous))
                        })
                        .map(|group| group.key.clone())
                });

                if let Some(neighboring_group_key) = neighboring_group_key
                    && neighboring_group_key.host().is_none()
                {
                    return this.find_or_create_local_workspace(
                        neighboring_group_key.path_list().clone(),
                        Some(neighboring_group_key),
                        std::slice::from_ref(&excluded_workspace),
                        None,
                        OpenMode::Activate,
                        window,
                        cx,
                    );
                }

                let app_state = this.workspace().read(cx).app_state().clone();
                let project = Project::local(
                    app_state.client.clone(),
                    app_state.node_runtime.clone(),
                    app_state.user_store.clone(),
                    app_state.languages.clone(),
                    app_state.fs.clone(),
                    None,
                    project::LocalProjectFlags::default(),
                    cx,
                );
                let new_workspace =
                    cx.new(|cx| Workspace::new(None, project, app_state, window, cx));
                Task::ready(Ok(new_workspace))
            },
            window,
            cx,
        )
    }

    pub fn remove_project_group(
        &mut self,
        group_key: &ProjectGroupKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        let pos = self
            .project_groups
            .iter()
            .position(|group| group.key == *group_key);
        let workspaces = self
            .workspaces_for_project_group(group_key, cx)
            .unwrap_or_default();

        // Compute the neighbor while the group is still in the list.
        let neighbor_key = pos.and_then(|pos| {
            self.project_groups
                .get(pos + 1)
                .or_else(|| pos.checked_sub(1).and_then(|i| self.project_groups.get(i)))
                .map(|group| group.key.clone())
        });

        // Now remove the group.
        self.project_groups.retain(|group| group.key != *group_key);
        cx.emit(MultiWorkspaceEvent::ProjectGroupsChanged);

        let excluded_workspaces = workspaces.clone();
        self.remove(
            workspaces,
            move |this, window, cx| {
                if let Some(neighbor_key) = neighbor_key
                    && neighbor_key.host().is_none()
                {
                    return this.find_or_create_local_workspace(
                        neighbor_key.path_list().clone(),
                        Some(neighbor_key.clone()),
                        &excluded_workspaces,
                        None,
                        OpenMode::Activate,
                        window,
                        cx,
                    );
                }

                // No other project groups remain — create an empty workspace.
                let app_state = this.workspace().read(cx).app_state().clone();
                let project = Project::local(
                    app_state.client.clone(),
                    app_state.node_runtime.clone(),
                    app_state.user_store.clone(),
                    app_state.languages.clone(),
                    app_state.fs.clone(),
                    None,
                    project::LocalProjectFlags::default(),
                    cx,
                );
                let new_workspace =
                    cx.new(|cx| Workspace::new(None, project, app_state, window, cx));
                Task::ready(Ok(new_workspace))
            },
            window,
            cx,
        )
    }

    /// Goes through sqlite: serialize -> close -> open new window
    /// This avoids issues with pending tasks having the wrong window
    pub fn open_project_group_in_new_window(
        &mut self,
        key: &ProjectGroupKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let paths: Vec<PathBuf> = key.path_list().ordered_paths().cloned().collect();
        if paths.is_empty() {
            return Task::ready(Ok(()));
        }

        let app_state = self.workspace().read(cx).app_state().clone();

        let workspaces: Vec<_> = self
            .workspaces_for_project_group(key, cx)
            .unwrap_or_default();
        let mut serialization_tasks = Vec::new();
        for workspace in &workspaces {
            serialization_tasks.push(workspace.update(cx, |workspace, inner_cx| {
                workspace.flush_serialization(window, inner_cx)
            }));
        }

        let remove_task = self.remove_project_group(key, window, cx);

        cx.spawn(async move |_this, cx| {
            futures::future::join_all(serialization_tasks).await;

            let removed = remove_task.await?;
            if !removed {
                return Ok(());
            }

            cx.update(|cx| {
                Workspace::new_local(paths, app_state, None, None, None, OpenMode::NewWindow, cx)
            })
            .await?;

            Ok(())
        })
    }
}
