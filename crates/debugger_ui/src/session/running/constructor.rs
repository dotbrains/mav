use super::*;

impl RunningState {
    pub(crate) fn new(
        session: Entity<Session>,
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        parent_terminal: Option<Entity<DebugTerminal>>,
        serialized_pane_layout: Option<SerializedLayout>,
        dock_axis: Axis,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let session_id = session.read(cx).session_id();
        let weak_project = project.downgrade();
        let weak_state = cx.weak_entity();
        let stack_frame_list = cx.new(|cx| {
            StackFrameList::new(
                workspace.clone(),
                session.clone(),
                weak_state.clone(),
                window,
                cx,
            )
        });

        let debug_terminal =
            parent_terminal.unwrap_or_else(|| cx.new(|cx| DebugTerminal::empty(window, cx)));
        let memory_view = cx.new(|cx| {
            MemoryView::new(
                session.clone(),
                workspace.clone(),
                stack_frame_list.downgrade(),
                window,
                cx,
            )
        });
        let variable_list = cx.new(|cx| {
            VariableList::new(
                session.clone(),
                stack_frame_list.clone(),
                memory_view.clone(),
                weak_state.clone(),
                window,
                cx,
            )
        });

        let module_list = cx.new(|cx| ModuleList::new(session.clone(), workspace.clone(), cx));

        let loaded_source_list = cx.new(|cx| LoadedSourceList::new(session.clone(), cx));

        let console = cx.new(|cx| {
            Console::new(
                session.clone(),
                stack_frame_list.clone(),
                variable_list.clone(),
                window,
                cx,
            )
        });

        let breakpoint_list = BreakpointList::new(
            Some(session.clone()),
            workspace.clone(),
            &project,
            window,
            cx,
        );

        let _subscriptions = vec![
            cx.on_app_quit(move |this, cx| {
                let shutdown = this
                    .session
                    .update(cx, |session, cx| session.on_app_quit(cx));
                let terminal = this.debug_terminal.clone();
                async move {
                    shutdown.await;
                    drop(terminal)
                }
            }),
            cx.observe(&module_list, |_, _, cx| cx.notify()),
            cx.subscribe_in(&session, window, |this, _, event, window, cx| {
                match event {
                    SessionEvent::Stopped(thread_id) => {
                        let panel = this
                            .workspace
                            .update(cx, |workspace, cx| {
                                let panel = workspace
                                    .item_of_type::<crate::DebugPanel>(cx)
                                    .or_else(|| workspace.panel::<crate::DebugPanel>(cx));
                                if let Some(panel) = panel.clone() {
                                    crate::DebugPanel::open(panel, workspace, window, cx);
                                }
                                panel
                            })
                            .log_err()
                            .flatten();

                        if let Some(thread_id) = thread_id {
                            this.select_thread(*thread_id, window, cx);
                        }
                        if let Some(panel) = panel {
                            let id = this.session_id;
                            window.defer(cx, move |window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.activate_session_by_id(id, window, cx);
                                })
                            })
                        }
                    }
                    SessionEvent::Threads => {
                        let threads = this.session.update(cx, |this, cx| this.threads(cx));
                        this.select_current_thread(&threads, window, cx);
                    }
                    SessionEvent::CapabilitiesLoaded => {
                        let capabilities = this.capabilities(cx);
                        if !capabilities.supports_modules_request.unwrap_or(false) {
                            this.remove_pane_item(DebuggerPaneItem::Modules, window, cx);
                        }
                        if !capabilities
                            .supports_loaded_sources_request
                            .unwrap_or(false)
                        {
                            this.remove_pane_item(DebuggerPaneItem::LoadedSources, window, cx);
                        }
                    }
                    SessionEvent::RunInTerminal { request, sender } => this
                        .handle_run_in_terminal(request, sender.clone(), window, cx)
                        .detach_and_log_err(cx),

                    _ => {}
                }
                cx.notify()
            }),
            cx.on_focus_out(&focus_handle, window, |this, _, window, cx| {
                this.serialize_layout(window, cx);
            }),
            cx.subscribe(
                &session,
                |this, session, event: &SessionStateEvent, cx| match event {
                    SessionStateEvent::Shutdown if session.read(cx).is_building() => {
                        this.shutdown(cx);
                    }
                    _ => {}
                },
            ),
        ];

        let mut pane_close_subscriptions = HashMap::default();
        let panes = if let Some(root) = serialized_pane_layout.and_then(|serialized_layout| {
            persistence::deserialize_pane_layout(
                serialized_layout.panes,
                dock_axis != serialized_layout.dock_axis,
                &workspace,
                &project,
                &stack_frame_list,
                &variable_list,
                &module_list,
                &console,
                &breakpoint_list,
                &loaded_source_list,
                &debug_terminal,
                &memory_view,
                &mut pane_close_subscriptions,
                window,
                cx,
            )
        }) {
            workspace::PaneGroup::with_root(root)
        } else {
            pane_close_subscriptions.clear();

            let root = Self::default_pane_layout(
                project,
                &workspace,
                &stack_frame_list,
                &variable_list,
                &console,
                &breakpoint_list,
                &debug_terminal,
                dock_axis,
                &mut pane_close_subscriptions,
                window,
                cx,
            );

            workspace::PaneGroup::with_root(root)
        };
        let active_pane = panes.first_pane();

        Self {
            memory_view,
            session,
            workspace,
            project: weak_project,
            focus_handle,
            variable_list,
            _subscriptions,
            thread_id: None,
            _remote_id: None,
            stack_frame_list,
            session_id,
            panes,
            active_pane,
            module_list,
            console,
            breakpoint_list,
            loaded_sources_list: loaded_source_list,
            pane_close_subscriptions,
            debug_terminal,
            dock_axis,
            _schedule_serialize: None,
            scenario: None,
            scenario_context: None,
        }
    }
}
