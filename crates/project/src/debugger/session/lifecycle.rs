use super::*;

impl Session {
    pub(crate) fn new(
        breakpoint_store: Entity<BreakpointStore>,
        session_id: SessionId,
        parent_session: Option<Entity<Session>>,
        label: Option<SharedString>,
        adapter: DebugAdapterName,
        task_context: SharedTaskContext,
        quirks: SessionQuirks,
        remote_client: Option<Entity<RemoteClient>>,
        node_runtime: Option<NodeRuntime>,
        http_client: Option<Arc<dyn HttpClient>>,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new::<Self>(|cx| {
            cx.subscribe(&breakpoint_store, |this, store, event, cx| match event {
                BreakpointStoreEvent::BreakpointsUpdated(path, reason) => {
                    if let Some(local) = (!this.ignore_breakpoints)
                        .then(|| this.as_running_mut())
                        .flatten()
                    {
                        local
                            .send_breakpoints_from_path(path.clone(), *reason, &store, cx)
                            .detach();
                    };
                }
                BreakpointStoreEvent::BreakpointsCleared(paths) => {
                    if let Some(local) = (!this.ignore_breakpoints)
                        .then(|| this.as_running_mut())
                        .flatten()
                    {
                        local.unset_breakpoints_from_paths(paths, cx).detach();
                    }
                }
                BreakpointStoreEvent::SetDebugLine | BreakpointStoreEvent::ClearDebugLines => {}
            })
            .detach();

            Self {
                state: SessionState::Booting(None),
                snapshots: VecDeque::with_capacity(DEBUG_HISTORY_LIMIT),
                selected_snapshot_index: None,
                active_snapshot: Default::default(),
                id: session_id,
                child_session_ids: HashSet::default(),
                parent_session,
                capabilities: Capabilities::default(),
                watchers: HashMap::default(),
                output_token: OutputToken(0),
                output: circular_buffer::CircularBuffer::boxed(),
                requests: Default::default(),
                background_tasks: Vec::default(),
                restart_task: None,
                is_session_terminated: false,
                ignore_breakpoints: false,
                breakpoint_store,
                data_breakpoints: Default::default(),
                exception_breakpoints: Default::default(),
                label,
                adapter,
                task_context,
                memory: memory::Memory::new(),
                quirks,
                remote_client,
                node_runtime,
                http_client,
                companion_port: None,
            }
        })
    }

    pub fn task_context(&self) -> &SharedTaskContext {
        &self.task_context
    }

    pub fn worktree(&self) -> Option<Entity<Worktree>> {
        match &self.state {
            SessionState::Booting(_) => None,
            SessionState::Running(local_mode) => local_mode.worktree.upgrade(),
        }
    }

    pub fn boot(
        &mut self,
        binary: DebugAdapterBinary,
        worktree: Entity<Worktree>,
        dap_store: WeakEntity<DapStore>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let (message_tx, mut message_rx) = futures::channel::mpsc::unbounded();
        let (initialized_tx, initialized_rx) = futures::channel::oneshot::channel();

        let background_tasks = vec![cx.spawn(async move |this: WeakEntity<Session>, cx| {
            let mut initialized_tx = Some(initialized_tx);
            while let Some(message) = message_rx.next().await {
                if let Message::Event(event) = message {
                    if let Events::Initialized(_) = *event {
                        if let Some(tx) = initialized_tx.take() {
                            tx.send(()).ok();
                        }
                    } else {
                        let Ok(_) = this.update(cx, |session, cx| {
                            session.handle_dap_event(event, cx);
                        }) else {
                            break;
                        };
                    }
                } else if let Message::Request(request) = message {
                    let Ok(_) = this.update(cx, |this, cx| {
                        if request.command == StartDebugging::COMMAND {
                            this.handle_start_debugging_request(request, cx)
                                .detach_and_log_err(cx);
                        } else if request.command == RunInTerminal::COMMAND {
                            this.handle_run_in_terminal_request(request, cx)
                                .detach_and_log_err(cx);
                        }
                    }) else {
                        break;
                    };
                }
            }
        })];
        self.background_tasks = background_tasks;
        let id = self.id;
        let parent_session = self.parent_session.clone();

        cx.spawn(async move |this, cx| {
            let mode = RunningMode::new(
                id,
                parent_session,
                worktree.downgrade(),
                binary.clone(),
                message_tx,
                cx,
            )
            .await?;
            this.update(cx, |this, cx| {
                match &mut this.state {
                    SessionState::Booting(task) if task.is_some() => {
                        task.take().unwrap().detach_and_log_err(cx);
                    }
                    SessionState::Booting(_) => {}
                    SessionState::Running(_) => {
                        debug_panic!("Attempting to boot a session that is already running");
                    }
                };
                this.state = SessionState::Running(mode);
                cx.emit(SessionStateEvent::Running);
            })?;

            this.update(cx, |session, cx| session.request_initialize(cx))?
                .await?;

            let result = this
                .update(cx, |session, cx| {
                    session.initialize_sequence(initialized_rx, dap_store.clone(), cx)
                })?
                .await;

            if result.is_err() {
                let mut console = this.update(cx, |session, cx| session.console_output(cx))?;

                console
                    .send(format!(
                        "Tried to launch debugger with: {}",
                        serde_json::to_string_pretty(&binary.request_args.configuration)
                            .unwrap_or_default(),
                    ))
                    .await
                    .ok();
            }

            result
        })
    }

