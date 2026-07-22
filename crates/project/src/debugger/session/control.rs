use super::*;

impl Session {
    fn fallback_to_manual_restart(
        &mut self,
        res: Result<()>,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        if res.log_err().is_none() {
            cx.emit(SessionStateEvent::Restart);
            return None;
        }
        Some(())
    }

    fn empty_response(&mut self, res: Result<()>, _cx: &mut Context<Self>) -> Option<()> {
        res.log_err()?;
        Some(())
    }

    fn on_step_response<T: LocalDapCommand + PartialEq + Eq + Hash>(
        thread_id: ThreadId,
    ) -> impl FnOnce(&mut Self, Result<T::Response>, &mut Context<Self>) -> Option<T::Response> + 'static
    {
        move |this, response, cx| match response.log_err() {
            Some(response) => {
                this.breakpoint_store.update(cx, |store, cx| {
                    store.remove_active_position(Some(this.session_id()), cx)
                });
                Some(response)
            }
            None => {
                this.active_snapshot.thread_states.stop_thread(thread_id);
                cx.notify();
                None
            }
        }
    }

    fn clear_active_debug_line_response(
        &mut self,
        response: Result<()>,
        cx: &mut Context<Session>,
    ) -> Option<()> {
        response.log_err()?;
        self.clear_active_debug_line(cx);
        Some(())
    }

    fn clear_active_debug_line(&mut self, cx: &mut Context<Session>) {
        self.breakpoint_store.update(cx, |store, cx| {
            store.remove_active_position(Some(self.id), cx)
        });
    }

    pub fn pause_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.request(
            PauseCommand {
                thread_id: thread_id.0,
            },
            Self::empty_response,
            cx,
        )
        .detach();
    }

    pub fn restart_stack_frame(&mut self, stack_frame_id: u64, cx: &mut Context<Self>) {
        self.request(
            RestartStackFrameCommand { stack_frame_id },
            Self::empty_response,
            cx,
        )
        .detach();
    }

    pub fn restart(&mut self, args: Option<Value>, cx: &mut Context<Self>) {
        if self.restart_task.is_some() || self.as_running().is_none() {
            return;
        }

        let supports_dap_restart =
            self.capabilities.supports_restart_request.unwrap_or(false) && !self.is_terminated();

        self.restart_task = Some(cx.spawn(async move |this, cx| {
            this.update(cx, |session, cx| {
                if supports_dap_restart {
                    session.request(
                        RestartCommand {
                            raw: args.unwrap_or(Value::Null),
                        },
                        Self::fallback_to_manual_restart,
                        cx,
                    )
                } else {
                    cx.emit(SessionStateEvent::Restart);
                    Task::ready(None)
                }
            })
            .unwrap_or_else(|_| Task::ready(None))
            .await;

            this.update(cx, |session, _cx| {
                session.restart_task = None;
            })
            .ok();
        }));
    }

    pub fn shutdown(&mut self, cx: &mut Context<Self>) -> Task<()> {
        if self.is_session_terminated {
            return Task::ready(());
        }

        self.is_session_terminated = true;
        self.active_snapshot.thread_states.exit_all_threads();
        cx.notify();

        let task = match &mut self.state {
            SessionState::Running(_) => {
                if self
                    .capabilities
                    .supports_terminate_request
                    .unwrap_or_default()
                {
                    self.request(
                        TerminateCommand {
                            restart: Some(false),
                        },
                        Self::clear_active_debug_line_response,
                        cx,
                    )
                } else {
                    self.request(
                        DisconnectCommand {
                            restart: Some(false),
                            terminate_debuggee: Some(true),
                            suspend_debuggee: Some(false),
                        },
                        Self::clear_active_debug_line_response,
                        cx,
                    )
                }
            }
            SessionState::Booting(build_task) => {
                build_task.take();
                Task::ready(Some(()))
            }
        };

        cx.emit(SessionStateEvent::Shutdown);

        cx.spawn(async move |this, cx| {
            task.await;
            let _ = this.update(cx, |this, _| {
                if let Some(adapter_client) = this.adapter_client() {
                    adapter_client.kill();
                }
            });
        })
    }

    pub fn completions(
        &mut self,
        query: CompletionsQuery,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<dap::CompletionItem>>> {
        let task = self.request(query, |_, result, _| result.log_err(), cx);

        cx.background_executor().spawn(async move {
            anyhow::Ok(
                task.await
                    .map(|response| response.targets)
                    .context("failed to fetch completions")?,
            )
        })
    }

    pub fn continue_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.select_historic_snapshot(None, cx);

        let supports_single_thread_execution_requests =
            self.capabilities.supports_single_thread_execution_requests;
        self.active_snapshot
            .thread_states
            .continue_thread(thread_id);
        self.request(
            ContinueCommand {
                args: ContinueArguments {
                    thread_id: thread_id.0,
                    single_thread: supports_single_thread_execution_requests,
                },
            },
            Self::on_step_response::<ContinueCommand>(thread_id),
            cx,
        )
        .detach();
    }
}
