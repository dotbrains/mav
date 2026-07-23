use super::*;

impl MultiWorkspace {
    pub(super) fn app_will_quit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl Future<Output = ()> + use<> {
        self.serialize(cx);
        let mut tasks: Vec<Task<()>> = Vec::new();
        if let Some(task) = self._serialize_task.take() {
            tasks.push(task);
        }
        tasks.extend(std::mem::take(&mut self.pending_removal_tasks));

        async move {
            futures::future::join_all(tasks).await;
        }
    }
    pub fn focus_active_workspace(&self, window: &mut Window, cx: &mut App) {
        // If a dock panel is zoomed, focus it instead of the center pane.
        // Otherwise, focusing the center pane triggers dismiss_zoomed_items_to_reveal
        // which closes the zoomed dock.
        let focus_handle = {
            let workspace = self.workspace().read(cx);
            let mut target = None;
            for dock in workspace.all_docks() {
                let dock = dock.read(cx);
                if dock.is_open() {
                    if let Some(panel) = dock.active_panel() {
                        if panel.is_zoomed(window, cx) {
                            target = Some(panel.panel_focus_handle(cx));
                            break;
                        }
                    }
                }
            }
            target.unwrap_or_else(|| {
                let pane = workspace.active_pane().clone();
                pane.read(cx).focus_handle(cx)
            })
        };
        window.focus(&focus_handle, cx);
    }

    pub fn panel<T: Panel>(&self, cx: &App) -> Option<Entity<T>> {
        self.workspace().read(cx).panel::<T>(cx)
    }

