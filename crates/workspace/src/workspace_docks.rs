use super::*;

impl Workspace {
    pub fn project_group_key(&self, cx: &App) -> ProjectGroupKey {
        self.project.read(cx).project_group_key(cx)
    }

    pub fn weak_handle(&self) -> WeakEntity<Self> {
        self.weak_self.clone()
    }

    pub fn left_dock(&self) -> &Entity<Dock> {
        &self.left_dock
    }

    pub fn right_dock(&self) -> &Entity<Dock> {
        &self.right_dock
    }

    pub fn all_docks(&self) -> [&Entity<Dock>; 2] {
        [&self.left_dock, &self.right_dock]
    }

    pub fn capture_dock_state(&self, _window: &Window, cx: &App) -> DockStructure {
        let active_hosted_panel_by_kind = [
            self.active_panel_for_pane_kind(PaneKind::Project, cx),
            self.active_panel_for_pane_kind(PaneKind::Agent, cx),
        ];

        let left_dock = self.left_dock.read(cx);
        let left_visible = left_dock.is_open();
        let left_active_panel =
            self.active_panel_name_for_dock(&left_dock, active_hosted_panel_by_kind.as_slice());
        // `zoomed_position` is kept in sync with individual panel zoom state
        // by the dock code in `Dock::new` and `Dock::add_panel`.
        let left_dock_zoom = self.zoomed_position == Some(DockPosition::Left);

        let right_dock = self.right_dock.read(cx);
        let right_visible = right_dock.is_open();
        let right_active_panel =
            self.active_panel_name_for_dock(&right_dock, active_hosted_panel_by_kind.as_slice());
        let right_dock_zoom = self.zoomed_position == Some(DockPosition::Right);

        DockStructure {
            left: DockData {
                visible: left_visible,
                active_panel: left_active_panel,
                zoom: left_dock_zoom,
            },
            right: DockData {
                visible: right_visible,
                active_panel: right_active_panel,
                zoom: right_dock_zoom,
            },
            bottom: DockData::default(),
        }
    }

    fn active_panel_for_pane_kind(
        &self,
        pane_kind: PaneKind,
        cx: &App,
    ) -> Option<Arc<dyn PanelHandle>> {
        self.center.panes().into_iter().find_map(|pane| {
            if pane.read(cx).pane_kind() != pane_kind {
                return None;
            }

            let item = pane.read(cx).active_item()?;
            let panel_item = item.downcast::<PanelItem>()?;
            Some(panel_item.read(cx).panel())
        })
    }

    fn active_panel_name_for_dock(
        &self,
        dock: &Dock,
        active_hosted_panels: &[Option<Arc<dyn PanelHandle>>],
    ) -> Option<String> {
        for active_panel in active_hosted_panels.iter().flatten() {
            let Some(panel_pane_kind) = PanelPaneKind::for_panel_key(active_panel.panel_key())
            else {
                continue;
            };
            let dock_has_hosted_panel_of_same_kind = dock.panel_handles().iter().any(|panel| {
                PanelPaneKind::for_panel_key(panel.panel_key()) == Some(panel_pane_kind)
            });
            if !dock_has_hosted_panel_of_same_kind {
                continue;
            }

            return dock
                .panel_for_id(active_panel.panel_id())
                .map(|panel| panel.persistent_name().to_string());
        }

        dock.active_panel()
            .filter(|panel| PanelPaneKind::for_panel_key(panel.panel_key()).is_none())
            .map(|panel| panel.persistent_name().to_string())
    }

    pub fn set_dock_structure(
        &self,
        docks: DockStructure,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (dock, data) in [
            (&self.left_dock, docks.left),
            (&self.right_dock, docks.right),
        ] {
            dock.update(cx, |dock, cx| {
                dock.serialized_dock = Some(data);
                dock.restore_state(window, cx);
            });
        }
    }

    /// Returns which dock currently has focus, or `None` if focus is in the
    /// center pane or elsewhere. Does NOT fall back to any global state.
    pub fn focused_dock_position(&self, window: &Window, cx: &App) -> Option<DockPosition> {
        [
            (DockPosition::Left, &self.left_dock),
            (DockPosition::Right, &self.right_dock),
        ]
        .into_iter()
        .find(|(_, dock)| {
            dock.read(cx).is_open() && dock.focus_handle(cx).contains_focused(window, cx)
        })
        .map(|(position, _)| position)
    }

    pub fn active_worktree_creation(&self) -> &ActiveWorktreeCreation {
        &self.active_worktree_creation
    }

    pub fn set_active_worktree_creation(
        &mut self,
        label: Option<SharedString>,
        is_switch: bool,
        cx: &mut Context<Self>,
    ) {
        self.active_worktree_creation.label = label;
        self.active_worktree_creation.is_switch = is_switch;
        cx.emit(Event::WorktreeCreationChanged);
        cx.notify();
    }

