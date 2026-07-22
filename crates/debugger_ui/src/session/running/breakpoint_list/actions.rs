use super::*;

impl BreakpointList {
    pub(crate) fn new(
        session: Option<Entity<Session>>,
        workspace: WeakEntity<Workspace>,
        project: &Entity<Project>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let project = project.read(cx);
        let breakpoint_store = project.breakpoint_store();
        let worktree_store = project.worktree_store();
        let dap_store = project.dap_store();
        let focus_handle = cx.focus_handle();
        let scroll_handle = UniformListScrollHandle::new();

        let adapter_name = session.as_ref().map(|session| session.read(cx).adapter());
        cx.new(|cx| {
            let this = Self {
                breakpoint_store,
                dap_store,
                worktree_store,
                breakpoints: Default::default(),
                max_width_index: None,
                workspace,
                session,
                focus_handle,
                scroll_handle,
                selected_ix: None,
                input: cx.new(|cx| Editor::single_line(window, cx)),
                strip_mode: None,
                serialize_exception_breakpoints_task: None,
            };
            if let Some(name) = adapter_name {
                _ = this.deserialize_exception_breakpoints(name, cx);
            }
            this
        })
    }

    pub(super) fn edit_line_breakpoint(
        &self,
        path: Arc<Path>,
        row: u32,
        action: BreakpointEditAction,
        cx: &mut App,
    ) {
        Self::edit_line_breakpoint_inner(&self.breakpoint_store, path, row, action, cx);
    }
    pub(super) fn edit_line_breakpoint_inner(
        breakpoint_store: &Entity<BreakpointStore>,
        path: Arc<Path>,
        row: u32,
        action: BreakpointEditAction,
        cx: &mut App,
    ) {
        breakpoint_store.update(cx, |breakpoint_store, cx| {
            if let Some((buffer, breakpoint)) = breakpoint_store.breakpoint_at_row(&path, row, cx) {
                breakpoint_store.toggle_breakpoint(buffer, breakpoint, action, cx);
            } else {
                log::error!("Couldn't find breakpoint at row event though it exists: row {row}")
            }
        })
    }

    pub(super) fn go_to_line_breakpoint(
        &mut self,
        path: Arc<Path>,
        row: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = self
            .worktree_store
            .update(cx, |this, cx| this.find_or_create_worktree(path, false, cx));
        cx.spawn_in(window, async move |this, cx| {
            let (worktree, relative_path) = task.await?;
            let worktree_id = worktree.read_with(cx, |this, _| this.id());
            let item = this
                .update_in(cx, |this, window, cx| {
                    this.workspace.update(cx, |this, cx| {
                        this.open_path((worktree_id, relative_path), None, true, window, cx)
                    })
                })??
                .await?;
            if let Some(editor) = item.downcast::<Editor>() {
                editor
                    .update_in(cx, |this, window, cx| {
                        this.go_to_singleton_buffer_point(Point { row, column: 0 }, window, cx);
                    })
                    .ok();
            }
            anyhow::Ok(())
        })
        .detach();
    }

    pub(crate) fn selection_kind(&self) -> Option<(SelectedBreakpointKind, bool)> {
        self.selected_ix.and_then(|ix| {
            self.breakpoints.get(ix).map(|bp| match &bp.kind {
                BreakpointEntryKind::LineBreakpoint(bp) => (
                    SelectedBreakpointKind::Source,
                    bp.breakpoint.state
                        == project::debugger::breakpoint_store::BreakpointState::Enabled,
                ),
                BreakpointEntryKind::ExceptionBreakpoint(bp) => {
                    (SelectedBreakpointKind::Exception, bp.is_enabled)
                }
                BreakpointEntryKind::DataBreakpoint(bp) => {
                    (SelectedBreakpointKind::Data, bp.0.is_enabled)
                }
            })
        })
    }

    pub(super) fn set_active_breakpoint_property(
        &mut self,
        prop: ActiveBreakpointStripMode,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.strip_mode = Some(prop);
        let placeholder = match prop {
            ActiveBreakpointStripMode::Log => "Set Log Message",
            ActiveBreakpointStripMode::Condition => "Set Condition",
            ActiveBreakpointStripMode::HitCondition => "Set Hit Condition",
        };
        let mut is_exception_breakpoint = true;
        let active_value = self.selected_ix.and_then(|ix| {
            self.breakpoints.get(ix).and_then(|bp| {
                if let BreakpointEntryKind::LineBreakpoint(bp) = &bp.kind {
                    is_exception_breakpoint = false;
                    match prop {
                        ActiveBreakpointStripMode::Log => bp.breakpoint.message.clone(),
                        ActiveBreakpointStripMode::Condition => bp.breakpoint.condition.clone(),
                        ActiveBreakpointStripMode::HitCondition => {
                            bp.breakpoint.hit_condition.clone()
                        }
                    }
                } else {
                    None
                }
            })
        });

        self.input.update(cx, |this, cx| {
            this.set_placeholder_text(placeholder, window, cx);
            this.set_read_only(is_exception_breakpoint);
            this.set_text(active_value.as_deref().unwrap_or(""), window, cx);
        });
    }

