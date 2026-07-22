use super::*;

impl DebugPanel {
    pub(crate) async fn register_session(
        this: WeakEntity<Self>,
        session: Entity<Session>,
        focus: bool,
        cx: &mut AsyncWindowContext,
    ) -> Result<Entity<DebugSession>> {
        let debug_session = register_session_inner(&this, session, cx).await?;

        let (workspace, debug_panel) = this.update_in(cx, |this, window, cx| {
            if focus {
                this.activate_session(debug_session.clone(), window, cx);
            }

            (this.workspace.clone(), cx.entity())
        })?;
        workspace.update_in(cx, |workspace, window, cx| {
            DebugPanel::open(debug_panel, workspace, window, cx);
        })?;
        Ok(debug_session)
    }

    pub(crate) fn handle_restart_request(
        &mut self,
        mut curr_session: Entity<Session>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        while let Some(parent_session) = curr_session.read(cx).parent_session().cloned() {
            curr_session = parent_session;
        }

        let Some(worktree) = curr_session.read(cx).worktree() else {
            log::error!("Attempted to restart a non-running session");
            return;
        };

        let dap_store_handle = self.project.read(cx).dap_store();
        let label = curr_session.read(cx).label();
        let quirks = curr_session.read(cx).quirks();
        let adapter = curr_session.read(cx).adapter();
        let binary = curr_session.read(cx).binary().cloned().unwrap();
        let task_context = curr_session.read(cx).task_context().clone();

        let curr_session_id = curr_session.read(cx).session_id();
        self.sessions_with_children
            .retain(|session, _| session.read(cx).session_id(cx) != curr_session_id);
        let task = dap_store_handle.update(cx, |dap_store, cx| {
            dap_store.shutdown_session(curr_session_id, cx)
        });

        cx.spawn_in(window, async move |this, cx| {
            task.await.log_err();

            let (session, task) = dap_store_handle.update(cx, |dap_store, cx| {
                let session = dap_store.new_session(label, adapter, task_context, None, quirks, cx);

                let task = session.update(cx, |session, cx| {
                    session.boot(binary, worktree, dap_store_handle.downgrade(), cx)
                });
                (session, task)
            });
            Self::register_session(this.clone(), session.clone(), true, cx).await?;

            if let Err(error) = task.await {
                session
                    .update(cx, |session, cx| {
                        session
                            .console_output(cx)
                            .unbounded_send(format!(
                                "Session failed to restart with error: {}",
                                error
                            ))
                            .ok();
                        session.shutdown(cx)
                    })
                    .await;

                return Err(error);
            };

            Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub fn handle_start_debugging_request(
        &mut self,
        request: &StartDebuggingRequestArguments,
        parent_session: Entity<Session>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(worktree) = parent_session.read(cx).worktree() else {
            log::error!("Attempted to start a child-session from a non-running session");
            return;
        };

        let dap_store_handle = self.project.read(cx).dap_store();
        let label = self.label_for_child_session(&parent_session, request, cx);
        let adapter = parent_session.read(cx).adapter();
        let quirks = parent_session.read(cx).quirks();
        let Some(mut binary) = parent_session.read(cx).binary().cloned() else {
            log::error!("Attempted to start a child-session without a binary");
            return;
        };
        let task_context = parent_session.read(cx).task_context().clone();
        binary.request_args = request.clone();
        cx.spawn_in(window, async move |this, cx| {
            let (session, task) = dap_store_handle.update(cx, |dap_store, cx| {
                let session = dap_store.new_session(
                    label,
                    adapter,
                    task_context,
                    Some(parent_session.clone()),
                    quirks,
                    cx,
                );

                let task = session.update(cx, |session, cx| {
                    session.boot(binary, worktree, dap_store_handle.downgrade(), cx)
                });
                (session, task)
            });
            // Focus child sessions if the parent has never emitted a stopped event;
            // this improves our JavaScript experience, as it always spawns a "main" session that then spawns subsessions.
            let parent_ever_stopped = parent_session.update(cx, |this, _| this.has_ever_stopped());
            Self::register_session(this, session, !parent_ever_stopped, cx).await?;
            task.await
        })
        .detach_and_log_err(cx);
    }

    pub(crate) fn close_session(
        &mut self,
        entity_id: EntityId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self
            .sessions_with_children
            .keys()
            .find(|other| entity_id == other.entity_id())
            .cloned()
        else {
            return;
        };
        session.update(cx, |this, cx| {
            this.running_state().update(cx, |this, cx| {
                this.serialize_layout(window, cx);
            });
        });
        let session_id = session.update(cx, |this, cx| this.session_id(cx));
        let should_prompt = self
            .project
            .update(cx, |this, cx| {
                let session = this.dap_store().read(cx).session_by_id(session_id);
                session.map(|session| !session.read(cx).is_terminated())
            })
            .unwrap_or_default();

        cx.spawn_in(window, async move |this, cx| {
            if should_prompt {
                let response = cx.prompt(
                    gpui::PromptLevel::Warning,
                    "This Debug Session is still running. Are you sure you want to terminate it?",
                    None,
                    &["Yes", "No"],
                );
                if response.await == Ok(1) {
                    return;
                }
            }
            session.update(cx, |session, cx| session.shutdown(cx));
            this.update(cx, |this, cx| {
                this.retain_sessions(&|other: &Entity<DebugSession>| {
                    entity_id != other.entity_id()
                });
                if let Some(active_session_id) = this
                    .active_session
                    .as_ref()
                    .map(|session| session.entity_id())
                    && active_session_id == entity_id
                {
                    this.active_session = this.sessions_with_children.keys().next().cloned();
                }
                cx.notify()
            })
            .ok();
        })
        .detach();
    }

    fn label_for_child_session(
        &self,
        parent_session: &Entity<Session>,
        request: &StartDebuggingRequestArguments,
        cx: &mut Context<'_, Self>,
    ) -> Option<SharedString> {
        let adapter = parent_session.read(cx).adapter();
        if let Some(adapter) = DapRegistry::global(cx).adapter(&adapter)
            && let Some(label) = adapter.label_for_child_session(request)
        {
            return Some(label.into());
        }
        None
    }

    fn retain_sessions(&mut self, keep: &dyn Fn(&Entity<DebugSession>) -> bool) {
        self.sessions_with_children
            .retain(|session, _| keep(session));
        for children in self.sessions_with_children.values_mut() {
            children.retain(|child| {
                let Some(child) = child.upgrade() else {
                    return false;
                };
                keep(&child)
            });
        }
    }
}

async fn register_session_inner(
    this: &WeakEntity<DebugPanel>,
    session: Entity<Session>,
    cx: &mut AsyncWindowContext,
) -> Result<Entity<DebugSession>> {
    let adapter_name = session.read_with(cx, |session, _| session.adapter());
    this.update_in(cx, |_, window, cx| {
        cx.subscribe_in(
            &session,
            window,
            move |this, session, event: &SessionStateEvent, window, cx| match event {
                SessionStateEvent::Restart => {
                    this.handle_restart_request(session.clone(), window, cx);
                }
                SessionStateEvent::SpawnChildSession { request } => {
                    this.handle_start_debugging_request(request, session.clone(), window, cx);
                }
                _ => {}
            },
        )
        .detach();
    })
    .ok();
    let serialized_layout = this
        .update(cx, |_, cx| {
            persistence::get_serialized_layout(&adapter_name, &db::kvp::KeyValueStore::global(cx))
        })
        .ok()
        .flatten();
    let debug_session = this.update_in(cx, |this, window, cx| {
        let parent_session = this
            .sessions_with_children
            .keys()
            .find(|p| Some(p.read(cx).session_id(cx)) == session.read(cx).parent_id(cx))
            .cloned();
        this.retain_sessions(&|session: &Entity<DebugSession>| {
            !session
                .read(cx)
                .running_state()
                .read(cx)
                .session()
                .read(cx)
                .is_terminated()
        });

        let debug_session = DebugSession::running(
            this.project.clone(),
            this.workspace.clone(),
            parent_session
                .as_ref()
                .map(|p| p.read(cx).running_state().read(cx).debug_terminal.clone()),
            session,
            serialized_layout,
            this.position(window, cx).axis(),
            window,
            cx,
        );

        // We might want to make this an event subscription and only notify when a new thread is selected
        // This is used to filter the command menu correctly
        cx.observe(
            &debug_session.read(cx).running_state().clone(),
            |_, _, cx| cx.notify(),
        )
        .detach();
        let insert_position = this
            .sessions_with_children
            .keys()
            .position(|session| Some(session) == parent_session.as_ref())
            .map(|position| position + 1)
            .unwrap_or(this.sessions_with_children.len());
        // Maintain topological sort order of sessions
        let (_, old) = this.sessions_with_children.insert_before(
            insert_position,
            debug_session.clone(),
            Default::default(),
        );
        debug_assert!(old.is_none());
        if let Some(parent_session) = parent_session {
            this.sessions_with_children
                .entry(parent_session)
                .and_modify(|children| children.push(debug_session.downgrade()));
        }

        debug_session
    })?;
    Ok(debug_session)
}
