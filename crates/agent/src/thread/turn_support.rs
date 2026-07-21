use super::*;

impl Thread {
    /// Computes the retry status for a failed completion, notifies listeners,
    /// and waits out the backoff delay (or returns early if the turn is
    /// cancelled while waiting). Returns an error if the completion is not
    /// retryable or retries are exhausted.
    pub(super) async fn retry_completion_error(
        this: &WeakEntity<Self>,
        event_stream: &ThreadEventStream,
        cancellation_rx: &mut watch::Receiver<bool>,
        error: LanguageModelCompletionError,
        attempt: u8,
        cx: &mut AsyncApp,
    ) -> Result<ControlFlow<()>> {
        let retry = this.update(cx, |this, cx| {
            let user_store = this.user_store.read(cx);
            this.handle_completion_error(error, attempt, user_store.plan())
        })??;
        let timer = cx.background_executor().timer(retry.duration);
        event_stream.send_retry(retry);
        futures::select! {
            _ = timer.fuse() => {}
            _ = cancellation_rx.changed().fuse() => {
                if *cancellation_rx.borrow() {
                    log::debug!("Turn cancelled during retry delay, exiting");
                    return Ok(ControlFlow::Break(()));
                }
            }
        }
        Ok(ControlFlow::Continue(()))
    }

    pub(super) async fn perform_compaction_if_needed(
        this: &WeakEntity<Self>,
        event_stream: &ThreadEventStream,
        cancellation_rx: watch::Receiver<bool>,
        cx: &mut AsyncApp,
    ) -> Result<ControlFlow<()>> {
        let Some((model, request, insertion_ix)) = this.update(cx, |this, cx| {
            let insertion_ix = this.compaction_message_target_ix(cx)?;
            let model = this.model().cloned()?;
            let request = this.build_compaction_request(insertion_ix, &model, cx);
            this.current_request_token_usage = TokenUsage::default();
            // Preserve telemetry across retries so the retry count keeps
            // accumulating rather than resetting on each attempt.
            if this.pending_compaction_telemetry.is_none() {
                this.pending_compaction_telemetry = this.build_compaction_telemetry("auto", cx);
            }
            Some((model, request, insertion_ix))
        })?
        else {
            return Ok(ControlFlow::Continue(()));
        };

        Self::stream_compaction(
            this,
            event_stream,
            cancellation_rx,
            model,
            request,
            CompactionInsertion::Auto { insertion_ix },
            cx,
        )
        .await
    }

