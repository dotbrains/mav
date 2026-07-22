use super::*;

impl Session {
    pub fn adapter_client(&self) -> Option<Arc<DebugAdapterClient>> {
        match self.state {
            SessionState::Running(ref local) => Some(local.client.clone()),
            SessionState::Booting(_) => None,
        }
    }

    pub fn has_ever_stopped(&self) -> bool {
        self.state.has_ever_stopped()
    }

    pub fn step_over(
        &mut self,
        thread_id: ThreadId,
        granularity: SteppingGranularity,
        cx: &mut Context<Self>,
    ) {
        self.select_historic_snapshot(None, cx);

        let supports_single_thread_execution_requests =
            self.capabilities.supports_single_thread_execution_requests;
        let supports_stepping_granularity = self
            .capabilities
            .supports_stepping_granularity
            .unwrap_or_default();

        let command = NextCommand {
            inner: StepCommand {
                thread_id: thread_id.0,
                granularity: supports_stepping_granularity.then(|| granularity),
                single_thread: supports_single_thread_execution_requests,
            },
        };

        self.active_snapshot.thread_states.process_step(thread_id);
        self.request(
            command,
            Self::on_step_response::<NextCommand>(thread_id),
            cx,
        )
        .detach();
    }

    pub fn step_in(
        &mut self,
        thread_id: ThreadId,
        granularity: SteppingGranularity,
        cx: &mut Context<Self>,
    ) {
        self.select_historic_snapshot(None, cx);

        let supports_single_thread_execution_requests =
            self.capabilities.supports_single_thread_execution_requests;
        let supports_stepping_granularity = self
            .capabilities
            .supports_stepping_granularity
            .unwrap_or_default();

        let command = StepInCommand {
            inner: StepCommand {
                thread_id: thread_id.0,
                granularity: supports_stepping_granularity.then(|| granularity),
                single_thread: supports_single_thread_execution_requests,
            },
        };

        self.active_snapshot.thread_states.process_step(thread_id);
        self.request(
            command,
            Self::on_step_response::<StepInCommand>(thread_id),
            cx,
        )
        .detach();
    }

    pub fn step_out(
        &mut self,
        thread_id: ThreadId,
        granularity: SteppingGranularity,
        cx: &mut Context<Self>,
    ) {
        self.select_historic_snapshot(None, cx);

        let supports_single_thread_execution_requests =
            self.capabilities.supports_single_thread_execution_requests;
        let supports_stepping_granularity = self
            .capabilities
            .supports_stepping_granularity
            .unwrap_or_default();

        let command = StepOutCommand {
            inner: StepCommand {
                thread_id: thread_id.0,
                granularity: supports_stepping_granularity.then(|| granularity),
                single_thread: supports_single_thread_execution_requests,
            },
        };

        self.active_snapshot.thread_states.process_step(thread_id);
        self.request(
            command,
            Self::on_step_response::<StepOutCommand>(thread_id),
            cx,
        )
        .detach();
    }

    pub fn step_back(
        &mut self,
        thread_id: ThreadId,
        granularity: SteppingGranularity,
        cx: &mut Context<Self>,
    ) {
        self.select_historic_snapshot(None, cx);

        let supports_single_thread_execution_requests =
            self.capabilities.supports_single_thread_execution_requests;
        let supports_stepping_granularity = self
            .capabilities
            .supports_stepping_granularity
            .unwrap_or_default();

        let command = StepBackCommand {
            inner: StepCommand {
                thread_id: thread_id.0,
                granularity: supports_stepping_granularity.then(|| granularity),
                single_thread: supports_single_thread_execution_requests,
            },
        };

        self.active_snapshot.thread_states.process_step(thread_id);

        self.request(
            command,
            Self::on_step_response::<StepBackCommand>(thread_id),
            cx,
        )
        .detach();
    }