    pub fn session_id(&self) -> SessionId {
        self.id
    }

    pub fn child_session_ids(&self) -> HashSet<SessionId> {
        self.child_session_ids.clone()
    }

    pub fn add_child_session_id(&mut self, session_id: SessionId) {
        self.child_session_ids.insert(session_id);
    }

    pub fn remove_child_session_id(&mut self, session_id: SessionId) {
        self.child_session_ids.remove(&session_id);
    }

    pub fn parent_id(&self, cx: &App) -> Option<SessionId> {
        self.parent_session
            .as_ref()
            .map(|session| session.read(cx).id)
    }

    pub fn parent_session(&self) -> Option<&Entity<Self>> {
        self.parent_session.as_ref()
    }

    pub fn on_app_quit(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let Some(client) = self.adapter_client() else {
            return Task::ready(());
        };

        let supports_terminate = self
            .capabilities
            .support_terminate_debuggee
            .unwrap_or(false);

        cx.background_spawn(async move {
            if supports_terminate {
                client
                    .request::<dap::requests::Terminate>(dap::TerminateArguments {
                        restart: Some(false),
                    })
                    .await
                    .ok();
            } else {
                client
                    .request::<dap::requests::Disconnect>(dap::DisconnectArguments {
                        restart: Some(false),
                        terminate_debuggee: Some(true),
                        suspend_debuggee: Some(false),
                    })
                    .await
                    .ok();
            }
        })
    }

    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    pub fn binary(&self) -> Option<&DebugAdapterBinary> {
        match &self.state {
            SessionState::Booting(_) => None,
            SessionState::Running(running_mode) => Some(&running_mode.binary),
        }
    }

    pub fn adapter(&self) -> DebugAdapterName {
        self.adapter.clone()
    }

    pub fn label(&self) -> Option<SharedString> {
        self.label.clone()
    }

    pub fn is_terminated(&self) -> bool {
        self.is_session_terminated
    }

    pub fn console_output(&mut self, cx: &mut Context<Self>) -> mpsc::UnboundedSender<String> {
        let (tx, mut rx) = mpsc::unbounded();

        cx.spawn(async move |this, cx| {
            while let Some(output) = rx.next().await {
                this.update(cx, |this, _| {
                    let event = dap::OutputEvent {
                        category: None,
                        output,
                        group: None,
                        variables_reference: None,
                        source: None,
                        line: None,
                        column: None,
                        data: None,
                        location_reference: None,
                    };
                    this.push_output(event);
                })?;
            }
            anyhow::Ok(())
        })
        .detach();

        tx
    }

    pub fn is_started(&self) -> bool {
        match &self.state {
            SessionState::Booting(_) => false,
            SessionState::Running(running) => running.is_started,
        }
    }

    pub fn is_building(&self) -> bool {
        matches!(self.state, SessionState::Booting(_))
    }

    pub fn as_running_mut(&mut self) -> Option<&mut RunningMode> {
        match &mut self.state {
            SessionState::Running(local_mode) => Some(local_mode),
            SessionState::Booting(_) => None,
        }
    }

    pub fn as_running(&self) -> Option<&RunningMode> {
        match &self.state {
            SessionState::Running(local_mode) => Some(local_mode),
            SessionState::Booting(_) => None,
        }
    }
}
