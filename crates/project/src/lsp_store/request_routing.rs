use super::*;

impl LspStore {
    pub(super) fn is_capable_for_proto_request<R>(
        &self,
        buffer: &Entity<Buffer>,
        request: &R,
        cx: &App,
    ) -> bool
    where
        R: LspCommand,
    {
        self.check_if_capable_for_proto_request(
            buffer,
            |capabilities| {
                request.check_capabilities(AdapterServerCapabilities {
                    server_capabilities: capabilities.clone(),
                    code_action_kinds: None,
                })
            },
            cx,
        )
    }

    pub(super) fn relevant_server_ids_for_capability_check(
        &self,
        buffer: &Entity<Buffer>,
        cx: &App,
    ) -> Vec<LanguageServerId> {
        let buffer_id = buffer.read(cx).remote_id();
        if let Some(local) = self.as_local() {
            return local
                .buffers_opened_in_servers
                .get(&buffer_id)
                .into_iter()
                .flatten()
                .copied()
                .collect();
        }

        let Some(language) = buffer.read(cx).language().cloned() else {
            return Vec::default();
        };
        let registered_language_servers = self
            .languages
            .lsp_adapters(&language.name())
            .into_iter()
            .map(|lsp_adapter| lsp_adapter.name())
            .collect::<HashSet<_>>();
        self.language_server_statuses
            .iter()
            .filter_map(|(server_id, server_status)| {
                registered_language_servers
                    .contains(&server_status.name)
                    .then_some(*server_id)
            })
            .collect()
    }

    pub(super) fn check_if_any_relevant_server_matches<F>(
        &self,
        buffer: &Entity<Buffer>,
        mut check: F,
        cx: &App,
    ) -> bool
    where
        F: FnMut(&LanguageServerStatus, &lsp::ServerCapabilities) -> bool,
    {
        self.relevant_server_ids_for_capability_check(buffer, cx)
            .into_iter()
            .filter_map(|server_id| {
                Some((
                    self.language_server_statuses.get(&server_id)?,
                    self.lsp_server_capabilities.get(&server_id)?,
                ))
            })
            .any(|(server_status, capabilities)| check(server_status, capabilities))
    }

    pub(super) fn check_if_capable_for_proto_request<F>(
        &self,
        buffer: &Entity<Buffer>,
        mut check: F,
        cx: &App,
    ) -> bool
    where
        F: FnMut(&lsp::ServerCapabilities) -> bool,
    {
        self.check_if_any_relevant_server_matches(buffer, |_, capabilities| check(capabilities), cx)
    }

    pub fn supports_range_formatting(&self, buffer: &Entity<Buffer>, cx: &App) -> bool {
        let settings = LanguageSettings::for_buffer(buffer.read(cx), cx);
        settings.formatter.as_ref().iter().any(|formatter| {
            match formatter {
                Formatter::None => false,
                Formatter::Auto => {
                    settings.prettier.allowed
                        || self.check_if_capable_for_proto_request(
                            buffer,
                            server_capabilities_support_range_formatting,
                            cx,
                        )
                }
                Formatter::Prettier => true,
                Formatter::External { .. } => false,
                Formatter::LanguageServer(settings::LanguageServerFormatterSpecifier::Current) => {
                    self.check_if_capable_for_proto_request(
                        buffer,
                        server_capabilities_support_range_formatting,
                        cx,
                    )
                }
                Formatter::LanguageServer(
                    settings::LanguageServerFormatterSpecifier::Specific { name },
                ) => self.check_if_any_relevant_server_matches(
                    buffer,
                    |server_status, capabilities| {
                        server_status.name.0.as_ref() == name
                            && server_capabilities_support_range_formatting(capabilities)
                    },
                    cx,
                ),
                // `FormatSelections` should only surface when a formatter can honor the
                // selected ranges. Code actions can still run as part of formatting, but
                // they operate on the whole buffer rather than the selected text.
                Formatter::CodeAction(_) => false,
            }
        })
    }

    pub(super) fn all_capable_for_proto_request<F>(
        &self,
        buffer: &Entity<Buffer>,
        mut check: F,
        cx: &App,
    ) -> Vec<(lsp::LanguageServerId, lsp::LanguageServerName)>
    where
        F: FnMut(&lsp::LanguageServerName, &lsp::ServerCapabilities) -> bool,
    {
        self.relevant_server_ids_for_capability_check(buffer, cx)
            .into_iter()
            .filter_map(|server_id| {
                Some((
                    server_id,
                    &self.language_server_statuses.get(&server_id)?.name,
                    self.lsp_server_capabilities.get(&server_id)?,
                ))
            })
            .filter(|(_, server_name, capabilities)| check(server_name, capabilities))
            .map(|(server_id, server_name, _)| (server_id, server_name.clone()))
            .collect()
    }

