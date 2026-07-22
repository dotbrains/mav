use super::*;

impl RunningState {
    pub(crate) fn remove_pane_item(
        &mut self,
        item_kind: DebuggerPaneItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((pane, item_id)) = self.panes.panes().iter().find_map(|pane| {
            Some(pane).zip(
                pane.read(cx)
                    .items()
                    .find(|item| {
                        item.act_as::<SubView>(cx)
                            .is_some_and(|view| view.read(cx).kind == item_kind)
                    })
                    .map(|item| item.item_id()),
            )
        }) {
            pane.update(cx, |pane, cx| {
                pane.remove_item(item_id, false, true, window, cx)
            })
        }
    }

    pub(crate) fn has_pane_at_position(&self, position: Point<Pixels>) -> bool {
        self.panes.pane_at_pixel_position(position).is_some()
    }
    fn create_sub_view(
        &self,
        item_kind: DebuggerPaneItem,
        pane: &Entity<Pane>,
        cx: &mut Context<Self>,
    ) -> Box<dyn ItemHandle> {
        let running_state = cx.weak_entity();
        let host_pane = pane.downgrade();

        match item_kind {
            DebuggerPaneItem::Console => Box::new(SubView::console(
                self.console.clone(),
                running_state,
                host_pane,
                cx,
            )),
            DebuggerPaneItem::Variables => Box::new(SubView::new(
                self.variable_list.focus_handle(cx),
                self.variable_list.clone().into(),
                item_kind,
                running_state,
                host_pane,
                cx,
            )),
            DebuggerPaneItem::BreakpointList => Box::new(SubView::breakpoint_list(
                self.breakpoint_list.clone(),
                running_state,
                host_pane,
                cx,
            )),
            DebuggerPaneItem::Frames => Box::new(SubView::new(
                self.stack_frame_list.focus_handle(cx),
                self.stack_frame_list.clone().into(),
                item_kind,
                running_state,
                host_pane,
                cx,
            )),
            DebuggerPaneItem::Modules => Box::new(SubView::new(
                self.module_list.focus_handle(cx),
                self.module_list.clone().into(),
                item_kind,
                running_state,
                host_pane,
                cx,
            )),
            DebuggerPaneItem::LoadedSources => Box::new(SubView::new(
                self.loaded_sources_list.focus_handle(cx),
                self.loaded_sources_list.clone().into(),
                item_kind,
                running_state,
                host_pane,
                cx,
            )),
            DebuggerPaneItem::Terminal => Box::new(SubView::new(
                self.debug_terminal.focus_handle(cx),
                self.debug_terminal.clone().into(),
                item_kind,
                running_state,
                host_pane,
                cx,
            )),
            DebuggerPaneItem::MemoryView => Box::new(SubView::new(
                self.memory_view.focus_handle(cx),
                self.memory_view.clone().into(),
                item_kind,
                running_state,
                host_pane,
                cx,
            )),
        }
    }

    pub(crate) fn ensure_pane_item(
        &mut self,
        item_kind: DebuggerPaneItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pane_items_status(cx).get(&item_kind) == Some(&true) {
            return;
        };
        let pane = self.panes.last_pane();
        let sub_view = self.create_sub_view(item_kind, &pane, cx);

        pane.update(cx, |pane, cx| {
            pane.add_item_inner(sub_view, false, false, false, None, window, cx);
        })
    }

    pub(crate) fn add_pane_item(
        &mut self,
        item_kind: DebuggerPaneItem,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        debug_assert!(
            item_kind.is_supported(self.session.read(cx).capabilities()),
            "We should only allow adding supported item kinds"
        );

        if let Some(pane) = self.panes.pane_at_pixel_position(position) {
            let sub_view = self.create_sub_view(item_kind, pane, cx);

            pane.update(cx, |pane, cx| {
                pane.add_item(sub_view, false, false, None, window, cx);
            })
        }
    }

