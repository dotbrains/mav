use super::*;

impl ConversationView {
    pub(super) fn handle_auth_required(
        this: WeakEntity<Self>,
        err: AuthRequired,
        agent_id: AgentId,
        connection: Rc<dyn AgentConnection>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (configuration_view, subscription) = if let Some(provider_id) = &err.provider_id {
            let registry = LanguageModelRegistry::global(cx);

            let sub = window.subscribe(&registry, cx, {
                let provider_id = provider_id.clone();
                let this = this.clone();
                move |_, ev, window, cx| {
                    if let language_model::Event::ProviderStateChanged(updated_provider_id) = &ev
                        && &provider_id == updated_provider_id
                        && LanguageModelRegistry::global(cx)
                            .read(cx)
                            .provider(&provider_id)
                            .map_or(false, |provider| provider.is_authenticated(cx))
                    {
                        this.update(cx, |this, cx| {
                            this.reset(window, cx);
                        })
                        .ok();
                    }
                }
            });

            let view = registry.read(cx).provider(&provider_id).map(|provider| {
                provider.configuration_view(
                    language_model::ConfigurationViewTargetAgent::Other(agent_id.0),
                    window,
                    cx,
                )
            });

            (view, Some(sub))
        } else {
            (None, None)
        };

        this.update(cx, |this, cx| {
            let description = err
                .description
                .map(|desc| cx.new(|cx| Markdown::new(desc.into(), None, None, cx)));
            let auth_state = AuthState::Unauthenticated {
                pending_auth_method: None,
                configuration_view,
                description,
                _subscription: subscription,
            };
            if let Some(connected) = this.as_connected_mut() {
                connected.auth_state = auth_state;
                cx.emit(StateChange);
                if let Some(view) = connected.active_view()
                    && view
                        .read(cx)
                        .message_editor
                        .focus_handle(cx)
                        .is_focused(window)
                {
                    this.focus_handle.focus(window, cx)
                }
            } else {
                this.set_server_state(
                    ServerState::Connected(ConnectedServerState {
                        auth_state,
                        active_id: None,
                        threads: HashMap::default(),
                        connection,
                        conversation: cx.new(|_cx| Conversation::default()),
                        _connection_entry_subscription: Subscription::new(|| {}),
                    }),
                    cx,
                );
            }
            cx.notify();
        })
        .ok();
    }

    pub(super) fn authenticate(
        &mut self,
        method: acp::AuthMethodId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let Some(connected) = self.as_connected_mut() else {
            return;
        };
        let connection = connected.connection.clone();

        let AuthState::Unauthenticated {
            configuration_view,
            pending_auth_method,
            ..
        } = &mut connected.auth_state
        else {
            return;
        };

        let agent_telemetry_id = connection.telemetry_id();

        if let Some(login_task) = connection.terminal_auth_task(&method, cx) {
            configuration_view.take();
            pending_auth_method.replace(method.clone());

            let project = self.project.clone();
            cx.emit(StateChange);
            cx.notify();
            self.auth_task = Some(cx.spawn_in(window, {
                async move |this, cx| {
                    let result = async {
                        let login = login_task.await?;
                        this.update_in(cx, |_this, window, cx| {
                            Self::spawn_external_agent_login(
                                login,
                                workspace,
                                project,
                                method.clone(),
                                false,
                                window,
                                cx,
                            )
                        })?
                        .await
                    }
                    .await;

                    match &result {
                        Ok(_) => telemetry::event!(
                            "Authenticate Agent Succeeded",
                            agent = agent_telemetry_id
                        ),
                        Err(_) => {
                            telemetry::event!(
                                "Authenticate Agent Failed",
                                agent = agent_telemetry_id,
                            )
                        }
                    }

                    this.update_in(cx, |this, window, cx| {
                        if let Err(err) = result {
                            if let Some(ConnectedServerState {
                                auth_state:
                                    AuthState::Unauthenticated {
                                        pending_auth_method,
                                        ..
                                    },
                                ..
                            }) = this.as_connected_mut()
                            {
                                pending_auth_method.take();
                                cx.emit(StateChange);
                            }
                            if let Some(active) = this.root_thread_view() {
                                active.update(cx, |active, cx| {
                                    active.handle_thread_error(err, cx);
                                })
                            }
                        } else {
                            this.reset(window, cx);
                        }
                        this.auth_task.take()
                    })
                    .ok();
                }
            }));
            return;
        }

        configuration_view.take();
        pending_auth_method.replace(method.clone());

        let authenticate = connection.authenticate(method, cx);
        cx.emit(StateChange);
        cx.notify();
        self.auth_task = Some(cx.spawn_in(window, {
            async move |this, cx| {
                let result = authenticate.await;

                match &result {
                    Ok(_) => telemetry::event!(
                        "Authenticate Agent Succeeded",
                        agent = agent_telemetry_id
                    ),
                    Err(_) => {
                        telemetry::event!("Authenticate Agent Failed", agent = agent_telemetry_id,)
                    }
                }

                this.update_in(cx, |this, window, cx| {
                    if let Err(err) = result {
                        if let Some(ConnectedServerState {
                            auth_state:
                                AuthState::Unauthenticated {
                                    pending_auth_method,
                                    ..
                                },
                            ..
                        }) = this.as_connected_mut()
                        {
                            pending_auth_method.take();
                            cx.emit(StateChange);
                        }
                        if let Some(active) = this.root_thread_view() {
                            active.update(cx, |active, cx| active.handle_thread_error(err, cx));
                        }
                    } else {
                        this.reset(window, cx);
                    }
                    this.auth_task.take()
                })
                .ok();
            }
        }));
    }

