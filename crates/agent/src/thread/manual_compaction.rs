use super::*;

impl Thread {
    /// Force a manual context compaction using the summary strategy,
    /// regardless of the current token usage or context window size.
    pub fn compact(
        &mut self,
        id: ClientUserMessageId,
        cx: &mut Context<Self>,
    ) -> Result<mpsc::UnboundedReceiver<Result<ThreadEvent>>> {
        let model = self
            .model()
            .cloned()
            .ok_or_else(|| anyhow!(NoModelConfiguredError))?;

        // Flush any pending message and cancel an in-flight turn before we
        // start, mirroring `run_turn` so a stray completion can't race with the
        // compaction we're about to perform.
        self.flush_pending_message(cx);
        self.cancel(cx).detach();

        let compaction = self.forced_compaction_target_ix().map(|request_end_ix| {
            self.advance_prompt_id();
            let request = self.build_compaction_request(request_end_ix, &model, cx);
            self.current_request_token_usage = TokenUsage::default();
            (model, request)
        });

        if compaction.is_some() {
            self.pending_compaction_telemetry = self.build_compaction_telemetry("manual", cx);
        }

        self.clear_summary();
        cx.notify();

        let (events_tx, events_rx) = mpsc::unbounded::<Result<ThreadEvent>>();
        let event_stream = ThreadEventStream(events_tx);
        let (cancellation_tx, mut cancellation_rx) = watch::channel(false);
        let task = cx.spawn({
            let event_stream = event_stream.clone();
            async move |this, cx| {
                let result = if let Some((model, request)) = compaction {
                    Self::stream_compaction(
                        &this,
                        &event_stream,
                        cancellation_rx.clone(),
                        model,
                        request,
                        CompactionInsertion::Manual { marker_id: id },
                        cx,
                    )
                    .await
                } else {
                    Ok(ControlFlow::Continue(()))
                };

                // If we were cancelled, `cancel()` already took `running_turn`
                // (possibly for a new turn), so leave it alone.
                if *cancellation_rx.borrow() {
                    this.update(cx, |this, _| {
                        this.emit_compaction_telemetry_outcome("canceled", None)
                    })
                    .log_err();
                    return;
                }

                match result {
                    // On success, the telemetry event is deferred until the next
                    // completion reports usage (see `handle_completion_event`),
                    // so we leave `pending_compaction_telemetry` in place here.
                    Ok(_) => event_stream.send_stop(acp::StopReason::EndTurn),
                    Err(error) => {
                        log::error!("Manual compaction failed: {:?}", error);
                        this.update(cx, |this, _| {
                            this.emit_compaction_telemetry_outcome(
                                "failed",
                                Some(error.to_string()),
                            )
                        })
                        .log_err();
                        event_stream.send_error(error);
                    }
                }

                _ = this.update(cx, |this, _| this.running_turn.take());
            }
        });
        self.running_turn = Some(RunningTurn::new(
            event_stream,
            BTreeMap::default(),
            cancellation_tx,
            task,
        ));

        Ok(events_rx)
    }
}