    pub(super) async fn stream_compaction(
        this: &WeakEntity<Self>,
        event_stream: &ThreadEventStream,
        mut cancellation_rx: watch::Receiver<bool>,
        model: Arc<dyn LanguageModel>,
        request: LanguageModelRequest,
        insertion: CompactionInsertion,
        cx: &mut AsyncApp,
    ) -> Result<ControlFlow<()>> {
        log::debug!("Running compaction");
        let compaction_id = acp_thread::ContextCompactionId(Uuid::new_v4().to_string().into());
        event_stream.send_context_compaction(
            compaction_id.clone(),
            acp_thread::ContextCompactionStatus::InProgress,
        );
        let stream = futures::select! {
            result = model.stream_completion(request, cx).fuse() => result,
            _ = cancellation_rx.changed().fuse() => {
                if *cancellation_rx.borrow() {
                    log::debug!("Compaction cancelled before request started");
                    return Ok(ControlFlow::Break(()));
                }
                return Ok(ControlFlow::Continue(()));
            }
        };
        let mut stream = stream?;

        let mut summary = String::new();
        loop {
            let event = futures::select! {
                event = stream.next().fuse() => event,
                _ = cancellation_rx.changed().fuse() => {
                    if *cancellation_rx.borrow() {
                        log::debug!("Compaction cancelled while summarizing");
                        return Ok(ControlFlow::Break(()));
                    }
                    continue;
                }
            };

            let Some(event) = event else {
                break;
            };

            match event? {
                LanguageModelCompletionEvent::Text(text) => {
                    summary.push_str(&text);
                    event_stream.send_context_compaction_update(compaction_id.clone(), &text);
                }
                LanguageModelCompletionEvent::UsageUpdate(usage) => {
                    this.update(cx, |this, _cx| {
                        this.accumulate_token_usage(usage);
                    })?;
                }
                LanguageModelCompletionEvent::Stop(_)
                | LanguageModelCompletionEvent::Started
                | LanguageModelCompletionEvent::Queued { .. }
                | LanguageModelCompletionEvent::Thinking { .. }
                | LanguageModelCompletionEvent::RedactedThinking { .. }
                | LanguageModelCompletionEvent::ReasoningDetails(_)
                | LanguageModelCompletionEvent::ToolUse(_)
                | LanguageModelCompletionEvent::ToolUseJsonParseError { .. }
                | LanguageModelCompletionEvent::StartMessage { .. }
                | LanguageModelCompletionEvent::Compaction(_) => {}
            }
        }

        if *cancellation_rx.borrow() {
            log::debug!("Compaction cancelled after summarizing");
            return Ok(ControlFlow::Break(()));
        }

        let summary = summary.trim().to_string();
        if summary.is_empty() {
            log::warn!("Compaction produced an empty summary");
            return Err(anyhow::anyhow!("Compaction produced an empty summary"));
        }

        log::debug!("Compaction succeeded:\n{summary}");
        event_stream.update_context_compaction_status(
            compaction_id,
            acp_thread::ContextCompactionStatus::Completed,
        );

        this.update(cx, |this, cx| {
            let compaction = Arc::new(Message::Compaction(CompactionInfo::Summary(summary.into())));
            match insertion {
                CompactionInsertion::Auto { insertion_ix } => {
                    if insertion_ix <= this.messages.len() {
                        this.messages.insert(insertion_ix, compaction);
                    } else {
                        this.messages.push(compaction);
                    }
                }
                CompactionInsertion::Manual { marker_id } => {
                    this.messages.push(Arc::new(Message::User(UserMessage {
                        id: marker_id,
                        content: Arc::from([]),
                    })));
                    this.messages.push(compaction);
                }
            }
            cx.notify();
        })?;

        Ok(ControlFlow::Continue(()))
    }

    pub(super) fn process_tool_result(
        this: &WeakEntity<Thread>,
        event_stream: &ThreadEventStream,
        cx: &mut AsyncApp,
        tool_result: LanguageModelToolResult,
    ) -> Result<(), anyhow::Error> {
        log::debug!("Tool finished {:?}", tool_result);

        event_stream.update_tool_call_fields(
            &tool_result.tool_use_id,
            acp::ToolCallUpdateFields::new()
                .status(if tool_result.is_error {
                    acp::ToolCallStatus::Failed
                } else {
                    acp::ToolCallStatus::Completed
                })
                .raw_output(tool_result.output.clone()),
            None,
        );
        this.update(cx, |this, _cx| {
            this.pending_message()
                .tool_results
                .insert(tool_result.tool_use_id.clone(), tool_result)
        })?;
        Ok(())
    }

    fn handle_completion_error(
        &mut self,
        error: LanguageModelCompletionError,
        attempt: u8,
        plan: Option<Plan>,
    ) -> Result<acp_thread::RetryStatus> {
        let Some(model) = self.model() else {
            return Err(anyhow!(error));
        };

        let auto_retry = if model.provider_id() == MAV_CLOUD_PROVIDER_ID {
            plan.is_some()
        } else {
            true
        };

        if !auto_retry {
            return Err(anyhow!(error));
        }

        let Some(strategy) = Self::retry_strategy_for(&error) else {
            return Err(anyhow!(error));
        };

        let max_attempts = match &strategy {
            RetryStrategy::ExponentialBackoff { max_attempts, .. } => *max_attempts,
            RetryStrategy::Fixed { max_attempts, .. } => *max_attempts,
        };

        if attempt > max_attempts {
            return Err(anyhow!(error));
        }

        let delay = match &strategy {
            RetryStrategy::ExponentialBackoff { initial_delay, .. } => {
                let delay_secs = initial_delay.as_secs() * 2u64.pow((attempt - 1) as u32);
                Duration::from_secs(delay_secs)
            }
            RetryStrategy::Fixed { delay, .. } => *delay,
        };
        log::debug!("Retry attempt {attempt} with delay {delay:?}");

        Ok(acp_thread::RetryStatus {
            last_error: error.to_string().into(),
            attempt: attempt as usize,
            max_attempts: max_attempts as usize,
            started_at: Instant::now(),
            duration: delay,
            meta: None,
        })
    }

