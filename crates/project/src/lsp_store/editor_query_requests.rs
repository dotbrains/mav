use super::*;

impl LspStore {
    pub fn signature_help<T: ToPointUtf16>(
        &mut self,
        buffer: &Entity<Buffer>,
        position: T,
        cx: &mut Context<Self>,
    ) -> Task<Option<Vec<SignatureHelp>>> {
        let position = position.to_point_utf16(buffer.read(cx));

        if let Some((client, upstream_project_id)) = self.upstream_client() {
            let request = GetSignatureHelp { position };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(None);
            }
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
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let lsp_store = weak_lsp_store.upgrade()?;
                let signatures = join_all(
                    request_task
                        .await
                        .log_err()
                        .flatten()
                        .map(|response| response.payload)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|response| {
                            let response = GetSignatureHelp { position }.response_from_proto(
                                response.response,
                                lsp_store.clone(),
                                buffer.clone(),
                                cx.clone(),
                            );
                            async move { response.await.log_err().flatten() }
                        }),
                )
                .await
                .into_iter()
                .flatten()
                .collect();
                Some(signatures)
            })
        } else {
            let all_actions_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetSignatureHelp { position },
                cx,
            );
            cx.background_spawn(async move {
                Some(
                    all_actions_task
                        .await
                        .into_iter()
                        .flat_map(|(_, actions)| actions)
                        .collect::<Vec<_>>(),
                )
            })
        }
    }

    pub fn hover(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Option<Vec<Hover>>> {
        if let Some((client, upstream_project_id)) = self.upstream_client() {
            let request = GetHover { position };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(None);
            }
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
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let lsp_store = weak_lsp_store.upgrade()?;
                let hovers = join_all(
                    request_task
                        .await
                        .log_err()
                        .flatten()
                        .map(|response| response.payload)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|response| {
                            let response = GetHover { position }.response_from_proto(
                                response.response,
                                lsp_store.clone(),
                                buffer.clone(),
                                cx.clone(),
                            );
                            async move {
                                response
                                    .await
                                    .log_err()
                                    .flatten()
                                    .and_then(remove_empty_hover_blocks)
                            }
                        }),
                )
                .await
                .into_iter()
                .flatten()
                .collect();
                Some(hovers)
            })
        } else {
            let all_actions_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetHover { position },
                cx,
            );
            cx.background_spawn(async move {
                Some(
                    all_actions_task
                        .await
                        .into_iter()
                        .filter_map(|(_, hover)| remove_empty_hover_blocks(hover?))
                        .collect::<Vec<Hover>>(),
                )
            })
        }
    }

    pub fn symbols(&self, query: &str, cx: &mut Context<Self>) -> Task<Result<Vec<Symbol>>> {
        let language_registry = self.languages.clone();

        if let Some((upstream_client, project_id)) = self.upstream_client().as_ref() {
            let request = upstream_client.request(proto::GetProjectSymbols {
                project_id: *project_id,
                query: query.to_string(),
            });
            cx.foreground_executor().spawn(async move {
                let response = request.await?;
                let mut symbols = Vec::new();
                let core_symbols = response
                    .symbols
                    .into_iter()
                    .filter_map(|symbol| Self::deserialize_symbol(symbol).log_err())
                    .collect::<Vec<_>>();
                populate_labels_for_symbols(core_symbols, &language_registry, None, &mut symbols)
                    .await;
                Ok(symbols)
            })
        } else if let Some(local) = self.as_local() {
            struct WorkspaceSymbolsResult {
                server_id: LanguageServerId,
                lsp_adapter: Arc<CachedLspAdapter>,
                worktree: WeakEntity<Worktree>,
                lsp_symbols: Vec<(String, SymbolKind, lsp::Location, Option<String>)>,
            }

            let mut requests = Vec::new();
            let mut requested_servers = BTreeSet::new();
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();

            for (seed, state) in local.language_server_ids.iter() {
                let Some(worktree_handle) = self
                    .worktree_store
                    .read(cx)
                    .worktree_for_id(seed.worktree_id, cx)
                else {
                    continue;
                };

                let worktree = worktree_handle.read(cx);
                if !worktree.is_visible() {
                    continue;
                }

                if !requested_servers.insert(state.id) {
                    continue;
                }

                let (lsp_adapter, server) = match local.language_servers.get(&state.id) {
                    Some(LanguageServerState::Running {
                        adapter, server, ..
                    }) => (adapter.clone(), server),

                    _ => continue,
                };

                let supports_workspace_symbol_request =
                    match server.capabilities().workspace_symbol_provider {
                        Some(OneOf::Left(supported)) => supported,
                        Some(OneOf::Right(_)) => true,
                        None => false,
                    };

                if !supports_workspace_symbol_request {
                    continue;
                }

                let worktree_handle = worktree_handle.clone();
                let server_id = server.server_id();
                requests.push(
                    server
                        .request::<lsp::request::WorkspaceSymbolRequest>(
                            lsp::WorkspaceSymbolParams {
                                query: query.to_string(),
                                ..Default::default()
                            },
                            request_timeout,
                        )
                        .map(move |response| {
                            let lsp_symbols = response
                                .into_response()
                                .context("workspace symbols request")
                                .log_err()
                                .flatten()
                                .map(|symbol_response| match symbol_response {
                                    lsp::WorkspaceSymbolResponse::Flat(flat_responses) => {
                                        flat_responses
                                            .into_iter()
                                            .map(|lsp_symbol| {
                                                (
                                                    lsp_symbol.name,
                                                    lsp_symbol.kind,
                                                    lsp_symbol.location,
                                                    lsp_symbol.container_name,
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                    }
                                    lsp::WorkspaceSymbolResponse::Nested(nested_responses) => {
                                        nested_responses
                                            .into_iter()
                                            .filter_map(|lsp_symbol| {
                                                let location = match lsp_symbol.location {
                                                    OneOf::Left(location) => location,
                                                    OneOf::Right(_) => {
                                                        log::error!(
                                                            "Unexpected: client capabilities \
                                                            forbid symbol resolutions in \
                                                            workspace.symbol.resolveSupport"
                                                        );
                                                        return None;
                                                    }
                                                };
                                                Some((
                                                    lsp_symbol.name,
                                                    lsp_symbol.kind,
                                                    location,
                                                    lsp_symbol.container_name,
                                                ))
                                            })
                                            .collect::<Vec<_>>()
                                    }
                                })
                                .unwrap_or_default();

                            WorkspaceSymbolsResult {
                                server_id,
                                lsp_adapter,
                                worktree: worktree_handle.downgrade(),
                                lsp_symbols,
                            }
                        }),
                );
            }

            cx.spawn(async move |this, cx| {
                let responses = futures::future::join_all(requests).await;
                let this = match this.upgrade() {
                    Some(this) => this,
                    None => return Ok(Vec::new()),
                };

                let mut symbols = Vec::new();
                for result in responses {
                    let core_symbols = this.update(cx, |this, cx| {
                        result
                            .lsp_symbols
                            .into_iter()
                            .filter_map(
                                |(symbol_name, symbol_kind, symbol_location, container_name)| {
                                    let abs_path = symbol_location.uri.to_file_path().ok()?;
                                    let source_worktree = result.worktree.upgrade()?;
                                    let source_worktree_id = source_worktree.read(cx).id();

                                    let path = if let Some((tree, rel_path)) =
                                        this.worktree_store.read(cx).find_worktree(&abs_path, cx)
                                    {
                                        let worktree_id = tree.read(cx).id();
                                        SymbolLocation::InProject(ProjectPath {
                                            worktree_id,
                                            path: rel_path,
                                        })
                                    } else {
                                        SymbolLocation::OutsideProject {
                                            signature: this.symbol_signature(&abs_path),
                                            abs_path: abs_path.into(),
                                        }
                                    };

                                    Some(CoreSymbol {
                                        source_language_server_id: result.server_id,
                                        language_server_name: result.lsp_adapter.name.clone(),
                                        source_worktree_id,
                                        path,
                                        kind: symbol_kind,
                                        name: collapse_newlines(&symbol_name, "↵ "),
                                        range: range_from_lsp(symbol_location.range),
                                        container_name: container_name
                                            .map(|c| collapse_newlines(&c, "↵ ")),
                                    })
                                },
                            )
                            .collect::<Vec<_>>()
                    });

                    populate_labels_for_symbols(
                        core_symbols,
                        &language_registry,
                        Some(result.lsp_adapter),
                        &mut symbols,
                    )
                    .await;
                }

                Ok(symbols)
            })
        } else {
            Task::ready(Err(anyhow!("No upstream client or local language server")))
        }
    }
}