    pub fn active_modal<V: ManagedView + 'static>(&self, cx: &App) -> Option<Entity<V>> {
        self.workspace().read(cx).active_modal::<V>(cx)
    }

    pub fn add_panel<T: Panel>(
        &mut self,
        panel: Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace().update(cx, |workspace, cx| {
            workspace.add_panel(panel, window, cx);
        });
    }

    pub fn focus_panel<T: Panel>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<T>> {
        self.workspace()
            .update(cx, |workspace, cx| workspace.focus_panel::<T>(window, cx))
    }

    // used in a test
    pub fn toggle_modal<V: ModalView, B>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: B,
    ) where
        B: FnOnce(&mut Window, &mut gpui::Context<V>) -> V,
    {
        self.workspace().update(cx, |workspace, cx| {
            workspace.toggle_modal(window, cx, build);
        });
    }

    pub fn toggle_dock(
        &mut self,
        dock_side: DockPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace().update(cx, |workspace, cx| {
            workspace.toggle_dock(dock_side, window, cx);
        });
    }

    pub fn active_item_as<I: 'static>(&self, cx: &App) -> Option<Entity<I>> {
        self.workspace().read(cx).active_item_as::<I>(cx)
    }

    pub fn items_of_type<'a, T: Item>(
        &'a self,
        cx: &'a App,
    ) -> impl 'a + Iterator<Item = Entity<T>> {
        self.workspace().read(cx).items_of_type::<T>(cx)
    }

    pub fn database_id(&self, cx: &App) -> Option<WorkspaceId> {
        self.workspace().read(cx).database_id()
    }

    pub fn take_pending_removal_tasks(&mut self) -> Vec<Task<()>> {
        let tasks: Vec<Task<()>> = std::mem::take(&mut self.pending_removal_tasks)
            .into_iter()
            .filter(|task| !task.is_ready())
            .collect();
        tasks
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_expand_all_groups(&mut self) {
        self.set_all_groups_expanded(true);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn assert_project_group_key_integrity(&self, cx: &App) -> anyhow::Result<()> {
        let mut retained_ids: collections::HashSet<EntityId> = Default::default();
        for workspace in &self.retained_workspaces {
            anyhow::ensure!(
                retained_ids.insert(workspace.entity_id()),
                "workspace {:?} is retained more than once",
                workspace.entity_id(),
            );

            let live_key = workspace.read(cx).project_group_key(cx);
            anyhow::ensure!(
                self.project_groups
                    .iter()
                    .any(|group| group.key == live_key),
                "workspace {:?} has live key {:?} but no project-group metadata",
                workspace.entity_id(),
                live_key,
            );
        }
        Ok(())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_random_database_id(&mut self, cx: &mut Context<Self>) {
        self.workspace().update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
        });
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_new(project: Entity<Project>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = cx.new(|cx| Workspace::test_new(project, window, cx));
        Self::new(workspace, window, cx)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_add_workspace(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Workspace> {
        let workspace = cx.new(|cx| Workspace::test_new(project, window, cx));
        self.activate(workspace.clone(), None, window, cx);
        workspace
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_add_project_group(&mut self, group: ProjectGroup) {
        self.project_groups.push(ProjectGroupState {
            key: group.key,
            expanded: group.expanded,
            last_active_workspace: None,
        });
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn create_test_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let app_state = self.workspace().read(cx).app_state().clone();
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
        let new_workspace = cx.new(|cx| Workspace::new(None, project, app_state, window, cx));
        self.activate(new_workspace.clone(), None, window, cx);

        let weak_workspace = new_workspace.downgrade();
        let db = crate::persistence::WorkspaceDb::global(cx);
        cx.spawn_in(window, async move |this, cx| {
            let workspace_id = db.next_id().await.unwrap();
            let workspace = weak_workspace.upgrade().unwrap();
            let task: Task<()> = this
                .update_in(cx, |this, window, cx| {
                    let session_id = workspace.read(cx).session_id();
                    let window_id = window.window_handle().window_id().as_u64();
                    workspace.update(cx, |workspace, _cx| {
                        workspace.set_database_id(workspace_id);
                    });
                    this.serialize(cx);
                    let db = db.clone();
                    cx.background_spawn(async move {
                        db.set_session_binding(workspace_id, session_id, Some(window_id))
                            .await
                            .log_err();
                    })
                })
                .unwrap();
            task.await
        })
    }

    /// Assigns random database IDs to all retained workspaces, flushes
    /// workspace serialization (SQLite) and multi-workspace state (KVP),
    /// and writes session bindings so the serialized data can be read
    /// back by `last_session_workspace_locations` +
    /// `read_serialized_multi_workspaces`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn flush_all_serialization(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Task<()>> {
        for workspace in self.workspaces() {
            workspace.update(cx, |ws, _cx| {
                if ws.database_id().is_none() {
                    ws.set_random_database_id();
                }
            });
        }

        let session_id = self.workspace().read(cx).session_id();
        let window_id_u64 = window.window_handle().window_id().as_u64();

        let mut tasks: Vec<Task<()>> = Vec::new();
        for workspace in self.workspaces() {
            tasks.push(workspace.update(cx, |ws, cx| ws.flush_serialization(window, cx)));
            if let Some(db_id) = workspace.read(cx).database_id() {
                let db = crate::persistence::WorkspaceDb::global(cx);
                let session_id = session_id.clone();
                tasks.push(cx.background_spawn(async move {
                    db.set_session_binding(db_id, session_id, Some(window_id_u64))
                        .await
                        .log_err();
                }));
            }
        }
        self.serialize(cx);
        tasks
    }

    /// Removes one or more workspaces from this multi-workspace.
    ///
    /// If the active workspace is among those being removed,
    /// `fallback_workspace` is called **synchronously before the removal
    /// begins** to produce a `Task` that resolves to the workspace that
    /// should become active. The fallback must not be one of the
    /// workspaces being removed.
    ///
    /// Returns `true` if any workspaces were actually removed.
    pub fn remove(
        &mut self,
        workspaces: impl IntoIterator<Item = Entity<Workspace>>,
        fallback_workspace: impl FnOnce(
            &mut Self,
            &mut Window,
            &mut Context<Self>,
        ) -> Task<Result<Entity<Workspace>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        let workspaces: Vec<_> = workspaces.into_iter().collect();

        if workspaces.is_empty() {
            return Task::ready(Ok(false));
        }

        let removing_active = workspaces.iter().any(|ws| ws == self.workspace());
        let original_active = self.workspace().clone();

        let fallback_task = removing_active.then(|| fallback_workspace(self, window, cx));

        cx.spawn_in(window, async move |this, cx| {
            // Run the standard workspace close lifecycle for every workspace
            // being removed from this window. This handles save prompting and
            // session cleanup consistently with other replace-in-window flows.
            for workspace in &workspaces {
                let should_continue = workspace
                    .update_in(cx, |workspace, window, cx| {
                        workspace.prepare_to_close(CloseIntent::ReplaceWindow, window, cx)
                    })?
                    .await?;

                if !should_continue {
                    return Ok(false);
                }
            }

            // If we're removing the active workspace, await the
            // fallback and switch to it before tearing anything down.
            // Otherwise restore the original active workspace in case
            // prompting switched away from it.
            if let Some(fallback_task) = fallback_task {
                let new_active = fallback_task.await?;

                this.update_in(cx, |this, window, cx| {
                    assert!(
                        !workspaces.contains(&new_active),
                        "fallback workspace must not be one of the workspaces being removed"
                    );
                    this.activate(new_active, None, window, cx);
                })?;
            } else {
                this.update_in(cx, |this, window, cx| {
                    if *this.workspace() != original_active {
                        this.activate(original_active, None, window, cx);
                    }
                })?;
            }

            // Actually remove the workspaces.
            this.update_in(cx, |this, _, cx| {
                let mut removed_any = false;

                for workspace in &workspaces {
                    let was_retained = this.is_workspace_retained(workspace);
                    if was_retained {
                        this.detach_workspace(workspace, cx);
                        removed_any = true;
                    }
                }

                if removed_any {
                    this.serialize(cx);
                    cx.notify();
                }

                Ok(removed_any)
            })?
        })
    }

    pub fn open_project(
        &mut self,
        paths: Vec<PathBuf>,
        open_mode: OpenMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Workspace>>> {
        if self.multi_workspace_enabled(cx) {
            let empty_workspace = if self
                .active_workspace
                .read(cx)
                .project()
                .read(cx)
                .visible_worktrees(cx)
                .next()
                .is_none()
            {
                Some(self.active_workspace.clone())
            } else {
                None
            };

            cx.spawn_in(window, async move |this, cx| {
                if let Some(empty_workspace) = empty_workspace.as_ref() {
                    let should_continue = empty_workspace
                        .update_in(cx, |workspace, window, cx| {
                            workspace.prepare_to_close(CloseIntent::ReplaceWindow, window, cx)
                        })?
                        .await?;
                    if !should_continue {
                        return Ok(empty_workspace.clone());
                    }
                }

                let create_task = this.update_in(cx, |this, window, cx| {
                    this.find_or_create_local_workspace(
                        PathList::new(&paths),
                        None,
                        empty_workspace.as_slice(),
                        None,
                        OpenMode::Activate,
                        window,
                        cx,
                    )
                })?;
                let new_workspace = create_task.await?;

                if let Some(empty_workspace) = empty_workspace {
                    this.update(cx, |this, cx| {
                        if this.is_workspace_retained(&empty_workspace) {
                            this.detach_workspace(&empty_workspace, cx);
                        }
                    })?;
                }

                Ok(new_workspace)
            })
        } else {
            let workspace = self.workspace().clone();
            cx.spawn_in(window, async move |_this, cx| {
                let should_continue = workspace
                    .update_in(cx, |workspace, window, cx| {
                        workspace.prepare_to_close(crate::CloseIntent::ReplaceWindow, window, cx)
                    })?
                    .await?;
                if should_continue {
                    workspace
                        .update_in(cx, |workspace, window, cx| {
                            workspace.open_workspace_for_paths(open_mode, paths, window, cx)
                        })?
                        .await
                } else {
                    Ok(workspace)
                }
            })
        }
    }
}
