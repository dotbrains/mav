use super::*;

impl LspStore {
    pub fn last_formatting_failure(&self) -> Option<&str> {
        self.last_formatting_failure.as_deref()
    }

    pub fn reset_last_formatting_failure(&mut self) {
        self.last_formatting_failure = None;
    }

    pub fn environment_for_buffer(
        &self,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Shared<Task<Option<HashMap<String, String>>>> {
        if let Some(environment) = &self.as_local().map(|local| local.environment.clone()) {
            environment.update(cx, |env, cx| {
                env.buffer_environment(buffer, &self.worktree_store, cx)
            })
        } else {
            Task::ready(None).shared()
        }
    }

    pub fn format(
        &mut self,
        buffers: HashSet<Entity<Buffer>>,
        target: LspFormatTarget,
        push_to_history: bool,
        trigger: FormatTrigger,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<ProjectTransaction>> {
        let logger = zlog::scoped!("format");
        if self.as_local().is_some() {
            zlog::trace!(logger => "Formatting locally");
            let logger = zlog::scoped!(logger => "local");
            let buffers = buffers
                .into_iter()
                .map(|buffer_handle| {
                    let buffer = buffer_handle.read(cx);
                    let buffer_abs_path = File::from_dyn(buffer.file())
                        .and_then(|file| file.as_local().map(|f| f.abs_path(cx)));

                    (buffer_handle, buffer_abs_path, buffer.remote_id())
                })
                .collect::<Vec<_>>();

            cx.spawn(async move |lsp_store, cx| {
                let mut formattable_buffers = Vec::with_capacity(buffers.len());

                for (handle, abs_path, id) in buffers {
                    let env = lsp_store
                        .update(cx, |lsp_store, cx| {
                            lsp_store.environment_for_buffer(&handle, cx)
                        })?
                        .await;

                    let ranges = match &target {
                        LspFormatTarget::Buffers => None,
                        LspFormatTarget::Ranges(ranges) => {
                            Some(ranges.get(&id).context("No format ranges provided for buffer")?.clone())
                        }
                    };

                    formattable_buffers.push(FormattableBuffer {
                        handle,
                        abs_path,
                        env,
                        ranges,
                    });
                }
                zlog::trace!(logger => "Formatting {:?} buffers", formattable_buffers.len());

                let format_timer = zlog::time!(logger => "Formatting buffers");
                let result = LocalLspStore::format_locally(
                    lsp_store.clone(),
                    formattable_buffers,
                    push_to_history,
                    trigger,
                    logger,
                    cx,
                )
                .await;
                format_timer.end();

                zlog::trace!(logger => "Formatting completed with result {:?}", result.as_ref().map(|_| "<project-transaction>"));

                lsp_store.update(cx, |lsp_store, _| {
                    lsp_store.update_last_formatting_failure(&result);
                })?;

                result
            })
        } else if let Some((client, project_id)) = self.upstream_client() {
            zlog::trace!(logger => "Formatting remotely");
            let logger = zlog::scoped!(logger => "remote");

            let buffer_ranges = match &target {
                LspFormatTarget::Buffers => Vec::new(),
                LspFormatTarget::Ranges(ranges) => ranges
                    .iter()
                    .map(|(buffer_id, ranges)| proto::BufferFormatRanges {
                        buffer_id: buffer_id.to_proto(),
                        ranges: ranges.iter().cloned().map(serialize_anchor_range).collect(),
                    })
                    .collect(),
            };

            let buffer_store = self.buffer_store();
            cx.spawn(async move |lsp_store, cx| {
                zlog::trace!(logger => "Sending remote format request");
                let request_timer = zlog::time!(logger => "remote format request");
                let result = client
                    .request(proto::FormatBuffers {
                        project_id,
                        trigger: trigger as i32,
                        buffer_ids: buffers
                            .iter()
                            .map(|buffer| buffer.read_with(cx, |buffer, _| buffer.remote_id().to_proto()))
                            .collect(),
                        buffer_ranges,
                    })
                    .await
                    .and_then(|result| result.transaction.context("missing transaction"));
                request_timer.end();

                zlog::trace!(logger => "Remote format request resolved to {:?}", result.as_ref().map(|_| "<project_transaction>"));

                lsp_store.update(cx, |lsp_store, _| {
                    lsp_store.update_last_formatting_failure(&result);
                })?;

                let transaction_response = result?;
                let _timer = zlog::time!(logger => "deserializing project transaction");
                buffer_store
                    .update(cx, |buffer_store, cx| {
                        buffer_store.deserialize_project_transaction(
                            transaction_response,
                            push_to_history,
                            cx,
                        )
                    })
                    .await
            })
        } else {
            zlog::trace!(logger => "Not formatting");
            Task::ready(Ok(ProjectTransaction::default()))
        }
    }

    pub(super) async fn handle_format_buffers(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::FormatBuffers>,
        mut cx: AsyncApp,
    ) -> Result<proto::FormatBuffersResponse> {
        let sender_id = envelope.original_sender_id().unwrap_or_default();
        let format = this.update(&mut cx, |this, cx| {
            let mut buffers = HashSet::default();
            for buffer_id in &envelope.payload.buffer_ids {
                let buffer_id = BufferId::new(*buffer_id)?;
                buffers.insert(this.buffer_store.read(cx).get_existing(buffer_id)?);
            }

            let target = if envelope.payload.buffer_ranges.is_empty() {
                LspFormatTarget::Buffers
            } else {
                let mut ranges_map = BTreeMap::new();
                for buffer_range in &envelope.payload.buffer_ranges {
                    let buffer_id = BufferId::new(buffer_range.buffer_id)?;
                    let ranges: Result<Vec<_>> = buffer_range
                        .ranges
                        .iter()
                        .map(|range| {
                            deserialize_anchor_range(range.clone()).context("invalid anchor range")
                        })
                        .collect();
                    ranges_map.insert(buffer_id, ranges?);
                }
                LspFormatTarget::Ranges(ranges_map)
            };

            let trigger = FormatTrigger::from_proto(envelope.payload.trigger);
            anyhow::Ok(this.format(buffers, target, false, trigger, cx))
        })?;

        let project_transaction = format.await?;
        let project_transaction = this.update(&mut cx, |this, cx| {
            this.buffer_store.update(cx, |buffer_store, cx| {
                buffer_store.serialize_project_transaction_for_peer(
                    project_transaction,
                    sender_id,
                    cx,
                )
            })
        });
        Ok(proto::FormatBuffersResponse {
            transaction: Some(project_transaction),
        })
    }

    pub(super) async fn handle_apply_code_action_kind(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::ApplyCodeActionKind>,
        mut cx: AsyncApp,
    ) -> Result<proto::ApplyCodeActionKindResponse> {
        let sender_id = envelope.original_sender_id().unwrap_or_default();
        let format = this.update(&mut cx, |this, cx| {
            let mut buffers = HashSet::default();
            for buffer_id in &envelope.payload.buffer_ids {
                let buffer_id = BufferId::new(*buffer_id)?;
                buffers.insert(this.buffer_store.read(cx).get_existing(buffer_id)?);
            }
            let kind = match envelope.payload.kind.as_str() {
                "" => CodeActionKind::EMPTY,
                "quickfix" => CodeActionKind::QUICKFIX,
                "refactor" => CodeActionKind::REFACTOR,
                "refactor.extract" => CodeActionKind::REFACTOR_EXTRACT,
                "refactor.inline" => CodeActionKind::REFACTOR_INLINE,
                "refactor.rewrite" => CodeActionKind::REFACTOR_REWRITE,
                "source" => CodeActionKind::SOURCE,
                "source.organizeImports" => CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                "source.fixAll" => CodeActionKind::SOURCE_FIX_ALL,
                _ => anyhow::bail!(
                    "Invalid code action kind {}",
                    envelope.payload.kind.as_str()
                ),
            };
            anyhow::Ok(this.apply_code_action_kind(buffers, kind, false, cx))
        })?;

        let project_transaction = format.await?;
        let project_transaction = this.update(&mut cx, |this, cx| {
            this.buffer_store.update(cx, |buffer_store, cx| {
                buffer_store.serialize_project_transaction_for_peer(
                    project_transaction,
                    sender_id,
                    cx,
                )
            })
        });
        Ok(proto::ApplyCodeActionKindResponse {
            transaction: Some(project_transaction),
        })
    }

    pub(super) fn update_last_formatting_failure<T>(
        &mut self,
        formatting_result: &anyhow::Result<T>,
    ) {
        match &formatting_result {
            Ok(_) => self.last_formatting_failure = None,
            Err(error) => {
                let error_string = format!("{error:#}");
                log::error!("Formatting failed: {error_string}");
                self.last_formatting_failure
                    .replace(error_string.lines().join(" "));
            }
        }
    }
}