    pub fn stack_frames(
        &mut self,
        thread_id: ThreadId,
        cx: &mut Context<Self>,
    ) -> Result<Vec<StackFrame>> {
        if self.active_snapshot.thread_states.thread_status(thread_id) == ThreadStatus::Stopped
            && self.requests.contains_key(&ThreadsCommand.type_id())
            && self.active_snapshot.threads.contains_key(&thread_id)
        // ^ todo(debugger): We need a better way to check that we're not querying stale data
        // We could still be using an old thread id and have sent a new thread's request
        // This isn't the biggest concern right now because it hasn't caused any issues outside of tests
        // But it very well could cause a minor bug in the future that is hard to track down
        {
            self.fetch(
                super::dap_command::StackTraceCommand {
                    thread_id: thread_id.0,
                    start_frame: None,
                    levels: None,
                },
                move |this, stack_frames, cx| {
                    let entry =
                        this.active_snapshot
                            .threads
                            .entry(thread_id)
                            .and_modify(|thread| match &stack_frames {
                                Ok(stack_frames) => {
                                    thread.stack_frames = stack_frames
                                        .iter()
                                        .cloned()
                                        .map(StackFrame::from)
                                        .collect();
                                    thread.stack_frames_error = None;
                                }
                                Err(error) => {
                                    thread.stack_frames.clear();
                                    thread.stack_frames_error = Some(error.to_string().into());
                                }
                            });
                    debug_assert!(
                        matches!(entry, indexmap::map::Entry::Occupied(_)),
                        "Sent request for thread_id that doesn't exist"
                    );
                    if let Ok(stack_frames) = stack_frames {
                        this.active_snapshot.stack_frames.extend(
                            stack_frames
                                .into_iter()
                                .filter(|frame| {
                                    // Workaround for JavaScript debug adapter sending out "fake" stack frames for delineating await points. This is fine,
                                    // except that they always use an id of 0 for it, which collides with other (valid) stack frames.
                                    !(frame.id == 0
                                        && frame.line == 0
                                        && frame.column == 0
                                        && frame.presentation_hint
                                            == Some(StackFramePresentationHint::Label))
                                })
                                .map(|frame| (frame.id, StackFrame::from(frame))),
                        );
                    }

                    this.invalidate_command_type::<ScopesCommand>();
                    this.invalidate_command_type::<VariablesCommand>();

                    cx.emit(SessionEvent::StackTrace);
                },
                cx,
            );
        }

        match self.session_state().threads.get(&thread_id) {
            Some(thread) => {
                if let Some(error) = &thread.stack_frames_error {
                    Err(anyhow!(error.to_string()))
                } else {
                    Ok(thread.stack_frames.clone())
                }
            }
            None => Ok(Vec::new()),
        }
    }

    pub fn scopes(&mut self, stack_frame_id: u64, cx: &mut Context<Self>) -> &[dap::Scope] {
        if self.requests.contains_key(&TypeId::of::<ThreadsCommand>())
            && self
                .requests
                .contains_key(&TypeId::of::<StackTraceCommand>())
        {
            self.fetch(
                ScopesCommand { stack_frame_id },
                move |this, scopes, cx| {
                    let Some(scopes) = scopes.log_err() else {
                        return
                    };

                    for scope in scopes.iter() {
                        this.variables(scope.variables_reference, cx);
                    }

                    let entry = this
                        .active_snapshot
                        .stack_frames
                        .entry(stack_frame_id)
                        .and_modify(|stack_frame| {
                            stack_frame.scopes = scopes;
                        });

                    cx.emit(SessionEvent::Variables);

                    debug_assert!(
                        matches!(entry, indexmap::map::Entry::Occupied(_)),
                        "Sent scopes request for stack_frame_id that doesn't exist or hasn't been fetched"
                    );
                },
                cx,
            );
        }

        self.session_state()
            .stack_frames
            .get(&stack_frame_id)
            .map(|frame| frame.scopes.as_slice())
            .unwrap_or_default()
    }

    pub fn variables_by_stack_frame_id(
        &self,
        stack_frame_id: StackFrameId,
        globals: bool,
        locals: bool,
    ) -> Vec<dap::Variable> {
        let state = self.session_state();
        let Some(stack_frame) = state.stack_frames.get(&stack_frame_id) else {
            return Vec::new();
        };

        stack_frame
            .scopes
            .iter()
            .filter(|scope| {
                (scope.name.to_lowercase().contains("local") && locals)
                    || (scope.name.to_lowercase().contains("global") && globals)
            })
            .filter_map(|scope| state.variables.get(&scope.variables_reference))
            .flatten()
            .cloned()
            .collect()
    }
}
