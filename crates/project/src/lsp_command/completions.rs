use super::*;

impl GetCompletions {
    pub fn can_resolve_completions(capabilities: &lsp::ServerCapabilities) -> bool {
        capabilities
            .completion_provider
            .as_ref()
            .and_then(|options| options.resolve_provider)
            .unwrap_or(false)
    }
}

#[async_trait(?Send)]
impl LspCommand for GetCompletions {
    type Response = CoreCompletionResponse;
    type LspRequest = lsp::request::Completion;
    type ProtoRequest = proto::GetCompletions;

    fn display_name(&self) -> &str {
        "Get completion"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        capabilities
            .server_capabilities
            .completion_provider
            .is_some()
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::CompletionParams> {
        Ok(lsp::CompletionParams {
            text_document_position: make_lsp_text_document_position(path, self.position)?,
            context: Some(self.context.clone()),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        completions: Option<lsp::CompletionResponse>,
        lsp_store: Entity<LspStore>,
        buffer: Entity<Buffer>,
        server_id: LanguageServerId,
        mut cx: AsyncApp,
    ) -> Result<Self::Response> {
        let mut response_list = None;
        let (mut completions, mut is_incomplete) = if let Some(completions) = completions {
            match completions {
                lsp::CompletionResponse::Array(completions) => (completions, false),
                lsp::CompletionResponse::List(mut list) => {
                    let is_incomplete = list.is_incomplete;
                    let items = std::mem::take(&mut list.items);
                    response_list = Some(list);
                    (items, is_incomplete)
                }
            }
        } else {
            (Vec::new(), false)
        };

        let unfiltered_completions_count = completions.len();

        let language_server_adapter = lsp_store
            .read_with(&cx, |lsp_store, _| {
                lsp_store.language_server_adapter_for_id(server_id)
            })
            .with_context(|| format!("no language server with id {server_id}"))?;

        let lsp_defaults = response_list
            .as_ref()
            .and_then(|list| list.item_defaults.clone())
            .map(Arc::new);

        let mut completion_edits = Vec::new();
        buffer.update(&mut cx, |buffer, _cx| {
            let snapshot = buffer.snapshot();
            let clipped_position = buffer.clip_point_utf16(Unclipped(self.position), Bias::Left);

            let mut range_for_token = None;
            completions.retain(|lsp_completion| {
                let lsp_edit = lsp_completion.text_edit.clone().or_else(|| {
                    let default_text_edit = lsp_defaults.as_deref()?.edit_range.as_ref()?;
                    let new_text = lsp_completion
                        .text_edit_text
                        .as_ref()
                        .unwrap_or(&lsp_completion.label)
                        .clone();
                    match default_text_edit {
                        CompletionListItemDefaultsEditRange::Range(range) => {
                            Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                                range: *range,
                                new_text,
                            }))
                        }
                        CompletionListItemDefaultsEditRange::InsertAndReplace {
                            insert,
                            replace,
                        } => Some(lsp::CompletionTextEdit::InsertAndReplace(
                            lsp::InsertReplaceEdit {
                                new_text,
                                insert: *insert,
                                replace: *replace,
                            },
                        )),
                    }
                });

                let edit = match lsp_edit {
                    // If the language server provides a range to overwrite, then
                    // check that the range is valid.
                    Some(completion_text_edit) => {
                        match parse_completion_text_edit(&completion_text_edit, &snapshot) {
                            Some(edit) => edit,
                            None => return false,
                        }
                    }
                    // If the language server does not provide a range, then infer
                    // the range based on the syntax tree.
                    None => {
                        if self.position != clipped_position {
                            log::info!("completion out of expected range ");
                            return false;
                        }

                        let default_edit_range = lsp_defaults.as_ref().and_then(|lsp_defaults| {
                            lsp_defaults
                                .edit_range
                                .as_ref()
                                .and_then(|range| match range {
                                    CompletionListItemDefaultsEditRange::Range(r) => Some(r),
                                    _ => None,
                                })
                        });

                        let range = if let Some(range) = default_edit_range {
                            let range = range_from_lsp(*range);
                            let start = snapshot.clip_point_utf16(range.start, Bias::Left);
                            let end = snapshot.clip_point_utf16(range.end, Bias::Left);
                            if start != range.start.0 || end != range.end.0 {
                                log::info!("completion out of expected range");
                                return false;
                            }

                            snapshot.anchor_before(start)..snapshot.anchor_after(end)
                        } else {
                            range_for_token
                                .get_or_insert_with(|| {
                                    let offset = self.position.to_offset(&snapshot);
                                    let (range, kind) = snapshot.surrounding_word(
                                        offset,
                                        Some(CharScopeContext::Completion),
                                    );
                                    let range = if kind == Some(CharKind::Word) {
                                        range
                                    } else {
                                        offset..offset
                                    };

                                    snapshot.anchor_before(range.start)
                                        ..snapshot.anchor_after(range.end)
                                })
                                .clone()
                        };

                        // We already know text_edit is None here
                        let text = lsp_completion
                            .insert_text
                            .as_ref()
                            .unwrap_or(&lsp_completion.label)
                            .clone();

                        ParsedCompletionEdit {
                            replace_range: range,
                            insert_range: None,
                            new_text: text,
                        }
                    }
                };

                completion_edits.push(edit);
                true
            });
        });

        // If completions were filtered out due to errors that may be transient, mark the result
        // incomplete so that it is re-queried.
        if unfiltered_completions_count != completions.len() {
            is_incomplete = true;
        }

        language_server_adapter
            .process_completions(&mut completions)
            .await;

        let completions = completions
            .into_iter()
            .zip(completion_edits)
            .map(|(mut lsp_completion, mut edit)| {
                LineEnding::normalize(&mut edit.new_text);
                if lsp_completion.data.is_none()
                    && let Some(default_data) = lsp_defaults
                        .as_ref()
                        .and_then(|item_defaults| item_defaults.data.clone())
                {
                    // Servers (e.g. JDTLS) prefer unchanged completions, when resolving the items later,
                    // so we do not insert the defaults here, but `data` is needed for resolving, so this is an exception.
                    lsp_completion.data = Some(default_data);
                }
                CoreCompletion {
                    replace_range: edit.replace_range,
                    new_text: edit.new_text,
                    source: CompletionSource::Lsp {
                        insert_range: edit.insert_range,
                        server_id,
                        lsp_completion: Box::new(lsp_completion),
                        lsp_defaults: lsp_defaults.clone(),
                        resolved: false,
                    },
                }
            })
            .collect();

        Ok(CoreCompletionResponse {
            completions,
            is_incomplete,
        })
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::GetCompletions {
        let anchor = buffer.anchor_after(self.position);
        proto::GetCompletions {
            project_id,
            buffer_id: buffer.remote_id().into(),
            position: Some(language::proto::serialize_anchor(&anchor)),
            version: serialize_version(&buffer.version()),
            server_id: self.server_id.map(|id| id.to_proto()),
        }
    }

    async fn from_proto(
        message: proto::GetCompletions,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self> {
        let version = deserialize_version(&message.version);
        buffer
            .update(&mut cx, |buffer, _| buffer.wait_for_version(version))
            .await?;
        let position = message
            .position
            .and_then(language::proto::deserialize_anchor)
            .map(|p| {
                buffer.read_with(&cx, |buffer, _| {
                    buffer.clip_point_utf16(Unclipped(p.to_point_utf16(buffer)), Bias::Left)
                })
            })
            .context("invalid position")?;
        Ok(Self {
            position,
            context: CompletionContext {
                trigger_kind: CompletionTriggerKind::INVOKED,
                trigger_character: None,
            },
            server_id: message
                .server_id
                .map(|id| lsp::LanguageServerId::from_proto(id)),
        })
    }

    fn response_to_proto(
        response: CoreCompletionResponse,
        _: &mut LspStore,
        _: PeerId,
        buffer_version: &clock::Global,
        _: &mut App,
    ) -> proto::GetCompletionsResponse {
        proto::GetCompletionsResponse {
            completions: response
                .completions
                .iter()
                .map(LspStore::serialize_completion)
                .collect(),
            version: serialize_version(buffer_version),
            can_reuse: !response.is_incomplete,
        }
    }

    async fn response_from_proto(
        self,
        message: proto::GetCompletionsResponse,
        _project: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self::Response> {
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;

        let completions = message
            .completions
            .into_iter()
            .map(LspStore::deserialize_completion)
            .collect::<Result<Vec<_>>>()?;

        Ok(CoreCompletionResponse {
            completions,
            is_incomplete: !message.can_reuse,
        })
    }

    fn buffer_id_from_proto(message: &proto::GetCompletions) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}

pub struct ParsedCompletionEdit {
    pub replace_range: Range<Anchor>,
    pub insert_range: Option<Range<Anchor>>,
    pub new_text: String,
}

pub(crate) fn parse_completion_text_edit(
    edit: &lsp::CompletionTextEdit,
    snapshot: &BufferSnapshot,
) -> Option<ParsedCompletionEdit> {
    let (replace_range, insert_range, new_text) = match edit {
        lsp::CompletionTextEdit::Edit(edit) => (edit.range, None, &edit.new_text),
        lsp::CompletionTextEdit::InsertAndReplace(edit) => {
            (edit.replace, Some(edit.insert), &edit.new_text)
        }
    };

    let replace_range = {
        let range = range_from_lsp(replace_range);
        let start = snapshot.clip_point_utf16(range.start, Bias::Left);
        let end = snapshot.clip_point_utf16(range.end, Bias::Left);
        if start != range.start.0 || end != range.end.0 {
            log::info!(
                "completion out of expected range, start: {start:?}, end: {end:?}, range: {range:?}"
            );
            return None;
        }
        snapshot.anchor_before(start)..snapshot.anchor_after(end)
    };

    let insert_range = match insert_range {
        None => None,
        Some(insert_range) => {
            let range = range_from_lsp(insert_range);
            let start = snapshot.clip_point_utf16(range.start, Bias::Left);
            let end = snapshot.clip_point_utf16(range.end, Bias::Left);
            if start != range.start.0 || end != range.end.0 {
                log::info!("completion (insert) out of expected range");
                return None;
            }
            Some(snapshot.anchor_before(start)..snapshot.anchor_after(end))
        }
    };

    Some(ParsedCompletionEdit {
        insert_range,
        replace_range,
        new_text: new_text.clone(),
    })
}
