use super::*;

impl MultiWorkspace {
    fn retain_active_workspace_without_serializing(&mut self, cx: &mut Context<Self>) -> bool {
        let workspace = self.active_workspace.clone();
        if self.is_workspace_retained(&workspace) {
            return false;
        }

        let key = workspace.read(cx).project_group_key(cx);
        self.retain_workspace(workspace, key, cx);
        true
    }

    /// Detaches a workspace: clears session state, DB binding, cached
    /// group key, and emits `WorkspaceRemoved`. The DB row is preserved
    /// so the workspace still appears in the recent-projects list.
    fn detach_workspace(&mut self, workspace: &Entity<Workspace>, cx: &mut Context<Self>) {
        self.retained_workspaces
            .retain(|retained| retained != workspace);
        for group in &mut self.project_groups {
            if group
                .last_active_workspace
                .as_ref()
                .and_then(WeakEntity::upgrade)
                .as_ref()
                == Some(workspace)
            {
                group.last_active_workspace = None;
            }
        }
        cx.emit(MultiWorkspaceEvent::WorkspaceRemoved(workspace.entity_id()));
        workspace.update(cx, |workspace, _cx| {
            workspace.session_id.take();
            workspace._schedule_serialize_workspace.take();
            workspace._serialize_workspace_task.take();
        });

        if let Some(workspace_id) = workspace.read(cx).database_id() {
            let db = crate::persistence::WorkspaceDb::global(cx);
            self.pending_removal_tasks.retain(|task| !task.is_ready());
            self.pending_removal_tasks
                .push(cx.background_spawn(async move {
                    db.set_session_binding(workspace_id, None, None)
                        .await
                        .log_err();
                }));
        }
    }

    fn sync_sidebar_to_workspace(&self, workspace: &Entity<Workspace>, cx: &mut Context<Self>) {
        if self.sidebar_open() {
            let sidebar_focus_handle = self.sidebar.as_ref().map(|s| s.focus_handle(cx));
            workspace.update(cx, |workspace, _| {
                workspace.set_sidebar_focus_handle(sidebar_focus_handle);
            });
        }
    }

    pub fn serialize(&mut self, cx: &mut Context<Self>) {
        self._serialize_task = Some(cx.spawn(async move |this, cx| {
            let Some((window_id, state)) = this
                .read_with(cx, |this, cx| {
                    let state = MultiWorkspaceState {
                        active_workspace_id: this.workspace().read(cx).database_id(),
                        project_groups: this
                            .project_groups
                            .iter()
                            .map(|group| {
                                crate::persistence::model::SerializedProjectGroup::from_group(
                                    &group.key,
                                    group.expanded,
                                )
                            })
                            .collect::<Vec<_>>(),
                        sidebar_open: this.sidebar_open,
                        sidebar_state: this
                            .sidebar
                            .as_ref()
                            .and_then(|s| s.serialized_state(cx))
                            .or_else(|| this.pending_sidebar_state.clone()),
                    };
                    (this.window_id, state)
                })
                .ok()
            else {
                return;
            };
            let kvp = cx.update(|cx| db::kvp::KeyValueStore::global(cx));
            crate::persistence::write_multi_workspace_state(&kvp, window_id, state).await;
        }));
    }

    /// Returns the in-flight serialization task (if any) so the caller can
    /// await it. Used by the quit handler to ensure pending DB writes
    /// complete before the process exits.
    pub fn flush_serialization(&mut self) -> Task<()> {
        self._serialize_task.take().unwrap_or(Task::ready(()))
    }
}
