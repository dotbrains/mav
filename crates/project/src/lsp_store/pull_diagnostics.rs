use super::*;

impl LspStore {
    pub fn pull_diagnostics(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LspPullDiagnostics>>>> {
        let buffer_id = buffer.read(cx).remote_id();

        if let Some((client, upstream_project_id)) = self.upstream_client() {
            let mut suitable_capabilities = None;
            // Are we capable for proto request?
            let any_server_has_diagnostics_provider = self.check_if_capable_for_proto_request(
                &buffer,
                |capabilities| {
                    if let Some(caps) = &capabilities.diagnostic_provider {
                        suitable_capabilities = Some(caps.clone());
                        true
                    } else {
                        false
                    }
                },
                cx,
            );
            // We don't really care which caps are passed into the request, as they're ignored by RPC anyways.
            let Some(dynamic_caps) = suitable_capabilities else {
                return Task::ready(Ok(None));
            };
            assert!(any_server_has_diagnostics_provider);

            let identifier = buffer_diagnostic_identifier(&dynamic_caps);
            let request = GetDocumentDiagnostics {
                previous_result_id: None,
                identifier,
                registration_id: None,
            };
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = client.request_lsp(
                upstream_project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(upstream_project_id, buffer.read(cx)),
            );
            cx.background_spawn(async move {
                // Proto requests cause the diagnostics to be pulled from language server(s) on the local side
                // and then, buffer state updated with the diagnostics received, which will be later propagated to the client.
                // Do not attempt to further process the dummy responses here.
                let _response = request_task.await?;
                Ok(None)
            })
        } else {
            let servers = buffer.update(cx, |buffer, cx| {
                self.running_language_servers_for_local_buffer(buffer, cx)
                    .map(|(_, server)| server.clone())
                    .collect::<Vec<_>>()
            });

            let pull_diagnostics = servers
                .into_iter()
                .flat_map(|server| {
                    let result = maybe!({
                        let local = self.as_local()?;
                        let server_id = server.server_id();
                        let providers_with_identifiers = local
                            .language_server_dynamic_registrations
                            .get(&server_id)
                            .into_iter()
                            .flat_map(|registrations| registrations.diagnostics.clone())
                            .collect::<Vec<_>>();
                        Some(
                            providers_with_identifiers
                                .into_iter()
                                .map(|(registration_id, dynamic_caps)| {
                                    let identifier = buffer_diagnostic_identifier(&dynamic_caps);
                                    let registration_id = registration_id.map(SharedString::from);
                                    let result_id = self.result_id_for_buffer_pull(
                                        server_id,
                                        buffer_id,
                                        &registration_id,
                                        cx,
                                    );
                                    self.request_lsp(
                                        buffer.clone(),
                                        LanguageServerToQuery::Other(server_id),
                                        GetDocumentDiagnostics {
                                            previous_result_id: result_id,
                                            registration_id,
                                            identifier,
                                        },
                                        cx,
                                    )
                                })
                                .collect::<Vec<_>>(),
                        )
                    });

                    result.unwrap_or_default()
                })
                .collect::<Vec<_>>();

            cx.background_spawn(async move {
                let mut responses = Vec::new();
                for diagnostics in join_all(pull_diagnostics).await {
                    responses.extend(diagnostics?);
                }
                Ok(Some(responses))
            })
        }
    }

    pub(super) fn diagnostic_registration_exists(
        &self,
        server_id: LanguageServerId,
        registration_id: &Option<SharedString>,
    ) -> bool {
        let Some(local) = self.as_local() else {
            return false;
        };
        let Some(registrations) = local.language_server_dynamic_registrations.get(&server_id)
        else {
            return false;
        };
        let registration_key = registration_id.as_ref().map(|s| s.to_string());
        registrations.diagnostics.contains_key(&registration_key)
    }

    pub fn pull_diagnostics_for_buffer(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let diagnostics = self.pull_diagnostics(buffer, cx);
        cx.spawn(async move |lsp_store, cx| {
            let diagnostics = match diagnostics.await {
                Ok(Some(diagnostics)) => diagnostics,
                Ok(None) => return Ok(()),
                Err(error) if should_log_lsp_request_failure(&format!("{error:#}")) => {
                    return Err(error).context("pulling diagnostics");
                }
                // This is a weird way to suppress diagnostic failures on server side cancellation,
                // we should actually retry the request here?
                Err(_) => return Ok(()),
            };
            lsp_store.update(cx, |lsp_store, cx| {
                if lsp_store.as_local().is_none() {
                    return;
                }

                let mut unchanged_buffers = HashMap::default();
                let server_diagnostics_updates = diagnostics
                    .into_iter()
                    .filter_map(|diagnostics_set| match diagnostics_set {
                        LspPullDiagnostics::Response {
                            server_id,
                            uri,
                            diagnostics,
                            registration_id,
                        } => Some((server_id, uri, diagnostics, registration_id)),
                        LspPullDiagnostics::Default => None,
                    })
                    .filter(|(server_id, _, _, registration_id)| {
                        lsp_store.diagnostic_registration_exists(*server_id, registration_id)
                    })
                    .fold(
                        HashMap::default(),
                        |mut acc, (server_id, uri, diagnostics, new_registration_id)| {
                            let (result_id, diagnostics) = match diagnostics {
                                PulledDiagnostics::Unchanged { result_id } => {
                                    unchanged_buffers
                                        .entry(new_registration_id.clone())
                                        .or_insert_with(HashSet::default)
                                        .insert(uri.clone());
                                    (Some(result_id), Vec::new())
                                }
                                PulledDiagnostics::Changed {
                                    result_id,
                                    diagnostics,
                                } => (result_id, diagnostics),
                            };
                            let disk_based_sources = Cow::Owned(
                                lsp_store
                                    .language_server_adapter_for_id(server_id)
                                    .as_ref()
                                    .map(|adapter| adapter.disk_based_diagnostic_sources.as_slice())
                                    .unwrap_or(&[])
                                    .to_vec(),
                            );
                            acc.entry(server_id)
                                .or_insert_with(HashMap::default)
                                .entry(new_registration_id.clone())
                                .or_insert_with(Vec::new)
                                .push(DocumentDiagnosticsUpdate {
                                    server_id,
                                    diagnostics: lsp::PublishDiagnosticsParams {
                                        uri,
                                        diagnostics,
                                        version: None,
                                    },
                                    result_id: result_id.map(SharedString::new),
                                    disk_based_sources,
                                    registration_id: new_registration_id,
                                });
                            acc
                        },
                    );

                for diagnostic_updates in server_diagnostics_updates.into_values() {
                    for (registration_id, diagnostic_updates) in diagnostic_updates {
                        lsp_store
                            .merge_lsp_diagnostics(
                                DiagnosticSourceKind::Pulled,
                                diagnostic_updates,
                                |document_uri, old_diagnostic, _| match old_diagnostic.source_kind {
                                    DiagnosticSourceKind::Pulled => {
                                        old_diagnostic.registration_id != registration_id
                                            || unchanged_buffers
                                                .get(&old_diagnostic.registration_id)
                                                .is_some_and(|unchanged_buffers| {
                                                    unchanged_buffers.contains(&document_uri)
                                                })
                                    }
                                    DiagnosticSourceKind::Other | DiagnosticSourceKind::Pushed => {
                                        true
                                    }
                                },
                                cx,
                            )
                            .log_err();
                    }
                }
            })
        })
    }
}
