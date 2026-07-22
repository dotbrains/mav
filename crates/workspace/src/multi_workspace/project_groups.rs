use super::*;

impl MultiWorkspace {
    fn subscribe_to_workspace(
        workspace: &Entity<Workspace>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let project = workspace.read(cx).project().clone();
        cx.subscribe_in(&project, window, {
            let workspace = workspace.downgrade();
            move |this, _project, event, _window, cx| match event {
                project::Event::WorktreePathsChanged { old_worktree_paths } => {
                    if let Some(workspace) = workspace.upgrade() {
                        let host = workspace
                            .read(cx)
                            .project()
                            .read(cx)
                            .remote_connection_options(cx);
                        let old_key =
                            ProjectGroupKey::from_worktree_paths(old_worktree_paths, host);
                        this.handle_project_group_key_change(&workspace, &old_key, cx);
                    }
                }
                _ => {}
            }
        })
        .detach();

        cx.subscribe_in(workspace, window, |this, workspace, event, window, cx| {
            if let WorkspaceEvent::Activate = event {
                this.activate(workspace.clone(), None, window, cx);
            }
        })
        .detach();
    }

    fn handle_project_group_key_change(
        &mut self,
        workspace: &Entity<Workspace>,
        old_key: &ProjectGroupKey,
        cx: &mut Context<Self>,
    ) {
        if !self.is_workspace_retained(workspace) {
            return;
        }

        let new_key = workspace.read(cx).project_group_key(cx);
        if new_key.path_list().paths().is_empty() {
            return;
        }

        // The Project already emitted WorktreePathsChanged which the
        // sidebar handles for thread migration.
        self.rekey_project_group(old_key, &new_key, cx);
        self.serialize(cx);
        cx.notify();
    }

    pub fn is_workspace_retained(&self, workspace: &Entity<Workspace>) -> bool {
        self.retained_workspaces
            .iter()
            .any(|retained| retained == workspace)
    }

    pub fn active_workspace_is_retained(&self) -> bool {
        self.is_workspace_retained(&self.active_workspace)
    }

    pub fn retained_workspaces(&self) -> &[Entity<Workspace>] {
        &self.retained_workspaces
    }

    /// Ensures a project group exists for `key`, creating one if needed.
    fn ensure_project_group_state(&mut self, key: ProjectGroupKey) {
        if key.path_list().paths().is_empty() {
            return;
        }

        if self.project_groups.iter().any(|group| group.key == key) {
            return;
        }

        self.project_groups.insert(
            0,
            ProjectGroupState {
                key,
                expanded: true,
                last_active_workspace: None,
            },
        );
    }

    /// Transitions a project group from `old_key` to `new_key`.
    ///
    /// On collision (both keys have groups), the active workspace's
    /// Re-keys a project group from `old_key` to `new_key`, handling
    /// collisions. When two groups collide, the active workspace's
    /// group always wins. Otherwise the old key's state is preserved
    /// — it represents the group the user or system just acted on.
    /// The losing group is removed, and the winner is re-keyed in
    /// place to preserve sidebar order.
    fn rekey_project_group(
        &mut self,
        old_key: &ProjectGroupKey,
        new_key: &ProjectGroupKey,
        cx: &App,
    ) {
        if old_key == new_key {
            return;
        }

        if new_key.path_list().paths().is_empty() {
            return;
        }

        let old_key_exists = self.project_groups.iter().any(|g| g.key == *old_key);
        let new_key_exists = self.project_groups.iter().any(|g| g.key == *new_key);

        if !old_key_exists {
            self.ensure_project_group_state(new_key.clone());
            return;
        }

        if new_key_exists {
            let active_key = self.active_workspace.read(cx).project_group_key(cx);
            if active_key == *new_key {
                self.project_groups.retain(|g| g.key != *old_key);
            } else {
                self.project_groups.retain(|g| g.key != *new_key);
                if let Some(group) = self.project_groups.iter_mut().find(|g| g.key == *old_key) {
                    group.key = new_key.clone();
                }
            }
        } else {
            if let Some(group) = self.project_groups.iter_mut().find(|g| g.key == *old_key) {
                group.key = new_key.clone();
            }
        }

        // If another retained workspace still has the old key (e.g. a
        // linked worktree workspace), re-create the old group so it
        // remains reachable in the sidebar.
        let other_workspace_needs_old_key = self
            .retained_workspaces
            .iter()
            .any(|ws| ws.read(cx).project_group_key(cx) == *old_key);
        if other_workspace_needs_old_key {
            self.ensure_project_group_state(old_key.clone());
        }
    }

    pub(crate) fn retain_workspace(
        &mut self,
        workspace: Entity<Workspace>,
        key: ProjectGroupKey,
        cx: &mut Context<Self>,
    ) {
        self.ensure_project_group_state(key);
        if self.is_workspace_retained(&workspace) {
            return;
        }

        self.retained_workspaces.push(workspace.clone());
        cx.emit(MultiWorkspaceEvent::WorkspaceAdded(workspace));
    }

    pub(crate) fn activate_provisional_workspace(
        &mut self,
        workspace: Entity<Workspace>,
        provisional_key: ProjectGroupKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if workspace != self.active_workspace {
            self.register_workspace(&workspace, window, cx);
        }

        self.ensure_project_group_state(provisional_key);
        if !self.is_workspace_retained(&workspace) {
            self.retained_workspaces.push(workspace.clone());
        }

        self.activate(workspace.clone(), None, window, cx);
        cx.emit(MultiWorkspaceEvent::WorkspaceAdded(workspace));
    }

    fn register_workspace(
        &mut self,
        workspace: &Entity<Workspace>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        Self::subscribe_to_workspace(workspace, window, cx);
        let weak_self = cx.weak_entity();
        let active_workspace_id = self.active_workspace_id.clone();
        workspace.update(cx, |workspace, cx| {
            workspace.set_multi_workspace(weak_self, active_workspace_id, cx);
        });

        let entity = cx.entity();
        cx.defer({
            let workspace = workspace.clone();
            move |cx| {
                entity.update(cx, |this, cx| {
                    this.sync_sidebar_to_workspace(&workspace, cx);
                })
            }
        });
    }

    pub fn project_group_key_for_workspace(
        &self,
        workspace: &Entity<Workspace>,
        cx: &App,
    ) -> ProjectGroupKey {
        workspace.read(cx).project_group_key(cx)
    }

    pub fn restore_project_groups(
        &mut self,
        groups: Vec<SerializedProjectGroupState>,
        _cx: &mut Context<Self>,
    ) {
        let mut restored: Vec<ProjectGroupState> = Vec::new();
        for SerializedProjectGroupState { key, expanded } in groups {
            if key.path_list().paths().is_empty() {
                continue;
            }
            if restored.iter().any(|group| group.key == key) {
                continue;
            }
            restored.push(ProjectGroupState {
                key,
                expanded,
                last_active_workspace: None,
            });
        }
        for existing in std::mem::take(&mut self.project_groups) {
            if !restored.iter().any(|group| group.key == existing.key) {
                restored.push(existing);
            }
        }
        self.project_groups = restored;
    }

    pub fn project_group_keys(&self) -> Vec<ProjectGroupKey> {
        self.project_groups
            .iter()
            .map(|group| group.key.clone())
            .collect()
    }
}
