use super::*;

impl AcpThread {
    #[cfg(any(test, feature = "test-support"))]
    pub fn send_raw(
        &mut self,
        message: &str,
        cx: &mut Context<Self>,
    ) -> BoxFuture<'static, Result<Option<acp::PromptResponse>>> {
        self.send(vec![message.into()], cx)
    }

    pub fn send(
        &mut self,
        message: Vec<acp::ContentBlock>,
        cx: &mut Context<Self>,
    ) -> BoxFuture<'static, Result<Option<acp::PromptResponse>>> {
        self.send_inner(message, true, cx)
    }

    /// Sends a prompt without displaying a user-message bubble for it.
    /// This is used for native slash commands (e.g. `/compact`) that run a turn
    /// which produces its own thread entry (like the compaction summary). The
    /// typed command isn't sent to the model as an ordinary user turn.
    pub fn send_command(
        &mut self,
        message: Vec<acp::ContentBlock>,
        cx: &mut Context<Self>,
    ) -> BoxFuture<'static, Result<Option<acp::PromptResponse>>> {
        self.send_inner(message, false, cx)
    }

    fn send_inner(
        &mut self,
        message: Vec<acp::ContentBlock>,
        push_user_message: bool,
        cx: &mut Context<Self>,
    ) -> BoxFuture<'static, Result<Option<acp::PromptResponse>>> {
        let block = ContentBlock::new_combined(
            message.clone(),
            self.project.read(cx).languages().clone(),
            self.project.read(cx).path_style(cx),
            cx,
        );
        let request = acp::PromptRequest::new(self.session_id.clone(), message.clone());
        let git_store = self.project.read(cx).git_store().clone();

        let client_user_message_ids = self.connection.client_user_message_ids(cx);
        let client_id = client_user_message_ids
            .as_ref()
            .map(|client_user_message_ids| client_user_message_ids.new_id());

        self.run_turn(cx, async move |this, cx| {
            if push_user_message {
                this.update(cx, |this, cx| {
                    this.push_entry(
                        AgentThreadEntry::UserMessage(UserMessage {
                            protocol_id: None,
                            client_id: client_id.clone(),
                            is_optimistic: true,
                            content: block,
                            chunks: message,
                            checkpoint: None,
                            indented: false,
                        }),
                        cx,
                    );
                })
                .ok();

                let old_checkpoint = git_store
                    .update(cx, |git, cx| git.checkpoint(cx))
                    .await
                    .context("failed to get old checkpoint")
                    .log_err();
                this.update(cx, |this, _cx| {
                    if let Some((_ix, message)) = this.last_user_message() {
                        message.checkpoint = old_checkpoint.map(|git_checkpoint| Checkpoint {
                            git_checkpoint,
                            show: false,
                        });
                    }
                })
                .ok();
            }

            this.update(cx, |this, cx| {
                if let (Some(prompt), Some(client_id)) = (client_user_message_ids, client_id) {
                    prompt.prompt(client_id, request, cx)
                } else {
                    this.connection.prompt(request, cx)
                }
            })?
            .await
        })
    }

    pub fn can_retry(&self, cx: &App) -> bool {
        self.connection.retry(&self.session_id, cx).is_some()
    }

    pub fn retry(
        &mut self,
        cx: &mut Context<Self>,
    ) -> BoxFuture<'static, Result<Option<acp::PromptResponse>>> {
        self.run_turn(cx, async move |this, cx| {
            this.update(cx, |this, cx| {
                this.connection
                    .retry(&this.session_id, cx)
                    .map(|retry| retry.run(cx))
            })?
            .context("retrying a session is not supported")?
            .await
        })
    }

    fn run_turn(
        &mut self,
        cx: &mut Context<Self>,
        f: impl 'static + AsyncFnOnce(WeakEntity<Self>, &mut AsyncApp) -> Result<acp::PromptResponse>,
    ) -> BoxFuture<'static, Result<Option<acp::PromptResponse>>> {
        self.clear_completed_plan_entries(cx);
        self.had_error = false;

        let (tx, rx) = oneshot::channel();
        let cancel_task = self.cancel(cx);

        self.turn_id += 1;
        let turn_id = self.turn_id;
        self.running_turn = Some(RunningTurn {
            id: turn_id,
            send_task: cx.spawn(async move |this, cx| {
                cancel_task.await;
                tx.send(f(this, cx).await).ok();
            }),
        });
        cx.emit(AcpThreadEvent::StatusChanged);

        cx.spawn(async move |this, cx| {
            let response = rx.await;

            this.update(cx, |this, cx| this.update_last_checkpoint(cx))?
                .await?;

            this.update(cx, |this, cx| {
                if this.parent_session_id.is_none() {
                    this.project
                        .update(cx, |project, cx| project.set_agent_location(None, cx));
                }

                let is_same_turn = this
                    .running_turn
                    .as_ref()
                    .is_some_and(|turn| turn_id == turn.id);

                // If the user submitted a follow up message, running_turn might
                // already point to a different turn. Therefore we only want to
                // take the task if it's the same turn. We do this before the
                // dropped-tx guard below so the panel exits its generating
                // state even when the send_task is cancelled before tx.send().
                if is_same_turn {
                    this.running_turn.take();
                    cx.emit(AcpThreadEvent::StatusChanged);
                }

                let Ok(response) = response else {
                    // tx dropped, just return
                    return Ok(None);
                };

                match response {
                    Ok(r) => {
                        Self::flush_streaming_text(&mut this.streaming_text_buffer, cx);

                        if r.stop_reason == acp::StopReason::MaxTokens {
                            this.had_error = true;
                            cx.emit(AcpThreadEvent::Error);
                            log::error!("Max tokens reached. Usage: {:?}", this.token_usage);

                            let exceeded_max_output_tokens =
                                this.token_usage.as_ref().is_some_and(|u| {
                                    u.max_output_tokens
                                        .is_some_and(|max| u.output_tokens >= max)
                                });

                            if exceeded_max_output_tokens {
                                log::error!(
                                    "Max output tokens reached. Usage: {:?}",
                                    this.token_usage
                                );
                            } else {
                                log::error!("Max tokens reached. Usage: {:?}", this.token_usage);
                            }
                            if is_same_turn {
                                this.mark_pending_entries_as_canceled(cx);
                            }
                            return Err(anyhow!(MaxOutputTokensError));
                        }

                        let canceled = matches!(r.stop_reason, acp::StopReason::Cancelled);
                        if canceled && is_same_turn {
                            this.mark_pending_entries_as_canceled(cx);
                        }

                        if !canceled {
                            this.snapshot_completed_plan(cx);
                        }

                        // Handle refusal - distinguish between user prompt and tool call refusals
                        if let acp::StopReason::Refusal = r.stop_reason {
                            this.had_error = true;
                            if let Some((user_msg_ix, _)) = this.last_user_message() {
                                // Check if there's a completed tool call with results after the last user message
                                // This indicates the refusal is in response to tool output, not the user's prompt
                                let has_completed_tool_call_after_user_msg =
                                    this.entries.iter().skip(user_msg_ix + 1).any(|entry| {
                                        if let AgentThreadEntry::ToolCall(tool_call) = entry {
                                            // Check if the tool call has completed and has output
                                            matches!(tool_call.status, ToolCallStatus::Completed)
                                                && tool_call.raw_output.is_some()
                                        } else {
                                            false
                                        }
                                    });

                                if has_completed_tool_call_after_user_msg {
                                    // Refusal is due to tool output - don't truncate, just notify
                                    // The model refused based on what the tool returned
                                    cx.emit(AcpThreadEvent::Refusal);
                                } else {
                                    // User prompt was refused - truncate back to before the user message
                                    let range = user_msg_ix..this.entries.len();
                                    if range.start < range.end {
                                        this.entries.truncate(user_msg_ix);
                                        cx.emit(AcpThreadEvent::EntriesRemoved(range));
                                    }
                                    cx.emit(AcpThreadEvent::Refusal);
                                }
                            } else {
                                // No user message found, treat as general refusal
                                cx.emit(AcpThreadEvent::Refusal);
                            }
                        }

                        if cx.has_flag::<AcpBetaFeatureFlag>()
                            && let Some(response_usage) = &r.usage
                        {
                            let usage = this.token_usage.get_or_insert_with(Default::default);
                            usage.input_tokens = response_usage.input_tokens;
                            usage.output_tokens = response_usage.output_tokens;
                            cx.emit(AcpThreadEvent::TokenUsageUpdated);
                        }

                        cx.emit(AcpThreadEvent::Stopped(r.stop_reason));
                        Ok(Some(r))
                    }
                    Err(e) => {
                        Self::flush_streaming_text(&mut this.streaming_text_buffer, cx);
                        if is_same_turn {
                            this.mark_pending_entries_as_canceled(cx);
                        }
                        this.had_error = true;
                        cx.emit(AcpThreadEvent::Error);
                        log::error!("Error in run turn: {:?}", e);
                        Err(e)
                    }
                }
            })?
        })
        .boxed()
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let Some(turn) = self.running_turn.take() else {
            return Task::ready(());
        };
        self.connection.cancel(&self.session_id, cx);

        Self::flush_streaming_text(&mut self.streaming_text_buffer, cx);
        self.mark_pending_entries_as_canceled(cx);
        cx.emit(AcpThreadEvent::StatusChanged);

        // Wait for the send task to complete
        cx.background_spawn(turn.send_task)
    }

    fn mark_pending_entries_as_canceled(&mut self, cx: &mut Context<Self>) {
        for (ix, entry) in self.entries.iter_mut().enumerate() {
            match entry {
                AgentThreadEntry::ToolCall(call) => {
                    let cancel = matches!(
                        call.status,
                        ToolCallStatus::Pending
                            | ToolCallStatus::WaitingForConfirmation { .. }
                            | ToolCallStatus::InProgress
                    );
                    if cancel {
                        call.status = ToolCallStatus::Canceled;
                        cx.emit(AcpThreadEvent::EntryUpdated(ix));
                    }
                }
                AgentThreadEntry::ContextCompaction(compaction) => {
                    if compaction.status == ContextCompactionStatus::InProgress {
                        compaction.status = ContextCompactionStatus::Canceled;
                        cx.emit(AcpThreadEvent::EntryUpdated(ix));
                    }
                }
                _ => {}
            }
        }
    }
}
