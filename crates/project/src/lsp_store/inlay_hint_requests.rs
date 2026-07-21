use super::*;

impl LspStore {
    pub fn applicable_inlay_chunks(
        &mut self,
        buffer: &Entity<Buffer>,
        ranges: &[Range<text::Anchor>],
        cx: &mut Context<Self>,
    ) -> Vec<Range<BufferRow>> {
        let buffer_snapshot = buffer.read(cx).snapshot();
        let ranges = ranges
            .iter()
            .map(|range| range.to_point(&buffer_snapshot))
            .collect::<Vec<_>>();

        self.latest_lsp_data(buffer, cx)
            .inlay_hints
            .applicable_chunks(ranges.as_slice())
            .map(|chunk| chunk.row_range())
            .collect()
    }

    pub fn invalidate_inlay_hints<'a>(
        &'a mut self,
        for_buffers: impl IntoIterator<Item = &'a BufferId> + 'a,
    ) {
        for buffer_id in for_buffers {
            if let Some(lsp_data) = self.lsp_data.get_mut(buffer_id) {
                lsp_data.inlay_hints.clear();
            }
        }
    }

    pub fn inlay_hints(
        &mut self,
        invalidate: InvalidationStrategy,
        buffer: Entity<Buffer>,
        ranges: Vec<Range<text::Anchor>>,
        known_chunks: Option<(clock::Global, HashSet<Range<BufferRow>>)>,
        cx: &mut Context<Self>,
    ) -> HashMap<Range<BufferRow>, Task<Result<CacheInlayHints>>> {
        let next_hint_id = self.next_hint_id.clone();
        let lsp_data = self.latest_lsp_data(&buffer, cx);
        let query_version = lsp_data.buffer_version.clone();
        let mut lsp_refresh_requested = false;
        let for_server = if let InvalidationStrategy::RefreshRequested {
            server_id,
            request_id,
        } = invalidate
        {
            let invalidated = lsp_data
                .inlay_hints
                .invalidate_for_server_refresh(server_id, request_id);
            lsp_refresh_requested = invalidated;
            Some(server_id)
        } else {
            None
        };
        let existing_inlay_hints = &mut lsp_data.inlay_hints;
        let known_chunks = known_chunks
            .filter(|(known_version, _)| !lsp_data.buffer_version.changed_since(known_version))
            .map(|(_, known_chunks)| known_chunks)
            .unwrap_or_default();

        let buffer_snapshot = buffer.read(cx).snapshot();
        let ranges = ranges
            .iter()
            .map(|range| range.to_point(&buffer_snapshot))
            .collect::<Vec<_>>();

        let mut hint_fetch_tasks = Vec::new();
        let mut cached_inlay_hints = None;
        let mut ranges_to_query = None;
        let applicable_chunks = existing_inlay_hints
            .applicable_chunks(ranges.as_slice())
            .filter(|chunk| !known_chunks.contains(&chunk.row_range()))
            .collect::<Vec<_>>();
        if applicable_chunks.is_empty() {
            return HashMap::default();
        }

        for row_chunk in applicable_chunks {
            match (
                existing_inlay_hints
                    .cached_hints(&row_chunk)
                    .filter(|_| !lsp_refresh_requested)
                    .cloned(),
                existing_inlay_hints
                    .fetched_hints(&row_chunk)
                    .as_ref()
                    .filter(|_| !lsp_refresh_requested)
                    .cloned(),
            ) {
                (None, None) => {
                    let chunk_range = row_chunk.anchor_range();
                    ranges_to_query
                        .get_or_insert_with(Vec::new)
                        .push((row_chunk, chunk_range));
                }
                (None, Some(fetched_hints)) => hint_fetch_tasks.push((row_chunk, fetched_hints)),
                (Some(cached_hints), None) => {
                    for (server_id, cached_hints) in cached_hints {
                        if for_server.is_none_or(|for_server| for_server == server_id) {
                            cached_inlay_hints
                                .get_or_insert_with(HashMap::default)
                                .entry(row_chunk.row_range())
                                .or_insert_with(HashMap::default)
                                .entry(server_id)
                                .or_insert_with(Vec::new)
                                .extend(cached_hints);
                        }
                    }
                }
                (Some(cached_hints), Some(fetched_hints)) => {
                    hint_fetch_tasks.push((row_chunk, fetched_hints));
                    for (server_id, cached_hints) in cached_hints {
                        if for_server.is_none_or(|for_server| for_server == server_id) {
                            cached_inlay_hints
                                .get_or_insert_with(HashMap::default)
                                .entry(row_chunk.row_range())
                                .or_insert_with(HashMap::default)
                                .entry(server_id)
                                .or_insert_with(Vec::new)
                                .extend(cached_hints);
                        }
                    }
                }
            }
        }

        if hint_fetch_tasks.is_empty()
            && ranges_to_query
                .as_ref()
                .is_none_or(|ranges| ranges.is_empty())
            && let Some(cached_inlay_hints) = cached_inlay_hints
        {
            cached_inlay_hints
                .into_iter()
                .map(|(row_chunk, hints)| (row_chunk, Task::ready(Ok(hints))))
                .collect()
        } else {
            for (chunk, range_to_query) in ranges_to_query.into_iter().flatten() {
                // When a server refresh was requested, other servers' cached hints
                // are unaffected by the refresh and must be included in the result.
                // Otherwise apply_fetched_hints (with should_invalidate()=true)
                // removes all visible hints but only adds back the requesting
                // server's new hints, permanently losing other servers' hints.
                let other_servers_cached: CacheInlayHints = if lsp_refresh_requested {
                    lsp_data
                        .inlay_hints
                        .cached_hints(&chunk)
                        .cloned()
                        .unwrap_or_default()
                } else {
                    HashMap::default()
                };

                let next_hint_id = next_hint_id.clone();
                let buffer = buffer.clone();
                let query_version = query_version.clone();
                let new_inlay_hints = cx
                    .spawn(async move |lsp_store, cx| {
                        let new_fetch_task = lsp_store.update(cx, |lsp_store, cx| {
                            lsp_store.fetch_inlay_hints(for_server, &buffer, range_to_query, cx)
                        })?;
                        new_fetch_task
                            .await
                            .and_then(|new_hints_by_server| {
                                lsp_store.update(cx, |lsp_store, cx| {
                                    let lsp_data = lsp_store.latest_lsp_data(&buffer, cx);
                                    let update_cache = lsp_data.buffer_version == query_version;
                                    if new_hints_by_server.is_empty() {
                                        if update_cache {
                                            lsp_data.inlay_hints.invalidate_for_chunk(chunk);
                                        }
                                        other_servers_cached
                                    } else {
                                        let mut result = other_servers_cached;
                                        for (server_id, new_hints) in new_hints_by_server {
                                            let new_hints = new_hints
                                                .into_iter()
                                                .map(|new_hint| {
                                                    (
                                                        InlayId::Hint(next_hint_id.fetch_add(
                                                            1,
                                                            atomic::Ordering::AcqRel,
                                                        )),
                                                        new_hint,
                                                    )
                                                })
                                                .collect::<Vec<_>>();
                                            if update_cache {
                                                lsp_data.inlay_hints.insert_new_hints(
                                                    chunk,
                                                    server_id,
                                                    new_hints.clone(),
                                                );
                                            }
                                            result.insert(server_id, new_hints);
                                        }
                                        result
                                    }
                                })
                            })
                            .map_err(Arc::new)
                    })
                    .shared();

                let fetch_task = lsp_data.inlay_hints.fetched_hints(&chunk);
                *fetch_task = Some(new_inlay_hints.clone());
                hint_fetch_tasks.push((chunk, new_inlay_hints));
            }

            cached_inlay_hints
                .unwrap_or_default()
                .into_iter()
                .map(|(row_chunk, hints)| (row_chunk, Task::ready(Ok(hints))))
                .chain(hint_fetch_tasks.into_iter().map(|(chunk, hints_fetch)| {
                    (
                        chunk.row_range(),
                        cx.spawn(async move |_, _| {
                            hints_fetch.await.map_err(|e| {
                                if e.error_code() != ErrorCode::Internal {
                                    anyhow!(e.error_code())
                                } else {
                                    anyhow!("{e:#}")
                                }
                            })
                        }),
                    )
                }))
                .collect()
        }
    }

    pub(super) fn fetch_inlay_hints(
        &mut self,
        for_server: Option<LanguageServerId>,
        buffer: &Entity<Buffer>,
        range: Range<Anchor>,
        cx: &mut Context<Self>,
    ) -> Task<Result<HashMap<LanguageServerId, Vec<InlayHint>>>> {
        let request = InlayHints {
            range: range.clone(),
        };
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(HashMap::default()));
            }
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = upstream_client.request_lsp(
                project_id,
                for_server.map(|id| id.to_proto()),
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(HashMap::default());
                };
                let Some(responses) = request_task.await? else {
                    return Ok(HashMap::default());
                };

                let inlay_hints = join_all(responses.payload.into_iter().map(|response| {
                    let lsp_store = lsp_store.clone();
                    let buffer = buffer.clone();
                    let cx = cx.clone();
                    let request = request.clone();
                    async move {
                        (
                            LanguageServerId::from_proto(response.server_id),
                            request
                                .response_from_proto(response.response, lsp_store, buffer, cx)
                                .await,
                        )
                    }
                }))
                .await;

                let buffer_snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());
                let mut has_errors = false;
                let inlay_hints = inlay_hints
                    .into_iter()
                    .filter_map(|(server_id, inlay_hints)| match inlay_hints {
                        Ok(inlay_hints) => Some((server_id, inlay_hints)),
                        Err(e) => {
                            has_errors = true;
                            log::error!("{e:#}");
                            None
                        }
                    })
                    .map(|(server_id, mut new_hints)| {
                        new_hints.retain(|hint| {
                            hint.position.is_valid(&buffer_snapshot)
                                && range.start.is_valid(&buffer_snapshot)
                                && range.end.is_valid(&buffer_snapshot)
                                && hint.position.cmp(&range.start, &buffer_snapshot).is_ge()
                                && hint.position.cmp(&range.end, &buffer_snapshot).is_lt()
                        });
                        (server_id, new_hints)
                    })
                    .collect::<HashMap<_, _>>();
                anyhow::ensure!(
                    !has_errors || !inlay_hints.is_empty(),
                    "Failed to fetch inlay hints"
                );
                Ok(inlay_hints)
            })
        } else {
            let inlay_hints_task = match for_server {
                Some(server_id) => {
                    let server_task = self.request_lsp(
                        buffer.clone(),
                        LanguageServerToQuery::Other(server_id),
                        request,
                        cx,
                    );
                    cx.background_spawn(async move {
                        let mut responses = Vec::new();
                        match server_task.await {
                            Ok(response) => responses.push((server_id, response)),
                            // rust-analyzer likes to error with this when its still loading up
                            Err(e) if format!("{e:#}").ends_with("content modified") => (),
                            Err(e) => log::error!(
                                "Error handling response for inlay hints request: {e:#}"
                            ),
                        }
                        responses
                    })
                }
                None => self.request_multiple_lsp_locally(buffer, None::<usize>, request, cx),
            };
            let buffer_snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());
            cx.background_spawn(async move {
                Ok(inlay_hints_task
                    .await
                    .into_iter()
                    .map(|(server_id, mut new_hints)| {
                        new_hints.retain(|hint| {
                            hint.position.is_valid(&buffer_snapshot)
                                && range.start.is_valid(&buffer_snapshot)
                                && range.end.is_valid(&buffer_snapshot)
                                && hint.position.cmp(&range.start, &buffer_snapshot).is_ge()
                                && hint.position.cmp(&range.end, &buffer_snapshot).is_lt()
                        });
                        (server_id, new_hints)
                    })
                    .collect())
            })
        }
    }
}
