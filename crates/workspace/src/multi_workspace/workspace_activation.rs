use super::*;

impl MultiWorkspace {
    pub fn workspace(&self) -> &Entity<Workspace> {
        &self.active_workspace
    }

    pub fn workspaces(&self) -> impl Iterator<Item = &Entity<Workspace>> {
        let active_is_retained = self.is_workspace_retained(&self.active_workspace);
        self.retained_workspaces
            .iter()
            .chain(std::iter::once(&self.active_workspace).filter(move |_| !active_is_retained))
    }

    /// Adds a workspace to this window as persistent without changing which
    /// workspace is active. Unlike `activate()`, this always inserts into the
    /// persistent list regardless of sidebar state — it's used for system-
    /// initiated additions like deserialization and worktree discovery.
    pub fn add(&mut self, workspace: Entity<Workspace>, window: &Window, cx: &mut Context<Self>) {
        if self.is_workspace_retained(&workspace) {
            return;
        }

        if workspace != self.active_workspace {
            self.register_workspace(&workspace, window, cx);
        }

        let key = workspace.read(cx).project_group_key(cx);
        self.retain_workspace(workspace, key, cx);
        telemetry::event!(
            "Workspace Added",
            workspace_count = self.retained_workspaces.len()
        );
        cx.notify();
    }

    /// Ensures the workspace is in the multiworkspace and makes it the active one.
    pub fn activate(
        &mut self,
        workspace: Entity<Workspace>,
        source_workspace: Option<WeakEntity<Workspace>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace() == &workspace {
            self.focus_active_workspace(window, cx);
            return;
        }

        let old_active_workspace = self.active_workspace.clone();
        let old_active_was_retained = self.active_workspace_is_retained();
        let workspace_was_retained = self.is_workspace_retained(&workspace);
        let should_retain_workspaces = self.multi_workspace_enabled(cx);

        if should_retain_workspaces && !old_active_was_retained {
            let key = old_active_workspace.read(cx).project_group_key(cx);
            self.retain_workspace(old_active_workspace.clone(), key, cx);
        }

        if !workspace_was_retained {
            self.register_workspace(&workspace, window, cx);

            if should_retain_workspaces {
                let key = workspace.read(cx).project_group_key(cx);
                self.retain_workspace(workspace.clone(), key, cx);
            }
        }

        self.active_workspace = workspace;
        // Publish the new active workspace before anyone reads the shared cell
        // to decide who owns the window chrome.
        self.active_workspace_id
            .set(self.active_workspace.entity_id());

        let active_key = self.active_workspace.read(cx).project_group_key(cx);
        if let Some(group) = self.project_groups.iter_mut().find(|g| g.key == active_key) {
            group.last_active_workspace = Some(self.active_workspace.downgrade());
        }

        if !should_retain_workspaces && !old_active_was_retained {
            self.detach_workspace(&old_active_workspace, cx);
        }

        // The platform window is shared across all workspaces in this window.
        // The previously-active workspace left the title and edited indicator
        // reflecting its own state, so re-apply them from the newly-active
        // workspace (which is now the chrome owner per `owns_window_chrome`).
        self.active_workspace.update(cx, |workspace, cx| {
            workspace.refresh_window_state(window, cx);
        });

        cx.emit(MultiWorkspaceEvent::ActiveWorkspaceChanged { source_workspace });
        self.serialize(cx);
        self.focus_active_workspace(window, cx);
        cx.notify();
    }

    /// Adds `workspace` as a retained background tab without switching the
    /// active workspace to it or moving focus. Mirrors the registration and
    /// retention bookkeeping `activate` performs for the incoming workspace,
    /// but leaves the currently-active workspace focused.
    ///
    /// Used when something opens a workspace the user should not be yanked
    /// into — e.g. the agent's `create_thread` tool spawning a sibling
    /// worktree in the background.
    pub fn add_background_workspace(
        &mut self,
        workspace: Entity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace() == &workspace || self.is_workspace_retained(&workspace) {
            return;
        }
        self.register_workspace(&workspace, window, cx);
        let key = workspace.read(cx).project_group_key(cx);
        self.retain_workspace(workspace, key, cx);
        cx.notify();
    }

    /// Promotes the currently active workspace to persistent if it is
    /// transient, so it is retained across workspace switches even when
    /// the sidebar is closed. No-op if the workspace is already persistent.
    pub fn retain_active_workspace(&mut self, cx: &mut Context<Self>) {
        if self.retain_active_workspace_without_serializing(cx) {
            self.serialize(cx);
            cx.notify();
        }
    }
}