    /// Captures the current workspace state for restoring after a worktree switch.
    /// This includes dock layout, open file paths, and the active file path.
    pub fn capture_state_for_worktree_switch(
        &self,
        window: &Window,
        fallback_focused_dock: Option<DockPosition>,
        cx: &App,
    ) -> PreviousWorkspaceState {
        let dock_structure = self.capture_dock_state(window, cx);
        let open_file_paths = self.open_item_abs_paths(cx);
        let active_file_path = self
            .active_item(cx)
            .and_then(|item| item.project_path(cx))
            .and_then(|pp| self.project().read(cx).absolute_path(&pp, cx));

        let focused_dock = self
            .focused_dock_position(window, cx)
            .or(fallback_focused_dock);

        PreviousWorkspaceState {
            dock_structure,
            open_file_paths,
            active_file_path,
            focused_dock,
        }
    }

    pub fn open_item_abs_paths(&self, cx: &App) -> Vec<PathBuf> {
        self.items(cx)
            .filter_map(|item| {
                let project_path = item.project_path(cx)?;
                self.project.read(cx).absolute_path(&project_path, cx)
            })
            .collect()
    }

    pub fn dock_at_position(&self, position: DockPosition) -> &Entity<Dock> {
        match position {
            DockPosition::Left => &self.left_dock,
            DockPosition::Right => &self.right_dock,
            DockPosition::Bottom => &self.right_dock,
        }
    }

    fn valid_panel_dock_position<T: Panel>(
        &self,
        panel: &Entity<T>,
        window: &Window,
        cx: &App,
    ) -> DockPosition {
        let requested_position = panel.position(window, cx);
        if requested_position != DockPosition::Bottom
            && panel.position_is_valid(requested_position, cx)
        {
            requested_position
        } else if panel.position_is_valid(DockPosition::Left, cx) {
            DockPosition::Left
        } else {
            DockPosition::Right
        }
    }

    pub fn agent_panel_position(&self, cx: &App) -> Option<DockPosition> {
        self.all_docks().into_iter().find_map(|dock| {
            let dock = dock.read(cx);
            dock.has_agent_panel(cx).then_some(dock.position())
        })
    }

    pub fn panel_size_state<T: Panel>(&self, cx: &App) -> Option<dock::PanelSizeState> {
        self.all_docks().into_iter().find_map(|dock| {
            let dock = dock.read(cx);
            let panel = dock.panel::<T>()?;
            dock.stored_panel_size_state(&panel)
        })
    }

    pub fn persisted_panel_size_state(
        &self,
        panel_key: &'static str,
        cx: &App,
    ) -> Option<dock::PanelSizeState> {
        dock::Dock::load_persisted_size_state(self, panel_key, cx)
    }

    pub fn persist_panel_size_state(
        &self,
        panel_key: &str,
        size_state: dock::PanelSizeState,
        cx: &mut App,
    ) {
        let Some(workspace_id) = self
            .database_id()
            .map(|id| i64::from(id).to_string())
            .or(self.session_id())
        else {
            return;
        };

        let kvp = db::kvp::KeyValueStore::global(cx);
        let panel_key = panel_key.to_string();
        cx.background_spawn(async move {
            let scope = kvp.scoped(dock::PANEL_SIZE_STATE_KEY);
            scope
                .write(
                    format!("{workspace_id}:{panel_key}"),
                    serde_json::to_string(&size_state)?,
                )
                .await
        })
        .detach_and_log_err(cx);
    }

    pub fn set_panel_size_state<T: Panel>(
        &mut self,
        size_state: dock::PanelSizeState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(panel) = self.panel::<T>(cx) else {
            return false;
        };

        let dock_position = self.valid_panel_dock_position(&panel, window, cx);
        let dock = self.dock_at_position(dock_position);
        let did_set = dock.update(cx, |dock, cx| {
            dock.set_panel_size_state(&panel, size_state, cx)
        });

        if did_set {
            self.persist_panel_size_state(T::panel_key(), size_state, cx);
        }

        did_set
    }

    pub fn toggle_dock_panel_flexible_size(
        &self,
        dock: &Entity<Dock>,
        panel: &dyn PanelHandle,
        window: &mut Window,
        cx: &mut App,
    ) {
        let position = dock.read(cx).position();
        let current_size = self.dock_size(&dock.read(cx), window, cx);
        let current_flex =
            current_size.and_then(|size| self.dock_flex_for_size(position, size, window, cx));
        dock.update(cx, |dock, cx| {
            dock.toggle_panel_flexible_size(panel, current_size, current_flex, window, cx);
        });
    }

