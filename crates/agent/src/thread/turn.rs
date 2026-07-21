use super::*;

impl Thread {
    pub(super) fn run_turn(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<mpsc::UnboundedReceiver<Result<ThreadEvent>>> {
        // Flush the old pending message synchronously before cancelling,
        // to avoid a race where the detached cancel task might flush the NEW
        // turn's pending message instead of the old one.
        self.flush_pending_message(cx);
        self.cancel(cx).detach();

        let (events_tx, events_rx) = mpsc::unbounded::<Result<ThreadEvent>>();
        let event_stream = ThreadEventStream(events_tx);
        let message_ix = self.messages.len().saturating_sub(1);
        self.clear_summary();
        let tools = self.enabled_tools(cx);
        let (cancellation_tx, mut cancellation_rx) = watch::channel(false);
        let task = cx.spawn({
            let event_stream = event_stream.clone();
            async move |this, cx| {
                log::debug!("Starting agent turn execution");

                let turn_result =
                    Self::run_turn_internal(&this, &event_stream, cancellation_rx.clone(), cx)
                        .await;

                // Check if we were cancelled - if so, cancel() already took running_turn
                // and we shouldn't touch it (it might be a NEW turn now)
                let was_cancelled = *cancellation_rx.borrow();
                if was_cancelled {
                    log::debug!("Turn was cancelled, skipping cleanup");
                    return;
                }

                _ = this.update(cx, |this, cx| this.flush_pending_message(cx));

                match turn_result {
                    Ok(()) => {
                        log::debug!("Turn execution completed");
                        event_stream.send_stop(acp::StopReason::EndTurn);
                    }
                    Err(error) => {
                        log::error!("Turn execution failed: {:?}", error);
                        match error.downcast::<CompletionError>() {
                            Ok(CompletionError::Refusal) => {
                                event_stream.send_stop(acp::StopReason::Refusal);
                                _ = this.update(cx, |this, _| this.messages.truncate(message_ix));
                            }
                            Ok(CompletionError::MaxTokens) => {
                                event_stream.send_stop(acp::StopReason::MaxTokens);
                            }
                            Ok(CompletionError::Other(error)) | Err(error) => {
                                event_stream.send_error(error);
                            }
                        }
                    }
                }

                _ = this.update(cx, |this, _| this.running_turn.take());
            }
        });
        self.running_turn = Some(RunningTurn::new(event_stream, tools, cancellation_tx, task));
        Ok(events_rx)
    }

    async fn run_turn_internal(
        this: &WeakEntity<Self>,
        event_stream: &ThreadEventStream,
        mut cancellation_rx: watch::Receiver<bool>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let mut attempt = 0;
        let mut intent = CompletionIntent::UserPrompt;
        // Set when a refusal fallback occurs so subsequent iterations use the fallback model.
        let mut refusal_fallback_model: Option<Arc<dyn LanguageModel>> = None;
        loop {
            match Self::perform_compaction_if_needed(
                this,
                event_stream,
                cancellation_rx.clone(),
                cx,
            )
            .await
            {
                // On success the telemetry event is deferred until the
                // completion below reports usage, so we can record an
                // accurate post-compaction context size (see
                // `handle_completion_event`).
                Ok(ControlFlow::Continue(())) => {}
                Ok(ControlFlow::Break(())) => {
                    this.update(cx, |this, _| {
                        this.emit_compaction_telemetry_outcome("canceled", None)
                    })?;
                    return Ok(());
                }
                Err(error) => {
                    log::error!("Compaction failed: {}", error);
                    let error_message = error.to_string();
                    match error.downcast::<LanguageModelCompletionError>() {
                        Ok(error) => {
                            attempt += 1;
                            match Self::retry_completion_error(
                                this,
                                event_stream,
                                &mut cancellation_rx,
                                error,
                                attempt,
                                cx,
                            )
                            .await
                            {
                                Ok(ControlFlow::Break(())) => {
                                    this.update(cx, |this, _| {
                                        this.emit_compaction_telemetry_outcome("canceled", None)
                                    })?;
                                    return Ok(());
                                }
                                Ok(ControlFlow::Continue(())) => {
                                    this.update(cx, |this, _| {
                                        if let Some(telemetry) =
                                            this.pending_compaction_telemetry.as_mut()
                                        {
                                            telemetry.retries += 1;
                                        }
                                    })?;
                                    continue;
                                }
                                Err(retry_error) => {
                                    this.update(cx, |this, _| {
                                        this.emit_compaction_telemetry_outcome(
                                            "failed",
                                            Some(error_message),
                                        )
                                    })?;
                                    return Err(retry_error);
                                }
                            }
                        }
                        Err(error) => {
                            this.update(cx, |this, _| {
                                this.emit_compaction_telemetry_outcome(
                                    "failed",
                                    Some(error_message),
                                )
                            })?;
                            return Err(error);
                        }
                    }
                }
            }

            // Re-read the model and refresh tools on each iteration so that
            // mid-turn changes (e.g. the user switches model, toggles tools,
            // or changes profile) take effect between tool-call rounds.
            // If a refusal fallback is active, use that model instead.
            let (model, request) = this.update(cx, |this, cx| {
                let model = refusal_fallback_model
                    .clone()
                    .or_else(|| this.model().cloned())
                    .ok_or_else(|| anyhow!(NoModelConfiguredError))?;
                this.refresh_turn_tools(cx);
                let request = this.build_completion_request(intent, cx)?;
                this.current_request_token_usage = TokenUsage::default();
                anyhow::Ok((model, request))
            })??;

            telemetry::event!(
                "Agent Thread Completion",
                thread_id = this.read_with(cx, |this, _| this.id.to_string())?,
                parent_thread_id = this.read_with(cx, |this, _| this
                    .parent_thread_id()
                    .map(|id| id.to_string()))?,
                prompt_id = this.read_with(cx, |this, _| this.prompt_id.to_string())?,
                model = model.telemetry_id(),
                model_provider = model.provider_id().to_string(),
                attempt
            );

            log::debug!("Calling model.stream_completion, attempt {}", attempt);

            let (mut events, mut error) = match model.stream_completion(request, cx).await {
                Ok(events) => (events.fuse(), None),
                Err(err) => (stream::empty().boxed().fuse(), Some(err)),
            };
            let mut tool_results: FuturesUnordered<Task<LanguageModelToolResult>> =
                FuturesUnordered::new();
            let mut early_tool_results: Vec<LanguageModelToolResult> = Vec::new();
            let mut cancelled = false;
            let mut had_refusal = false;
            loop {
                // Race between getting the first event, tool completion, and cancellation.
                let first_event = futures::select! {
                    event = events.next().fuse() => event,
                    tool_result = futures::StreamExt::select_next_some(&mut tool_results) => {
                        let is_error = tool_result.is_error;
                        let is_still_streaming = this
                            .read_with(cx, |this, _cx| {
                                this.running_turn
                                    .as_ref()
                                    .and_then(|turn| turn.streaming_tool_inputs.get(&tool_result.tool_use_id))
                                    .map_or(false, |inputs| !inputs.has_received_final())
                            })
                            .unwrap_or(false);

                        early_tool_results.push(tool_result);

                        // Only break if the tool errored and we are still
                        // streaming the input of the tool. If the tool errored
                        // but we are no longer streaming its input (i.e. there
                        // are parallel tool calls) we want to continue
                        // processing those tool inputs.
                        if is_error && is_still_streaming {
                            break;
                        }
                        continue;
                    }
                    _ = cancellation_rx.changed().fuse() => {
                        if *cancellation_rx.borrow() {
                            cancelled = true;
                            break;
                        }
                        continue;
                    }
                };
                let Some(first_event) = first_event else {
                    break;
                };

                // Collect all immediately available events to process as a batch
                let mut batch = vec![first_event];
                while let Some(event) = events.next().now_or_never().flatten() {
                    batch.push(event);
                }

                // Process the batch in a single update
                let batch_result = this.update(cx, |this, cx| {
                    let mut batch_tool_results = Vec::new();
                    let mut batch_error = None;

                    for event in batch {
                        log::trace!("Received completion event: {:?}", event);
                        match event {
                            Ok(event) => {
                                match this.handle_completion_event(
                                    event,
                                    event_stream,
                                    cancellation_rx.clone(),
                                    cx,
                                ) {
                                    Ok(Some(task)) => batch_tool_results.push(task),
                                    Ok(None) => {}
                                    Err(err) => {
                                        batch_error = Some(err);
                                        break;
                                    }
                                }
                            }
                            Err(err) => {
                                batch_error = Some(err.into());
                                break;
                            }
                        }
                    }

                    cx.notify();
                    (batch_tool_results, batch_error)
                })?;

                tool_results.extend(batch_result.0);
                if let Some(err) = batch_result.1 {
                    let is_refusal = err
                        .downcast_ref::<CompletionError>()
                        .is_some_and(|e| matches!(e, CompletionError::Refusal));
                    if is_refusal {
                        log::info!("Model refused request; checking for fallback model");
                        had_refusal = true;
                        break;
                    }
                    error = Some(err.downcast()?);
                    break;
                }
            }

            // Drop the stream to release the rate limit permit before tool execution.
            // The stream holds a semaphore guard that limits concurrent requests.
            // Without this, the permit would be held during potentially long-running
            // tool execution, which could cause deadlocks when tools spawn subagents
            // that need their own permits.
            drop(events);

            // Drop streaming tool input senders that never received their final input.
            // This prevents deadlock when the LLM stream ends (e.g. because of an error)
            // before sending a tool use with `is_input_complete: true`.
            this.update(cx, |this, _cx| {
                if let Some(running_turn) = this.running_turn.as_mut() {
                    if running_turn.streaming_tool_inputs.is_empty() {
                        return;
                    }
                    log::warn!("Dropping partial tool inputs because the stream ended");
                    running_turn.streaming_tool_inputs.drain();
                }
            })?;

            if had_refusal {
                let maybe_fallback = this.update(cx, |this, cx| -> Option<Arc<dyn LanguageModel>> {
                    let current_model = refusal_fallback_model.as_ref().or(this.model())?;
                    let fallback_id = match current_model.refusal_fallback_model_id() {
                        Some(id) => id,
                        None => {
                            log::info!(
                                "Refusal fallback: no fallback configured for model {} (provider {})",
                                current_model.id().0,
                                current_model.provider_id()
                            );
                            return None;
                        }
                    };
                    let provider_id = current_model.provider_id();
                    let found = LanguageModelRegistry::global(cx)
                        .read(cx)
                        .available_models(cx)
                        .find(|m| {
                            m.provider_id() == provider_id && m.id().0.as_ref() == fallback_id
                        });
                    if found.is_none() {
                        log::info!(
                            "Refusal fallback: fallback model {}/{} not found in available models",
                            provider_id,
                            fallback_id
                        );
                    }
                    found
                })?;

                if let Some(fallback) = maybe_fallback {
                    log::info!("Refusal fallback: retrying with {}", fallback.id().0);
                    let fallback_name = fallback.name().0.clone();
                    this.update(cx, |this, cx| {
                        this.pending_message = None;
                        this.set_model(fallback.clone(), cx);
                    })?;
                    event_stream.send_retry(acp_thread::RetryStatus {
                        last_error: "Safety filter triggered".into(),
                        attempt: 1,
                        max_attempts: 1,
                        started_at: Instant::now(),
                        duration: Duration::MAX,
                        meta: Some(acp_thread::meta_with_refusal_fallback(&fallback_name)),
                    });
                    refusal_fallback_model = Some(fallback);
                    continue;
                }
                log::info!("Request refused with no fallback model available");
                return Err(CompletionError::Refusal.into());
            }

            let end_turn = tool_results.is_empty() && early_tool_results.is_empty();

            for tool_result in early_tool_results {
                Self::process_tool_result(this, event_stream, cx, tool_result)?;
            }
            while let Some(tool_result) = tool_results.next().await {
                Self::process_tool_result(this, event_stream, cx, tool_result)?;
            }

            this.update(cx, |this, cx| {
                this.flush_pending_message(cx);
                if this.title.is_none() {
                    this.generate_title(cx);
                }
            })?;

            if cancelled {
                log::debug!("Turn cancelled by user, exiting");
                return Ok(());
            }

            if let Some(error) = error {
                attempt += 1;
                match Self::retry_completion_error(
                    this,
                    event_stream,
                    &mut cancellation_rx,
                    error,
                    attempt,
                    cx,
                )
                .await?
                {
                    ControlFlow::Break(_) => return Ok(()),
                    ControlFlow::Continue(_) => {}
                }
                this.update(cx, |this, _cx| {
                    if let Some(Message::Agent(message)) = this.last_message() {
                        if message.tool_results.is_empty() {
                            intent = CompletionIntent::UserPrompt;
                            this.messages.push(Arc::new(Message::Resume));
                        }
                    }
                })?;
            } else if end_turn {
                return Ok(());
            } else {
                let end_at_boundary =
                    this.update(cx, |this, _| this.end_turn_at_next_boundary())?;
                if end_at_boundary {
                    log::debug!("Steering message queued, ending turn at message boundary");
                    return Ok(());
                }
                intent = CompletionIntent::ToolResults;
                attempt = 0;
            }
        }
    }
}
