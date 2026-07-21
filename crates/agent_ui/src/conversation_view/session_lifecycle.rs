use super::*;

impl ConversationView {
    pub(super) fn set_server_state(&mut self, state: ServerState, cx: &mut Context<Self>) {
        if let Some(connected) = self.as_connected() {
            connected.close_all_sessions(cx).detach();
        }

        self.server_state = state;
        cx.emit(StateChange);
        cx.emit(AcpServerViewEvent::ActiveThreadChanged);
        if matches!(&self.server_state, ServerState::Connected(_)) {
            cx.emit(RootThreadUpdated);
        }
        cx.notify();
    }

    pub(super) fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (resume_session_id, work_dirs, title) = self
            .root_thread_view()
            .map(|thread_view| {
                let tv = thread_view.read(cx);
                let thread = tv.thread.read(cx);
                (
                    Some(thread.session_id().clone()),
                    thread.work_dirs().cloned(),
                    thread.title(),
                )
            })
            .unwrap_or_else(|| {
                let session_id = self.root_session_id.clone();
                let (work_dirs, title) = session_id
                    .as_ref()
                    .and_then(|id| {
                        let store = ThreadMetadataStore::try_global(cx)?;
                        let entry = store.read(cx).entry_by_session(id)?;
                        Some((Some(entry.folder_paths().clone()), entry.title()))
                    })
                    .unwrap_or((None, None));
                (session_id, work_dirs, title)
            });

        self.loading_status = None;

        let state = Self::initial_state(
            self.agent.clone(),
            self.connection_store.clone(),
            self.connection_key.clone(),
            resume_session_id,
            self.thread_id,
            work_dirs,
            title,
            self.project.clone(),
            self.workspace.clone(),
            self.thread_store.clone(),
            None,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
        self.set_server_state(state, cx);

        if let Some(view) = self.root_thread_view() {
            view.update(cx, |this, cx| {
                this.message_editor.update(cx, |editor, cx| {
                    editor.set_session_capabilities(this.session_capabilities.clone(), cx);
                });
            });
        }
        cx.notify();
    }