    pub(super) fn dock_size(&self, dock: &Dock, window: &Window, cx: &App) -> Option<Pixels> {
        let panel = dock.active_panel()?;
        let size_state = dock
            .stored_panel_size_state(panel.as_ref())
            .unwrap_or_default();
        let position = dock.position();

        let use_flex = panel.has_flexible_size(window, cx);

        if position.axis() == Axis::Horizontal
            && use_flex
            && let Some(flex) = size_state.flex.or_else(|| self.default_dock_flex(position))
        {
            let workspace_width = self.bounds.size.width;
            if workspace_width <= Pixels::ZERO {
                return None;
            }
            let flex = flex.max(0.001);
            let center_column_count = self.center_full_height_column_count();
            let opposite = self.opposite_dock_panel_and_size_state(position, window, cx);
            if let Some(opposite_flex) = opposite.as_ref().and_then(|(_, s)| s.flex) {
                let total_flex = flex + center_column_count + opposite_flex;
                return Some((flex / total_flex * workspace_width).max(RESIZE_HANDLE_SIZE));
            } else {
                let opposite_fixed = opposite
                    .map(|(panel, s)| s.size.unwrap_or_else(|| panel.default_size(window, cx)))
                    .unwrap_or_default();
                let available = (workspace_width - opposite_fixed).max(RESIZE_HANDLE_SIZE);
                return Some(
                    (flex / (flex + center_column_count) * available).max(RESIZE_HANDLE_SIZE),
                );
            }
        }

        Some(
            size_state
                .size
                .unwrap_or_else(|| panel.default_size(window, cx)),
        )
    }

    pub fn dock_flex_for_size(
        &self,
        position: DockPosition,
        size: Pixels,
        window: &Window,
        cx: &App,
    ) -> Option<f32> {
        if position.axis() != Axis::Horizontal {
            return None;
        }

        let workspace_width = self.bounds.size.width;
        if workspace_width <= Pixels::ZERO {
            return None;
        }

        let center_column_count = self.center_full_height_column_count();
        let opposite = self.opposite_dock_panel_and_size_state(position, window, cx);
        if let Some(opposite_flex) = opposite.as_ref().and_then(|(_, s)| s.flex) {
            let size = size.clamp(px(0.), workspace_width - px(1.));
            Some((size * (center_column_count + opposite_flex) / (workspace_width - size)).max(0.0))
        } else {
            let opposite_width = opposite
                .map(|(panel, s)| s.size.unwrap_or_else(|| panel.default_size(window, cx)))
                .unwrap_or_default();
            let available = (workspace_width - opposite_width).max(RESIZE_HANDLE_SIZE);
            let remaining = (available - size).max(px(1.));
            Some((size * center_column_count / remaining).max(0.0))
        }
    }

    fn opposite_dock_panel_and_size_state(
        &self,
        position: DockPosition,
        window: &Window,
        cx: &App,
    ) -> Option<(Arc<dyn PanelHandle>, PanelSizeState)> {
        let opposite_position = match position {
            DockPosition::Left => DockPosition::Right,
            DockPosition::Right => DockPosition::Left,
            DockPosition::Bottom => return None,
        };

        let opposite_dock = self.dock_at_position(opposite_position).read(cx);
        let panel = opposite_dock.visible_panel()?;
        let mut size_state = opposite_dock
            .stored_panel_size_state(panel.as_ref())
            .unwrap_or_default();
        if size_state.flex.is_none() && panel.has_flexible_size(window, cx) {
            size_state.flex = self.default_dock_flex(opposite_position);
        }
        Some((panel.clone(), size_state))
    }

    fn center_full_height_column_count(&self) -> f32 {
        self.center.full_height_column_count().max(1) as f32
    }

    pub fn default_dock_flex(&self, position: DockPosition) -> Option<f32> {
        if position.axis() != Axis::Horizontal {
            return None;
        }

        Some(1.0)
    }

    pub fn is_edited(&self) -> bool {
        self.window_edited
    }

    pub fn add_panel<T: Panel>(
        &mut self,
        panel: Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus_handle = panel.panel_focus_handle(cx);
        cx.on_focus_in(&focus_handle, window, Self::handle_panel_focused)
            .detach();

        let dock_position = self.valid_panel_dock_position(&panel, window, cx);
        let dock = self.dock_at_position(dock_position);
        let any_panel = panel.to_any();
        let persisted_size_state =
            self.persisted_panel_size_state(T::panel_key(), cx)
                .or_else(|| {
                    load_legacy_panel_size(T::panel_key(), dock_position, self, cx).map(|size| {
                        let state = dock::PanelSizeState {
                            size: Some(size),
                            flex: None,
                        };
                        self.persist_panel_size_state(T::panel_key(), state, cx);
                        state
                    })
                });

        dock.update(cx, |dock, cx| {
            let index = dock.add_panel(panel.clone(), self.weak_self.clone(), window, cx);
            if let Some(size_state) = persisted_size_state {
                dock.set_panel_size_state(&panel, size_state, cx);
            }
            index
        });

        self.add_panel_to_panel_pane(panel.clone(), window, cx);
        cx.emit(Event::PanelAdded(any_panel));
    }

    pub fn remove_panel<T: Panel>(
        &mut self,
        panel: &Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for dock in self.all_docks() {
            dock.update(cx, |dock, cx| dock.remove_panel(panel, window, cx));
        }
    }
}