    pub(crate) fn pane_items_status(&self, cx: &App) -> IndexMap<DebuggerPaneItem, bool> {
        let caps = self.session.read(cx).capabilities();
        let mut pane_item_status = IndexMap::from_iter(
            DebuggerPaneItem::all()
                .iter()
                .filter(|kind| kind.is_supported(caps))
                .map(|kind| (*kind, false)),
        );
        self.panes.panes().iter().for_each(|pane| {
            pane.read(cx)
                .items()
                .filter_map(|item| item.act_as::<SubView>(cx))
                .for_each(|view| {
                    pane_item_status.insert(view.read(cx).kind, true);
                });
        });

        pane_item_status
    }

    pub(crate) fn serialize_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self._schedule_serialize.is_none() {
            self._schedule_serialize = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;

                let Some((adapter_name, pane_layout)) = this
                    .read_with(cx, |this, cx| {
                        let adapter_name = this.session.read(cx).adapter();
                        (
                            adapter_name,
                            persistence::build_serialized_layout(
                                &this.panes.root,
                                this.dock_axis,
                                cx,
                            ),
                        )
                    })
                    .ok()
                else {
                    return;
                };

                let kvp = this
                    .read_with(cx, |_, cx| db::kvp::KeyValueStore::global(cx))
                    .ok();
                if let Some(kvp) = kvp {
                    persistence::serialize_pane_layout(adapter_name, pane_layout, kvp)
                        .await
                        .log_err();
                }

                this.update(cx, |this, _| {
                    this._schedule_serialize.take();
                })
                .ok();
            }));
        }
    }

    pub(crate) fn handle_pane_event(
        this: &mut RunningState,
        source_pane: &Entity<Pane>,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<RunningState>,
    ) {
        this.serialize_layout(window, cx);
        match event {
            Event::AddItem { item } => {
                if let Some(sub_view) = item.downcast::<SubView>() {
                    sub_view.update(cx, |sub_view, _| {
                        sub_view.set_host_pane(source_pane.downgrade());
                    });
                }
            }
            Event::Remove { .. } => {
                let _did_find_pane = this.panes.remove(source_pane, cx).is_ok();
                debug_assert!(_did_find_pane);
                cx.notify();
            }
            Event::Focus => {
                this.active_pane = source_pane.clone();
            }
            _ => {}
        }
    }

    pub(crate) fn activate_pane_in_direction(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_pane = self.active_pane.clone();
        if let Some(pane) = self
            .panes
            .find_pane_in_direction(&active_pane, direction, cx)
        {
            pane.update(cx, |pane, cx| {
                pane.focus_active_item(window, cx);
            })
        } else {
            self.workspace
                .update(cx, |workspace, cx| {
                    workspace.activate_pane_in_direction(direction, window, cx)
                })
                .ok();
        }
    }

    pub(crate) fn go_to_selected_stack_frame(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.thread_id.is_some() {
            self.stack_frame_list
                .update(cx, |list, cx| {
                    let Some(stack_frame_id) = list.opened_stack_frame_id() else {
                        return Task::ready(Ok(()));
                    };
                    list.go_to_stack_frame(stack_frame_id, window, cx)
                })
                .detach();
        }
    }

    pub(crate) fn has_open_context_menu(&self, cx: &App) -> bool {
        self.variable_list.read(cx).has_open_context_menu()
    }

    pub fn session(&self) -> &Entity<Session> {
        &self.session
    }

    pub fn session_id(&self) -> SessionId {
        self.session_id
    }

    pub(crate) fn selected_stack_frame_id(&self, cx: &App) -> Option<dap::StackFrameId> {
        self.stack_frame_list.read(cx).opened_stack_frame_id()
    }

    pub(crate) fn stack_frame_list(&self) -> &Entity<StackFrameList> {
        &self.stack_frame_list
    }

    #[cfg(test)]
    pub fn console(&self) -> &Entity<Console> {
        &self.console
    }

    #[cfg(test)]
    pub(crate) fn module_list(&self) -> &Entity<ModuleList> {
        &self.module_list
    }

    pub(crate) fn activate_item(
        &mut self,
        item: DebuggerPaneItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_pane_item(item, window, cx);

        let (variable_list_position, pane) = self
            .panes
            .panes()
            .into_iter()
            .find_map(|pane| {
                pane.read(cx)
                    .items_of_type::<SubView>()
                    .position(|view| view.read(cx).view_kind() == item)
                    .map(|view| (view, pane))
            })
            .unwrap();

        pane.update(cx, |this, cx| {
            this.activate_item(variable_list_position, true, true, window, cx);
        });
    }

    #[cfg(test)]
    pub(crate) fn variable_list(&self) -> &Entity<VariableList> {
        &self.variable_list
    }

    #[cfg(test)]
    pub(crate) fn serialized_layout(&self, cx: &App) -> SerializedLayout {
        persistence::build_serialized_layout(&self.panes.root, self.dock_axis, cx)
    }

    pub fn capabilities(&self, cx: &App) -> Capabilities {
        self.session().read(cx).capabilities().clone()
    }
    fn default_pane_layout(
        project: Entity<Project>,
        workspace: &WeakEntity<Workspace>,
        stack_frame_list: &Entity<StackFrameList>,
        variable_list: &Entity<VariableList>,
        console: &Entity<Console>,
        breakpoints: &Entity<BreakpointList>,
        debug_terminal: &Entity<DebugTerminal>,
        dock_axis: Axis,
        subscriptions: &mut HashMap<EntityId, Subscription>,
        window: &mut Window,
        cx: &mut Context<'_, RunningState>,
    ) -> Member {
        let running_state = cx.weak_entity();

        let leftmost_pane = new_debugger_pane(workspace.clone(), project.clone(), window, cx);
        let leftmost_pane_handle = leftmost_pane.downgrade();
        let leftmost_frames = SubView::new(
            stack_frame_list.focus_handle(cx),
            stack_frame_list.clone().into(),
            DebuggerPaneItem::Frames,
            running_state.clone(),
            leftmost_pane_handle.clone(),
            cx,
        );
        let leftmost_breakpoints = SubView::breakpoint_list(
            breakpoints.clone(),
            running_state.clone(),
            leftmost_pane_handle,
            cx,
        );
        leftmost_pane.update(cx, |this, cx| {
            this.add_item(Box::new(leftmost_frames), true, false, None, window, cx);
            this.add_item(
                Box::new(leftmost_breakpoints),
                true,
                false,
                None,
                window,
                cx,
            );
            this.activate_item(0, false, false, window, cx);
        });

        let center_pane = new_debugger_pane(workspace.clone(), project.clone(), window, cx);
        let center_pane_handle = center_pane.downgrade();
        let center_console = SubView::console(
            console.clone(),
            running_state.clone(),
            center_pane_handle.clone(),
            cx,
        );
        let center_variables = SubView::new(
            variable_list.focus_handle(cx),
            variable_list.clone().into(),
            DebuggerPaneItem::Variables,
            running_state.clone(),
            center_pane_handle,
            cx,
        );

        center_pane.update(cx, |this, cx| {
            this.add_item(Box::new(center_console), true, false, None, window, cx);

            this.add_item(Box::new(center_variables), true, false, None, window, cx);
            this.activate_item(0, false, false, window, cx);
        });

        let rightmost_pane = new_debugger_pane(workspace.clone(), project, window, cx);
        let rightmost_terminal = SubView::new(
            debug_terminal.focus_handle(cx),
            debug_terminal.clone().into(),
            DebuggerPaneItem::Terminal,
            running_state,
            rightmost_pane.downgrade(),
            cx,
        );
        rightmost_pane.update(cx, |this, cx| {
            this.add_item(Box::new(rightmost_terminal), false, false, None, window, cx);
        });

        subscriptions.extend(
            [&leftmost_pane, &center_pane, &rightmost_pane]
                .into_iter()
                .map(|entity| {
                    (
                        entity.entity_id(),
                        cx.subscribe_in(entity, window, Self::handle_pane_event),
                    )
                }),
        );

        let group_root = workspace::PaneAxis::new(
            dock_axis.invert(),
            [leftmost_pane, center_pane, rightmost_pane]
                .into_iter()
                .map(workspace::Member::Pane)
                .collect(),
        );

        Member::Axis(group_root)
    }

    pub(crate) fn invert_axies(&mut self, cx: &mut App) {
        self.dock_axis = self.dock_axis.invert();
        self.panes.invert_axies(cx);
    }
}