    pub(super) fn initial_state(
        agent: Rc<dyn AgentServer>,
        connection_store: Entity<AgentConnectionStore>,
        connection_key: Agent,
        resume_session_id: Option<acp::SessionId>,
        thread_id: ThreadId,
        work_dirs: Option<PathList>,
        title: Option<SharedString>,
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        thread_store: Option<Entity<ThreadStore>>,
        initial_content: Option<AgentInitialContent>,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ServerState {
        if project.read(cx).is_via_collab()
            && agent.clone().downcast::<NativeAgentServer>().is_none()
        {
            return ServerState::LoadError {
                error: LoadError::Other(
                    "External agents are not yet supported in shared projects.".into(),
                ),
            };
        }
        let session_work_dirs = work_dirs.unwrap_or_else(|| project.read(cx).default_path_list(cx));

        let connection_entry = connection_store.update(cx, |store, cx| {
            store.request_connection(connection_key.clone(), agent.clone(), cx)
        });

        let connection_entry_subscription =
            cx.subscribe(&connection_entry, |this, _entry, event, cx| match event {
                AgentConnectionEntryEvent::NewVersionAvailable(version) => {
                    if let Some(thread) = this.root_thread_view() {
                        thread.update(cx, |thread, cx| {
                            thread.new_server_version_available = Some(version.clone());
                            cx.notify();
                        });
                    }
                }
                AgentConnectionEntryEvent::LoadingStatusChanged(status) => {
                    this.loading_status = status.clone();
                    cx.notify();
                }
            });

        let connect_result = connection_entry.read(cx).wait_for_connection();

        let side = crate::sidebar_side(cx);
        let thread_location = "current_worktree";
        let loading_draft = if resume_session_id.is_none()
            && initial_content.as_ref().is_none_or(|content| {
                matches!(
                    content,
                    AgentInitialContent::ContentBlock {
                        auto_submit: false,
                        ..
                    }
                )
            }) {
            Some(Self::new_loading_draft(
                &agent,
                &connection_key,
                thread_id,
                workspace.clone(),
                project.downgrade(),
                thread_store.clone(),
                initial_content.as_ref(),
                window,
                cx,
            ))
        } else {
            None
        };
        let initial_content = if loading_draft.is_some() {
            None
        } else {
            initial_content
        };

        let load_task = cx.spawn_in(window, async move |this, cx| {
            let connection = match connect_result.await {
                Ok(AgentConnectedState { connection, .. }) => connection,
                Err(err) => {
                    this.update_in(cx, |this, window, cx| {
                        this.handle_load_error(err, window, cx);
                        cx.notify();
                    })
                    .log_err();
                    return;
                }
            };

            telemetry::event!(
                "Agent Thread Started",
                agent = connection.telemetry_id(),
                source = source.as_str(),
                side = side,
                thread_location = thread_location
            );

            let mut resumed_without_history = false;
            let result = if let Some(session_id) = resume_session_id.clone() {
                cx.update(|_, cx| {
                    if connection.supports_load_session() {
                        connection.clone().load_session(
                            session_id,
                            project.clone(),
                            session_work_dirs,
                            title,
                            cx,
                        )
                    } else if connection.supports_resume_session() {
                        resumed_without_history = true;
                        connection.clone().resume_session(
                            session_id,
                            project.clone(),
                            session_work_dirs,
                            title,
                            cx,
                        )
                    } else {
                        Task::ready(Err(anyhow!(LoadError::Other(
                            "Loading or resuming sessions is not supported by this agent.".into()
                        ))))
                    }
                })
                .log_err()
            } else {
                cx.update(|_, cx| {
                    connection
                        .clone()
                        .new_session(project.clone(), session_work_dirs, cx)
                })
                .log_err()
            };

            let Some(result) = result else {
                return;
            };

            let result = match result.await {
                Err(e) => match e.downcast::<acp_thread::AuthRequired>() {
                    Ok(err) => {
                        cx.update(|window, cx| {
                            Self::handle_auth_required(
                                this,
                                err,
                                agent.agent_id(),
                                connection,
                                window,
                                cx,
                            )
                        })
                        .log_err();
                        return;
                    }
                    Err(err) => Err(err),
                },
                Ok(thread) => Ok(thread),
            };

            let draft_initial_content = if result.is_ok() {
                let draft_contents_task = this
                    .update(cx, |this, cx| {
                        this.loading_draft_editor().map(|message_editor| {
                            message_editor
                                .update(cx, |message_editor, cx| message_editor.draft_contents(cx))
                        })
                    })
                    .ok()
                    .flatten();

                if let Some(task) = draft_contents_task {
                    task.await
                        .ok()
                        .filter(|blocks| !blocks.is_empty())
                        .map(|blocks| AgentInitialContent::ContentBlock {
                            blocks,
                            auto_submit: false,
                        })
                } else {
                    None
                }
            } else {
                None
            };

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(thread) => {
                        let root_session_id = thread.read(cx).session_id().clone();

                        let conversation = cx.new(|cx| {
                            let mut conversation = Conversation::default();
                            conversation.register_thread(thread.clone(), cx);
                            conversation
                        });

                        let current = this.new_thread_view(
                            thread,
                            conversation.clone(),
                            resumed_without_history,
                            draft_initial_content.or(initial_content),
                            window,
                            cx,
                        );

                        if this.focus_handle.contains_focused(window, cx) {
                            current
                                .read(cx)
                                .message_editor
                                .focus_handle(cx)
                                .focus(window, cx);
                        }

                        this.root_session_id = Some(root_session_id.clone());
                        this.set_server_state(
                            ServerState::Connected(ConnectedServerState {
                                connection,
                                auth_state: AuthState::Ok,
                                active_id: Some(root_session_id.clone()),
                                threads: HashMap::from_iter([(root_session_id, current)]),
                                conversation,
                                _connection_entry_subscription: connection_entry_subscription,
                            }),
                            cx,
                        );
                    }
                    Err(err) => {
                        this.handle_load_error(
                            LoadError::Other(err.to_string().into()),
                            window,
                            cx,
                        );
                    }
                };
            })
            .log_err();
        });

        let loading_view = cx.new(|_cx| LoadingView {
            _load_task: load_task,
        });

        ServerState::Loading {
            _loading: loading_view,
            draft: loading_draft,
        }
    }

    fn handle_load_error(&mut self, err: LoadError, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.root_thread_view() {
            if view
                .read(cx)
                .message_editor
                .focus_handle(cx)
                .is_focused(window)
            {
                self.focus_handle.focus(window, cx)
            }
        }
        self.emit_load_error_telemetry(&err);
        self.set_server_state(ServerState::LoadError { error: err }, cx);
    }

    pub(super) fn handle_agent_servers_updated(
        &mut self,
        _agent_server_store: &Entity<project::AgentServerStore>,
        _event: &project::AgentServersUpdated,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let should_retry = match &self.server_state {
            ServerState::Loading { .. } => false,
            ServerState::LoadError { .. } => true,
            ServerState::Connected(connected) => {
                connected.auth_state.is_ok() && connected.has_thread_error(cx)
            }
        };

        if should_retry {
            if let Some(active) = self.root_thread_view() {
                active.update(cx, |active, cx| {
                    active.clear_thread_error(cx);
                });
            }
            self.reset(window, cx);
        }
    }

    pub(in crate::conversation_view) fn load_subagent_session(
        &mut self,
        subagent_id: acp::SessionId,
        parent_session_id: acp::SessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(connected) = self.as_connected() else {
            return;
        };
        if connected.threads.contains_key(&subagent_id)
            || !connected.connection.supports_load_session()
        {
            return;
        }
        let Some(parent_thread) = connected.threads.get(&parent_session_id) else {
            return;
        };
        let work_dirs = parent_thread
            .read(cx)
            .thread
            .read(cx)
            .work_dirs()
            .cloned()
            .unwrap_or_else(|| self.project.read(cx).default_path_list(cx));

        let subagent_thread_task = connected.connection.clone().load_session(
            subagent_id,
            self.project.clone(),
            work_dirs,
            None,
            cx,
        );

        cx.spawn_in(window, async move |this, cx| {
            let subagent_thread = subagent_thread_task.await?;
            this.update_in(cx, |this, window, cx| {
                let Some(conversation) = this
                    .as_connected()
                    .map(|connected| connected.conversation.clone())
                else {
                    return;
                };
                let subagent_session_id = subagent_thread.read(cx).session_id().clone();
                conversation.update(cx, |conversation, cx| {
                    conversation.register_thread(subagent_thread.clone(), cx);
                });
                let view =
                    this.new_thread_view(subagent_thread, conversation, false, None, window, cx);
                let Some(connected) = this.as_connected_mut() else {
                    return;
                };
                connected.threads.insert(subagent_session_id, view);
            })
        })
        .detach();
    }
}
