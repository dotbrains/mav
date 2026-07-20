use super::*;

impl Workspace {
    pub fn database_id(&self) -> Option<WorkspaceId> {
        self.database_id
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn set_database_id(&mut self, id: WorkspaceId) {
        self.database_id = Some(id);
    }

    pub fn session_id(&self) -> Option<String> {
        self.session_id.clone()
    }

    pub(crate) fn save_window_bounds(&self, window: &mut Window, cx: &mut App) -> Task<()> {
        let Some(display) = window.display(cx) else {
            return Task::ready(());
        };
        let Ok(display_uuid) = display.uuid() else {
            return Task::ready(());
        };

        let window_bounds = window.inner_window_bounds();
        let database_id = self.database_id;
        let has_paths = !self.root_paths(cx).is_empty();
        let db = WorkspaceDb::global(cx);
        let kvp = db::kvp::KeyValueStore::global(cx);

        cx.background_executor().spawn(async move {
            if !has_paths {
                persistence::write_default_window_bounds(&kvp, window_bounds, display_uuid)
                    .await
                    .log_err();
            }
            if let Some(database_id) = database_id {
                db.set_window_open_status(
                    database_id,
                    SerializedWindowBounds(window_bounds),
                    display_uuid,
                )
                .await
                .log_err();
            } else {
                persistence::write_default_window_bounds(&kvp, window_bounds, display_uuid)
                    .await
                    .log_err();
            }
        })
    }

    /// Bypass the 200ms serialization throttle and write workspace state to
    /// the DB immediately. Returns a task the caller can await to ensure the
    /// write completes. Used by the quit handler so the most recent state
    /// isn't lost to a pending throttle timer when the process exits.
    pub fn flush_serialization(&mut self, window: &mut Window, cx: &mut App) -> Task<()> {
        self._schedule_serialize_workspace.take();
        self._serialize_workspace_task.take();
        self.bounds_save_task_queued.take();

        let bounds_task = self.save_window_bounds(window, cx);
        let serialize_task = self.serialize_workspace_internal(window, cx);
        cx.spawn(async move |_| {
            bounds_task.await;
            serialize_task.await;
        })
    }

    pub fn root_paths(&self, cx: &App) -> Vec<Arc<Path>> {
        let project = self.project().read(cx);
        project
            .visible_worktrees(cx)
            .map(|worktree| worktree.read(cx).abs_path())
            .collect::<Vec<_>>()
    }

    pub(crate) fn remove_panes(
        &mut self,
        member: Member,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        match member {
            Member::Axis(PaneAxis { members, .. }) => {
                for child in members.iter() {
                    self.remove_panes(child.clone(), window, cx)
                }
            }
            Member::Pane(pane) => {
                self.force_remove_pane(&pane, &None, window, cx);
            }
        }
    }

    pub(crate) fn remove_from_session(&mut self, window: &mut Window, cx: &mut App) -> Task<()> {
        self.session_id.take();
        self.serialize_workspace_internal(window, cx)
    }

    pub(crate) fn force_remove_pane(
        &mut self,
        pane: &Entity<Pane>,
        focus_on: &Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let removing_active_pane = self.active_pane() == pane;
        self.panes.retain(|p| p != pane);
        if let Some(focus_on) = focus_on {
            if removing_active_pane {
                self.set_active_pane(focus_on, window, cx);
            }
            focus_on.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
        } else if removing_active_pane {
            let fallback_pane = self.panes.last().unwrap().clone();
            self.set_active_pane(&fallback_pane, window, cx);
            if !self.has_active_modal(window, cx) {
                fallback_pane.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
            }
        }
        if self.last_active_center_pane == Some(pane.downgrade()) {
            self.last_active_center_pane = None;
        }
        cx.notify();
    }

    pub(crate) fn serialize_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self._schedule_serialize_workspace.is_none() {
            self._schedule_serialize_workspace =
                Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor()
                        .timer(SERIALIZATION_THROTTLE_TIME)
                        .await;
                    this.update_in(cx, |this, window, cx| {
                        this._serialize_workspace_task =
                            Some(this.serialize_workspace_internal(window, cx));
                        this._schedule_serialize_workspace.take();
                    })
                    .log_err();
                }));
        }
    }

    pub(crate) fn serialize_workspace_internal(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<()> {
        let Some(database_id) = self.database_id() else {
            return Task::ready(());
        };

        fn serialize_pane_handle(
            pane_handle: &Entity<Pane>,
            window: &mut Window,
            cx: &mut App,
        ) -> SerializedPane {
            let (items, active, pinned_count, pane_kind, visible) = {
                let pane = pane_handle.read(cx);
                let active_item_id = pane.active_item().map(|item| item.item_id());
                (
                    pane.items()
                        .filter_map(|handle| {
                            let handle = handle.to_serializable_item_handle(cx)?;

                            Some(SerializedItem {
                                kind: Arc::from(handle.serialized_item_kind()),
                                item_id: handle.item_id().as_u64(),
                                active: Some(handle.item_id()) == active_item_id,
                                preview: pane.is_active_preview_item(handle.item_id()),
                            })
                        })
                        .collect::<Vec<_>>(),
                    pane.has_focus(window, cx),
                    pane.pinned_count(),
                    pane.pane_kind(),
                    pane.is_visible(),
                )
            };

            if pane_kind.is_tabbed() {
                SerializedPane::new(items, active, pinned_count).with_visible(visible)
            } else {
                SerializedPane::new_with_kind(items, active, pinned_count, pane_kind)
                    .with_visible(visible)
            }
        }

        fn build_serialized_pane_group(
            pane_group: &Member,
            window: &mut Window,
            cx: &mut App,
        ) -> Option<SerializedPaneGroup> {
            match pane_group {
                Member::Axis(PaneAxis {
                    axis,
                    members,
                    flexes,
                    bounding_boxes: _,
                }) => {
                    let children = members
                        .iter()
                        .filter_map(|member| build_serialized_pane_group(member, window, cx))
                        .collect::<Vec<_>>();

                    match children.len() {
                        0 => None,
                        1 => children.into_iter().next(),
                        _ => Some(SerializedPaneGroup::Group {
                            axis: SerializedAxis(*axis),
                            flexes: Some(flexes.lock().clone()),
                            children,
                        }),
                    }
                }
                Member::Pane(pane_handle) => Some(SerializedPaneGroup::Pane(
                    serialize_pane_handle(pane_handle, window, cx),
                )),
            }
        }

        fn build_serialized_docks(
            this: &Workspace,
            window: &mut Window,
            cx: &mut App,
        ) -> DockStructure {
            this.capture_dock_state(window, cx)
        }

        match self.workspace_location(cx) {
            WorkspaceLocation::Location(location, paths) => {
                let bookmarks = self.project.update(cx, |project, cx| {
                    project
                        .bookmark_store()
                        .read(cx)
                        .all_serialized_bookmarks(cx)
                });

                let breakpoints = self.project.update(cx, |project, cx| {
                    project
                        .breakpoint_store()
                        .read(cx)
                        .all_source_breakpoints(cx)
                });
                let user_toolchains = self
                    .project
                    .read(cx)
                    .user_toolchains(cx)
                    .unwrap_or_default();

                let center_group = build_serialized_pane_group(&self.center.root, window, cx)
                    .unwrap_or_else(|| SerializedPaneGroup::Pane(SerializedPane::default()));
                let docks = build_serialized_docks(self, window, cx);
                let window_bounds = Some(SerializedWindowBounds(window.window_bounds()));
                let identity_paths_hint = self.project_group_key(cx).path_list().clone();

                let serialized_workspace = SerializedWorkspace {
                    id: database_id,
                    location,
                    paths,
                    identity_paths: Some(identity_paths_hint),
                    center_group,
                    window_bounds,
                    display: Default::default(),
                    docks,
                    centered_layout: self.centered_layout,
                    session_id: self.session_id.clone(),
                    bookmarks,
                    breakpoints,
                    window_id: Some(window.window_handle().window_id().as_u64()),
                    user_toolchains,
                };

                let db = WorkspaceDb::global(cx);
                window.spawn(cx, async move |_| {
                    db.save_workspace(serialized_workspace).await;
                })
            }
            WorkspaceLocation::DetachFromSession => {
                let window_bounds = SerializedWindowBounds(window.window_bounds());
                let display = window.display(cx).and_then(|d| d.uuid().ok());
                // Save dock state for empty local workspaces
                let docks = build_serialized_docks(self, window, cx);
                let db = WorkspaceDb::global(cx);
                let kvp = db::kvp::KeyValueStore::global(cx);
                window.spawn(cx, async move |_| {
                    db.set_window_open_status(
                        database_id,
                        window_bounds,
                        display.unwrap_or_default(),
                    )
                    .await
                    .log_err();
                    db.set_session_id(database_id, None).await.log_err();
                    persistence::write_default_dock_state(&kvp, docks)
                        .await
                        .log_err();
                })
            }
            WorkspaceLocation::None => {
                // Save dock state for empty non-local workspaces
                let docks = build_serialized_docks(self, window, cx);
                let kvp = db::kvp::KeyValueStore::global(cx);
                window.spawn(cx, async move |_| {
                    persistence::write_default_dock_state(&kvp, docks)
                        .await
                        .log_err();
                })
            }
        }
    }

    pub(crate) fn has_any_items_open(&self, cx: &App) -> bool {
        self.panes
            .iter()
            .any(|pane| pane.read(cx).is_tabbed() && pane.read(cx).items_len() > 0)
    }

    pub(crate) fn workspace_location(&self, cx: &App) -> WorkspaceLocation {
        let paths = PathList::new(&self.root_paths(cx));
        if let Some(connection) = self.project.read(cx).remote_connection_options(cx) {
            WorkspaceLocation::Location(SerializedWorkspaceLocation::Remote(connection), paths)
        } else if self.project.read(cx).is_local() {
            if !paths.is_empty() || self.has_any_items_open(cx) {
                WorkspaceLocation::Location(SerializedWorkspaceLocation::Local, paths)
            } else {
                WorkspaceLocation::DetachFromSession
            }
        } else {
            WorkspaceLocation::None
        }
    }

    pub(crate) fn update_history(&self, cx: &mut App) {
        let Some(id) = self.database_id() else {
            return;
        };
        if !self.project.read(cx).is_local() {
            return;
        }
        if let Some(manager) = HistoryManager::global(cx) {
            let paths = PathList::new(&self.root_paths(cx));
            manager.update(cx, |this, cx| {
                this.update_history(id, HistoryManagerEntry::new(id, &paths), cx);
            });
        }
    }
}