    fn spawn_external_agent_login(
        login: task::SpawnInTerminal,
        workspace: Entity<Workspace>,
        project: Entity<Project>,
        method: acp::AuthMethodId,
        previous_attempt: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let Some(terminal_panel) = workspace.read(cx).panel::<TerminalPanel>(cx) else {
            return Task::ready(Err(anyhow!("Terminal panel is unavailable")));
        };

        window.spawn(cx, async move |cx| {
            let mut task = login.clone();
            if let Some(cmd) = &task.command {
                // Have "node" command use Mav's managed Node runtime by default
                if cmd == "node" {
                    let resolved_node_runtime = project.update(cx, |project, cx| {
                        let agent_server_store = project.agent_server_store().clone();
                        agent_server_store.update(cx, |store, cx| {
                            store.node_runtime().map(|node_runtime| {
                                cx.background_spawn(async move { node_runtime.binary_path().await })
                            })
                        })
                    });

                    if let Some(resolve_task) = resolved_node_runtime {
                        if let Ok(node_path) = resolve_task.await {
                            task.command = Some(node_path.to_string_lossy().to_string());
                        }
                    }
                }
            }
            task.shell = task::Shell::WithArguments {
                program: task.command.take().expect("login command should be set"),
                args: std::mem::take(&mut task.args),
                title_override: None,
            };

            let terminal = terminal_panel
                .update_in(cx, |terminal_panel, window, cx| {
                    terminal_panel.spawn_task(&task, window, cx)
                })?
                .await?;

            let success_patterns = match method.0.as_ref() {
                "claude-login" | GEMINI_TERMINAL_AUTH_METHOD_ID => vec![
                    "Login successful".to_string(),
                    "Type your message".to_string(),
                ],
                _ => Vec::new(),
            };
            if success_patterns.is_empty() {
                let exit_status = terminal
                    .read_with(cx, |terminal, cx| terminal.wait_for_completed_task(cx))?
                    .await;

                match exit_status {
                    Some(status) if status.success() => Ok(()),
                    Some(status) => Err(anyhow!(
                        "Login command failed with exit code: {:?}",
                        status.code()
                    )),
                    None => Err(anyhow!("Login command terminated without exit status")),
                }
            } else {
                let mut exit_status = terminal
                    .read_with(cx, |terminal, cx| terminal.wait_for_completed_task(cx))?
                    .fuse();

                let logged_in = cx
                    .spawn({
                        let terminal = terminal.clone();
                        async move |cx| {
                            loop {
                                cx.background_executor().timer(Duration::from_secs(1)).await;
                                let content =
                                    terminal.update(cx, |terminal, _cx| terminal.get_content())?;
                                if success_patterns
                                    .iter()
                                    .any(|pattern| content.contains(pattern))
                                {
                                    return anyhow::Ok(());
                                }
                            }
                        }
                    })
                    .fuse();
                futures::pin_mut!(logged_in);
                futures::select_biased! {
                    result = logged_in => {
                        if let Err(e) = result {
                            log::error!("{e}");
                            return Err(anyhow!("exited before logging in"));
                        }
                    }
                    _ = exit_status => {
                        if !previous_attempt
                            && project.read_with(cx, |project, _| project.is_via_remote_server())
                            && method.0.as_ref() == GEMINI_TERMINAL_AUTH_METHOD_ID
                        {
                            return cx
                                .update(|window, cx| {
                                    Self::spawn_external_agent_login(
                                        login,
                                        workspace,
                                        project.clone(),
                                        method,
                                        true,
                                        window,
                                        cx,
                                    )
                                })?
                                .await;
                        }
                        return Err(anyhow!("exited before logging in"));
                    }
                }
                terminal.update(cx, |terminal, _| terminal.kill_active_task())?;
                Ok(())
            }
        })
    }

    pub fn reauthenticate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let agent_id = self.agent.agent_id();
        if let Some(active) = self.root_thread_view() {
            active.update(cx, |active, cx| active.clear_thread_error(cx));
        }
        let this = cx.weak_entity();
        let Some(connection) = self.as_connected().map(|c| c.connection.clone()) else {
            debug_panic!("This should not be possible");
            return;
        };
        window.defer(cx, |window, cx| {
            Self::handle_auth_required(this, AuthRequired::new(), agent_id, connection, window, cx);
        })
    }

    pub fn logout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.supports_logout() {
            return;
        }

        if let Some(active) = self.root_thread_view() {
            active.update(cx, |active, cx| active.clear_thread_error(cx));
        }
        let Some(connection) = self
            .as_connected()
            .map(|connected| connected.connection.clone())
        else {
            return;
        };
        let logout = connection.logout(cx);
        self.auth_task = Some(cx.spawn_in(window, {
            async move |this, cx| {
                let result = logout.await;
                this.update_in(cx, |this, window, cx| {
                    if let Err(err) = result {
                        if let Some(active) = this.root_thread_view() {
                            active.update(cx, |active, cx| active.handle_thread_error(err, cx));
                        }
                    } else if let Some(connected) = this.as_connected_mut() {
                        connected.auth_state = AuthState::Unauthenticated {
                            description: None,
                            configuration_view: None,
                            pending_auth_method: None,
                            _subscription: None,
                        };
                        cx.emit(StateChange);
                        if let Some(view) = connected.active_view()
                            && view
                                .read(cx)
                                .message_editor
                                .focus_handle(cx)
                                .is_focused(window)
                        {
                            this.focus_handle.focus(window, cx)
                        }
                        cx.notify();
                    }
                    drop(this.auth_task.take());
                })
                .ok();
            }
        }));
    }
}