    fn retry_strategy_for(error: &LanguageModelCompletionError) -> Option<RetryStrategy> {
        use LanguageModelCompletionError::*;
        use http_client::StatusCode;

        // General strategy here:
        // - If retrying won't help (e.g. invalid API key or payload too large), return None so we don't retry at all.
        // - If it's a time-based issue (e.g. server overloaded, rate limit exceeded), retry up to 4 times with exponential backoff.
        // - If it's an issue that *might* be fixed by retrying (e.g. internal server error), retry up to 3 times.
        match error {
            HttpResponseError {
                status_code: StatusCode::TOO_MANY_REQUESTS,
                ..
            } => Some(RetryStrategy::ExponentialBackoff {
                initial_delay: BASE_RETRY_DELAY,
                max_attempts: MAX_RETRY_ATTEMPTS,
            }),
            ServerOverloaded { retry_after, .. } | RateLimitExceeded { retry_after, .. } => {
                Some(RetryStrategy::Fixed {
                    delay: retry_after.unwrap_or(BASE_RETRY_DELAY),
                    max_attempts: MAX_RETRY_ATTEMPTS,
                })
            }
            UpstreamProviderError {
                status,
                retry_after,
                ..
            } => match *status {
                StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => {
                    Some(RetryStrategy::Fixed {
                        delay: retry_after.unwrap_or(BASE_RETRY_DELAY),
                        max_attempts: MAX_RETRY_ATTEMPTS,
                    })
                }
                StatusCode::INTERNAL_SERVER_ERROR => Some(RetryStrategy::Fixed {
                    delay: retry_after.unwrap_or(BASE_RETRY_DELAY),
                    // Internal Server Error could be anything, retry up to 3 times.
                    max_attempts: 3,
                }),
                status => {
                    // There is no StatusCode variant for the unofficial HTTP 529 ("The service is overloaded"),
                    // but we frequently get them in practice. See https://http.dev/529
                    if status.as_u16() == 529 {
                        Some(RetryStrategy::Fixed {
                            delay: retry_after.unwrap_or(BASE_RETRY_DELAY),
                            max_attempts: MAX_RETRY_ATTEMPTS,
                        })
                    } else {
                        Some(RetryStrategy::Fixed {
                            delay: retry_after.unwrap_or(BASE_RETRY_DELAY),
                            max_attempts: 2,
                        })
                    }
                }
            },
            ApiInternalServerError { .. } => Some(RetryStrategy::Fixed {
                delay: BASE_RETRY_DELAY,
                max_attempts: 3,
            }),
            ApiReadResponseError { .. }
            | HttpSend { .. }
            | DeserializeResponse { .. }
            | BadRequestFormat { .. } => Some(RetryStrategy::Fixed {
                delay: BASE_RETRY_DELAY,
                max_attempts: 3,
            }),
            // Retrying these errors definitely shouldn't help.
            HttpResponseError {
                status_code:
                    StatusCode::PAYLOAD_TOO_LARGE | StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED,
                ..
            }
            | AuthenticationError { .. }
            | PermissionError { .. }
            | NoApiKey { .. }
            | ApiEndpointNotFound { .. }
            | PromptTooLarge { .. } => None,
            // These errors might be transient, so retry them
            SerializeRequest { .. } | BuildRequestBody { .. } | StreamEndedUnexpectedly { .. } => {
                Some(RetryStrategy::Fixed {
                    delay: BASE_RETRY_DELAY,
                    max_attempts: 1,
                })
            }
            // Retry all other 4xx and 5xx errors once.
            HttpResponseError { status_code, .. }
                if status_code.is_client_error() || status_code.is_server_error() =>
            {
                Some(RetryStrategy::Fixed {
                    delay: BASE_RETRY_DELAY,
                    max_attempts: 3,
                })
            }
            // Retrying won't help for Payment Required errors.
            PaymentRequired => None,
            // Retrying won't help until the user consents to data retention
            // or switches models.
            DataRetentionConsentRequired { .. } => None,
            // Conservatively assume that any other errors are non-retryable
            HttpResponseError { .. } | Other(..) => Some(RetryStrategy::Fixed {
                delay: BASE_RETRY_DELAY,
                max_attempts: 2,
            }),
        }
    }
}