    pub(super) fn select_ix(
        &mut self,
        ix: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_ix = ix;
        if let Some(ix) = ix {
            self.scroll_handle
                .scroll_to_item(ix, ScrollStrategy::Center);
        }
        if let Some(mode) = self.strip_mode {
            self.set_active_breakpoint_property(mode, window, cx);
        }

        cx.notify();
    }

    pub(super) fn select_next(
        &mut self,
        _: &menu::SelectNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.strip_mode.is_some() && self.input.focus_handle(cx).contains_focused(window, cx) {
            cx.propagate();
            return;
        }
        let ix = match self.selected_ix {
            _ if self.breakpoints.is_empty() => None,
            None => Some(0),
            Some(ix) => {
                if ix == self.breakpoints.len() - 1 {
                    Some(0)
                } else {
                    Some(ix + 1)
                }
            }
        };
        self.select_ix(ix, window, cx);
    }

    pub(super) fn select_previous(
        &mut self,
        _: &menu::SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.strip_mode.is_some() && self.input.focus_handle(cx).contains_focused(window, cx) {
            cx.propagate();
            return;
        }
        let ix = match self.selected_ix {
            _ if self.breakpoints.is_empty() => None,
            None => Some(self.breakpoints.len() - 1),
            Some(ix) => {
                if ix == 0 {
                    Some(self.breakpoints.len() - 1)
                } else {
                    Some(ix - 1)
                }
            }
        };
        self.select_ix(ix, window, cx);
    }

    pub(super) fn select_first(
        &mut self,
        _: &menu::SelectFirst,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.strip_mode.is_some() && self.input.focus_handle(cx).contains_focused(window, cx) {
            cx.propagate();
            return;
        }
        let ix = if !self.breakpoints.is_empty() {
            Some(0)
        } else {
            None
        };
        self.select_ix(ix, window, cx);
    }

    pub(super) fn select_last(
        &mut self,
        _: &menu::SelectLast,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.strip_mode.is_some() && self.input.focus_handle(cx).contains_focused(window, cx) {
            cx.propagate();
            return;
        }
        let ix = if !self.breakpoints.is_empty() {
            Some(self.breakpoints.len() - 1)
        } else {
            None
        };
        self.select_ix(ix, window, cx);
    }

    pub(super) fn dismiss(
        &mut self,
        _: &menu::Cancel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.input.focus_handle(cx).contains_focused(window, cx) {
            self.focus_handle.focus(window, cx);
        } else if self.strip_mode.is_some() {
            self.strip_mode.take();
            cx.notify();
        } else {
            cx.propagate();
        }
    }
    pub(super) fn confirm(
        &mut self,
        _: &menu::Confirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.selected_ix.and_then(|ix| self.breakpoints.get_mut(ix)) else {
            return;
        };

        if let Some(mode) = self.strip_mode {
            let handle = self.input.focus_handle(cx);
            if handle.is_focused(window) {
                // Go back to the main strip. Save the result as well.
                let text = self.input.read(cx).text(cx);

                match mode {
                    ActiveBreakpointStripMode::Log => {
                        if let BreakpointEntryKind::LineBreakpoint(line_breakpoint) = &entry.kind {
                            Self::edit_line_breakpoint_inner(
                                &self.breakpoint_store,
                                line_breakpoint.breakpoint.path.clone(),
                                line_breakpoint.breakpoint.row,
                                BreakpointEditAction::EditLogMessage(Arc::from(text)),
                                cx,
                            );
                        }
                    }
                    ActiveBreakpointStripMode::Condition => {
                        if let BreakpointEntryKind::LineBreakpoint(line_breakpoint) = &entry.kind {
                            Self::edit_line_breakpoint_inner(
                                &self.breakpoint_store,
                                line_breakpoint.breakpoint.path.clone(),
                                line_breakpoint.breakpoint.row,
                                BreakpointEditAction::EditCondition(Arc::from(text)),
                                cx,
                            );
                        }
                    }
                    ActiveBreakpointStripMode::HitCondition => {
                        if let BreakpointEntryKind::LineBreakpoint(line_breakpoint) = &entry.kind {
                            Self::edit_line_breakpoint_inner(
                                &self.breakpoint_store,
                                line_breakpoint.breakpoint.path.clone(),
                                line_breakpoint.breakpoint.row,
                                BreakpointEditAction::EditHitCondition(Arc::from(text)),
                                cx,
                            );
                        }
                    }
                }
                self.focus_handle.focus(window, cx);
            } else {
                handle.focus(window, cx);
            }

            return;
        }
        match &mut entry.kind {
            BreakpointEntryKind::LineBreakpoint(line_breakpoint) => {
                let path = line_breakpoint.breakpoint.path.clone();
                let row = line_breakpoint.breakpoint.row;
                self.go_to_line_breakpoint(path, row, window, cx);
            }
            BreakpointEntryKind::DataBreakpoint(_)
            | BreakpointEntryKind::ExceptionBreakpoint(_) => {}
        }
    }