    pub fn request_lsp<R>(
        &mut self,
        buffer: Entity<Buffer>,
        server: LanguageServerToQuery,
        request: R,
        cx: &mut Context<Self>,
    ) -> Task<Result<R::Response>>
    where
        R: LspCommand,
        <R::LspRequest as lsp::request::Request>::Result: Send,
        <R::LspRequest as lsp::request::Request>::Params: Send,
    {
        if let Some((upstream_client, upstream_project_id)) = self.upstream_client() {
            return self.send_lsp_proto_request(
                buffer,
                upstream_client,
                upstream_project_id,
                request,
                cx,
            );
        }

        let Some(language_server) = buffer.update(cx, |buffer, cx| match server {
            LanguageServerToQuery::FirstCapable => self.as_local().and_then(|local| {
                local
                    .language_servers_for_buffer(buffer, cx)
                    .find(|(_, server)| {
                        request.check_capabilities(server.adapter_server_capabilities())
                    })
                    .map(|(_, server)| server.clone())
            }),
            LanguageServerToQuery::Other(id) => self
                .language_server_for_local_buffer(buffer, id, cx)
                .and_then(|(_, server)| {
                    request
                        .check_capabilities(server.adapter_server_capabilities())
                        .then(|| Arc::clone(server))
                }),
        }) else {
            return Task::ready(Ok(Default::default()));
        };

        let file = File::from_dyn(buffer.read(cx).file()).and_then(File::as_local);

        let Some(file) = file else {
            return Task::ready(Ok(Default::default()));
        };

        let lsp_params = match request.to_lsp_params_or_response(
            &file.abs_path(cx),
            buffer.read(cx),
            &language_server,
            cx,
        ) {
            Ok(LspParamsOrResponse::Params(lsp_params)) => lsp_params,
            Ok(LspParamsOrResponse::Response(response)) => return Task::ready(Ok(response)),
            Err(err) => {
                let message = format!(
                    "{} via {} failed: {}",
                    request.display_name(),
                    language_server.name(),
                    err
                );
                if should_log_lsp_request_failure(&message) {
                    log::warn!("{message}");
                }
                return Task::ready(Err(anyhow!(message)));
            }
        };

        let status = request.status();
        let request_timeout = ProjectSettings::get_global(cx)
            .global_lsp_settings
            .get_request_timeout();

        cx.spawn(async move |this, cx| {
            let lsp_request = language_server.request::<R::LspRequest>(lsp_params, request_timeout);

            let id = lsp_request.id();
            let _cleanup = if status.is_some() {
                cx.update(|cx| {
                    this.update(cx, |this, cx| {
                        this.on_lsp_work_start(
                            language_server.server_id(),
                            ProgressToken::Number(id),
                            LanguageServerProgress {
                                is_disk_based_diagnostics_progress: false,
                                is_cancellable: false,
                                title: None,
                                message: status.clone(),
                                percentage: None,
                                last_update_at: cx.background_executor().now(),
                            },
                            cx,
                        );
                    })
                })
                .log_err();

                Some(defer(|| {
                    cx.update(|cx| {
                        this.update(cx, |this, cx| {
                            this.on_lsp_work_end(
                                language_server.server_id(),
                                ProgressToken::Number(id),
                                cx,
                            );
                        })
                    })
                    .log_err();
                }))
            } else {
                None
            };

            let result = lsp_request.await.into_response();

            let response = result.map_err(|err| {
                let message = format!(
                    "{} via {} failed: {}",
                    request.display_name(),
                    language_server.name(),
                    err
                );
                if should_log_lsp_request_failure(&message) {
                    log::warn!("{message}");
                }
                anyhow::anyhow!(message)
            })?;

            request
                .response_from_lsp(
                    response,
                    this.upgrade().context("no app context")?,
                    buffer,
                    language_server.server_id(),
                    cx.clone(),
                )
                .await
        })
    }
    pub(super) fn local_lsp_servers_for_buffer(
        &self,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Vec<LanguageServerId> {
        let Some(local) = self.as_local() else {
            return Vec::new();
        };

        let snapshot = buffer.read(cx).snapshot();

        buffer.update(cx, |buffer, cx| {
            local
                .language_servers_for_buffer(buffer, cx)
                .map(|(_, server)| server.server_id())
                .filter(|server_id| {
                    self.as_local().is_none_or(|local| {
                        local
                            .buffers_opened_in_servers
                            .get(&snapshot.remote_id())
                            .is_some_and(|servers| servers.contains(server_id))
                    })
                })
                .collect()
        })
    }

    pub(super) fn request_multiple_lsp_locally<P, R>(
        &mut self,
        buffer: &Entity<Buffer>,
        position: Option<P>,
        request: R,
        cx: &mut Context<Self>,
    ) -> Task<Vec<(LanguageServerId, R::Response)>>
    where
        P: ToOffset,
        R: LspCommand + Clone,
        <R::LspRequest as lsp::request::Request>::Result: Send,
        <R::LspRequest as lsp::request::Request>::Params: Send,
    {
        let Some(local) = self.as_local() else {
            return Task::ready(Vec::new());
        };

        let snapshot = buffer.read(cx).snapshot();
        let scope = position.and_then(|position| snapshot.language_scope_at(position));

        let server_ids = buffer.update(cx, |buffer, cx| {
            local
                .language_servers_for_buffer(buffer, cx)
                .filter(|(adapter, _)| {
                    scope
                        .as_ref()
                        .map(|scope| scope.language_allowed(&adapter.name))
                        .unwrap_or(true)
                })
                .map(|(_, server)| server.server_id())
                .filter(|server_id| {
                    self.as_local().is_none_or(|local| {
                        local
                            .buffers_opened_in_servers
                            .get(&snapshot.remote_id())
                            .is_some_and(|servers| servers.contains(server_id))
                    })
                })
                .collect::<Vec<_>>()
        });

        let mut response_results = server_ids
            .into_iter()
            .map(|server_id| {
                let task = self.request_lsp(
                    buffer.clone(),
                    LanguageServerToQuery::Other(server_id),
                    request.clone(),
                    cx,
                );
                async move { (server_id, task.await) }
            })
            .collect::<FuturesUnordered<_>>();

        cx.background_spawn(async move {
            let mut responses = Vec::with_capacity(response_results.len());
            while let Some((server_id, response_result)) = response_results.next().await {
                match response_result {
                    Ok(response) => responses.push((server_id, response)),
                    // rust-analyzer likes to error with this when its still loading up
                    Err(e) if format!("{e:#}").ends_with("content modified") => (),
                    Err(e) => log::error!("Error handling response for request {request:?}: {e:#}"),
                }
            }
            responses
        })
    }
}
