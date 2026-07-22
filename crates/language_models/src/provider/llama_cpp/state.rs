use super::*;

impl State {
    pub(super) fn is_authenticated(&self) -> bool {
        !self.fetched_models.is_empty()
    }

    pub(super) fn set_api_key(
        &mut self,
        api_key: Option<String>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = LlamaCppLanguageModelProvider::api_url(cx);
        let task = self.api_key_state.store(
            api_url,
            api_key,
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        );

        self.fetched_models.clear();
        // Drop the event stream so it reconnects with the new key (re-fetch
        // below restarts it).
        self.model_event_task = None;
        write_recover(&self.loading_progress).clear();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| this.restart_fetch_models_task(cx))
                .ok();
            result
        })
    }

    pub(super) fn authenticate(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), AuthenticateError>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = LlamaCppLanguageModelProvider::api_url(cx);
        let load_key_task = self.api_key_state.load_if_needed(
            api_url,
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        );

        if self.is_authenticated() {
            return Task::ready(Ok(()));
        }

        cx.spawn(async move |this, cx| {
            match load_key_task.await {
                Ok(()) | Err(AuthenticateError::CredentialsNotFound) => {}
                Err(error) => {
                    log::warn!("failed to load llama.cpp API key: {error}");
                }
            }
            let fetch_models_task = this.update(cx, |this, cx| this.fetch_models(cx))?;
            match fetch_models_task.await {
                Ok(()) => Ok(()),
                Err(err) => {
                    // A refused connection means the server isn't running yet, not an error.
                    let connection_refused = err.chain().any(|cause| {
                        cause
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|io_err| {
                                io_err.kind() == std::io::ErrorKind::ConnectionRefused
                            })
                    });
                    if connection_refused {
                        Err(AuthenticateError::ConnectionRefused)
                    } else {
                        Err(AuthenticateError::Other(err))
                    }
                }
            }
        })
    }

    fn fetch_models(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        let http_client = Arc::clone(&self.http_client);
        let settings = LlamaCppLanguageModelProvider::settings(cx);
        let api_url = LlamaCppLanguageModelProvider::api_url(cx);
        let api_key = self.api_key_state.key(&api_url);
        let extra_headers = settings.custom_headers.clone();

        cx.spawn(async move |this, cx| {
            let entries = get_models(
                http_client.as_ref(),
                &api_url,
                api_key.as_deref(),
                &extra_headers,
            )
            .await?;

            let is_router = entries.iter().any(ModelEntry::is_router_entry);

            // Models the server reports as loading, used below to prune stale
            // progress labels by reconciling against the live listing (a preempted
            // load or missed SSE event can skip the terminal event).
            let loading_ids: HashSet<String> = entries
                .iter()
                .filter(|entry| entry.is_loading())
                .map(|entry| entry.id.clone())
                .collect();

            let models: Vec<llama_cpp::Model> = if is_router {
                // Router mode: metadata comes from `/v1/models`. We probe
                // `/props` only for loaded models so listing never triggers a
                // load; unloaded models use the listing's hints and overrides.
                let tasks = entries.into_iter().map(|entry| {
                    let http_client = Arc::clone(&http_client);
                    let api_url = api_url.clone();
                    let api_key = api_key.clone();
                    let extra_headers = extra_headers.clone();
                    async move {
                        let props = if entry.is_loaded() {
                            get_props(
                                http_client.as_ref(),
                                &api_url,
                                api_key.as_deref(),
                                Some(&entry.id),
                                &extra_headers,
                            )
                            .await
                            .log_err()
                        } else {
                            None
                        };
                        model_from_entry(&entry, props.as_ref())
                    }
                });
                futures::stream::iter(tasks)
                    .buffer_unordered(5)
                    .collect()
                    .await
            } else {
                // Single-model mode: one `/props` call describes the loaded model.
                let props = get_props(
                    http_client.as_ref(),
                    &api_url,
                    api_key.as_deref(),
                    None,
                    &extra_headers,
                )
                .await
                .log_err();
                entries
                    .iter()
                    .map(|entry| model_from_entry(entry, props.as_ref()))
                    .collect()
            };

            this.update(cx, |this, cx| {
                this.fetched_models = models;
                let effective = compute_effective_models(
                    &this.fetched_models,
                    LlamaCppLanguageModelProvider::settings(cx),
                );
                sync_capability_cells(&this.capability_cells, &effective);
                // Drop progress labels for models no longer loading, so a stale
                // "Loading …" can't stick after a preempted load or missed event.
                write_recover(&this.loading_progress).retain(|id, _| loading_ids.contains(id));
                // Router mode loads models on demand: subscribe so capabilities
                // self-correct as they load/unload. Start it once (events trigger
                // re-discovery, not a re-spawn); single-model mode needs no stream.
                if is_router {
                    if this.model_event_task.is_none() {
                        this.start_model_event_stream(cx);
                    }
                } else {
                    this.model_event_task = None;
                }
                cx.notify();
            })
        })
    }

    /// Subscribes to `/models/sse` and re-runs discovery as models load, unload,
    /// or the list changes, so capabilities stay current. Reconnects if the stream
    /// drops; on builds without `/models/sse` the refresh is simply skipped.
    fn start_model_event_stream(&mut self, cx: &mut Context<Self>) {
        let http_client = Arc::clone(&self.http_client);
        let api_url = LlamaCppLanguageModelProvider::api_url(cx);
        let api_key = self.api_key_state.key(&api_url);
        let extra_headers = LlamaCppLanguageModelProvider::settings(cx)
            .custom_headers
            .clone();

        self.model_event_task = Some(cx.spawn(async move |this, cx| {
            loop {
                match stream_model_events(
                    http_client.as_ref(),
                    &api_url,
                    api_key.as_deref(),
                    &extra_headers,
                )
                .await
                {
                    Ok(mut events) => {
                        while let Some(event) = events.next().await {
                            let Some(event) = event.log_err() else {
                                continue;
                            };
                            if let Some(exit_code) = event.load_failure() {
                                log::error!(
                                    "llama.cpp model {} failed to load (exit code {exit_code})",
                                    event.model
                                );
                            }
                            // Loading-progress tick: record it for the selector (no
                            // re-discovery). `cx.notify()` drives `ProviderStateChanged`.
                            if let Some(progress) = event.load_progress() {
                                let label = SharedString::from(progress.progress_label());
                                if this
                                    .update(cx, |this, cx| {
                                        write_recover(&this.loading_progress)
                                            .insert(event.model.clone(), label);
                                        cx.notify();
                                    })
                                    .is_err()
                                {
                                    return;
                                }
                                continue;
                            }
                            if !event.changes_model_state() {
                                continue;
                            }
                            // Terminal load/unload (or list change): drop the
                            // progress label and re-discover to refresh capabilities.
                            if this
                                .update(cx, |this, cx| {
                                    write_recover(&this.loading_progress).remove(&event.model);
                                    this.restart_fetch_models_task(cx);
                                })
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                    // Endpoint missing (older build) or connection failed; retry after a backoff.
                    Err(error) => {
                        log::warn!("llama.cpp model event stream unavailable: {error:#}");
                    }
                }

                cx.background_executor()
                    .timer(MODEL_EVENT_RECONNECT_INTERVAL)
                    .await;
                if this.update(cx, |_, _| ()).is_err() {
                    return;
                }
            }
        }));
    }

    pub(super) fn restart_fetch_models_task(&mut self, cx: &mut Context<Self>) {
        let task = self.fetch_models(cx);
        self.fetch_model_task.replace(task);
    }
}