    pub(super) fn toggle_enable_breakpoint(
        &mut self,
        _: &ToggleEnableBreakpoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.selected_ix.and_then(|ix| self.breakpoints.get_mut(ix)) else {
            return;
        };
        if self.strip_mode.is_some() && self.input.focus_handle(cx).contains_focused(window, cx) {
            cx.propagate();
            return;
        }

        match &mut entry.kind {
            BreakpointEntryKind::LineBreakpoint(line_breakpoint) => {
                let path = line_breakpoint.breakpoint.path.clone();
                let row = line_breakpoint.breakpoint.row;
                self.edit_line_breakpoint(path, row, BreakpointEditAction::InvertState, cx);
            }
            BreakpointEntryKind::ExceptionBreakpoint(exception_breakpoint) => {
                let id = exception_breakpoint.id.clone();
                self.toggle_exception_breakpoint(&id, cx);
            }
            BreakpointEntryKind::DataBreakpoint(data_breakpoint) => {
                let id = data_breakpoint.0.dap.data_id.clone();
                self.toggle_data_breakpoint(&id, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn unset_breakpoint(
        &mut self,
        _: &UnsetBreakpoint,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.selected_ix.and_then(|ix| self.breakpoints.get_mut(ix)) else {
            return;
        };

        if let BreakpointEntryKind::LineBreakpoint(line_breakpoint) = &mut entry.kind {
            let path = line_breakpoint.breakpoint.path.clone();
            let row = line_breakpoint.breakpoint.row;
            self.edit_line_breakpoint(path, row, BreakpointEditAction::Toggle, cx);
        }
        cx.notify();
    }

    pub(super) fn previous_breakpoint_property(
        &mut self,
        _: &PreviousBreakpointProperty,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next_mode = match self.strip_mode {
            Some(ActiveBreakpointStripMode::Log) => None,
            Some(ActiveBreakpointStripMode::Condition) => Some(ActiveBreakpointStripMode::Log),
            Some(ActiveBreakpointStripMode::HitCondition) => {
                Some(ActiveBreakpointStripMode::Condition)
            }
            None => Some(ActiveBreakpointStripMode::HitCondition),
        };
        if let Some(mode) = next_mode {
            self.set_active_breakpoint_property(mode, window, cx);
        } else {
            self.strip_mode.take();
        }

        cx.notify();
    }
    pub(super) fn next_breakpoint_property(
        &mut self,
        _: &NextBreakpointProperty,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next_mode = match self.strip_mode {
            Some(ActiveBreakpointStripMode::Log) => Some(ActiveBreakpointStripMode::Condition),
            Some(ActiveBreakpointStripMode::Condition) => {
                Some(ActiveBreakpointStripMode::HitCondition)
            }
            Some(ActiveBreakpointStripMode::HitCondition) => None,
            None => Some(ActiveBreakpointStripMode::Log),
        };
        if let Some(mode) = next_mode {
            self.set_active_breakpoint_property(mode, window, cx);
        } else {
            self.strip_mode.take();
        }
        cx.notify();
    }

    pub(super) fn toggle_data_breakpoint(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            session.update(cx, |this, cx| {
                this.toggle_data_breakpoint(id, cx);
            });
        }
    }

    pub(super) fn toggle_exception_breakpoint(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            session.update(cx, |this, cx| {
                this.toggle_exception_breakpoint(id, cx);
            });
            cx.notify();
            const EXCEPTION_SERIALIZATION_INTERVAL: Duration = Duration::from_secs(1);
            self.serialize_exception_breakpoints_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(EXCEPTION_SERIALIZATION_INTERVAL)
                    .await;
                this.update(cx, |this, cx| this.serialize_exception_breakpoints(cx))?
                    .await?;
                Ok(())
            }));
        }
    }

    pub(super) fn kvp_key(adapter_name: &str) -> String {
        format!("debug_adapter_`{adapter_name}`_persistence")
    }
    pub(super) fn serialize_exception_breakpoints(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        if let Some(session) = self.session.as_ref() {
            let key = {
                let session = session.read(cx);
                let name = session.adapter().0;
                Self::kvp_key(&name)
            };
            let settings = self.dap_store.update(cx, |this, cx| {
                this.sync_adapter_options(session, cx);
            });
            let value = serde_json::to_string(&settings);

            let kvp = KeyValueStore::global(cx);
            cx.background_executor()
                .spawn(async move { kvp.write_kvp(key, value?).await })
        } else {
            Task::ready(Result::Ok(()))
        }
    }

    pub(super) fn deserialize_exception_breakpoints(
        &self,
        adapter_name: DebugAdapterName,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let Some(val) = KeyValueStore::global(cx).read_kvp(&Self::kvp_key(&adapter_name))? else {
            return Ok(());
        };
        let value: PersistedAdapterOptions = serde_json::from_str(&val)?;
        self.dap_store
            .update(cx, |this, _| this.set_adapter_options(adapter_name, value));

        Ok(())
    }
}
