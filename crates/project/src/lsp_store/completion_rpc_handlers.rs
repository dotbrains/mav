use super::*;

impl LspStore {
    pub(super) async fn handle_resolve_completion_documentation(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::ResolveCompletionDocumentation>,
        mut cx: AsyncApp,
    ) -> Result<proto::ResolveCompletionDocumentationResponse> {
        let lsp_completion = serde_json::from_slice(&envelope.payload.lsp_completion)?;

        let completion = this
            .read_with(&cx, |this, cx| {
                let id = LanguageServerId(envelope.payload.language_server_id as usize);
                let server = this
                    .language_server_for_id(id)
                    .with_context(|| format!("No language server {id}"))?;

                let request_timeout = ProjectSettings::get_global(cx)
                    .global_lsp_settings
                    .get_request_timeout();

                anyhow::Ok(cx.background_spawn(async move {
                    let can_resolve = server
                        .capabilities()
                        .completion_provider
                        .as_ref()
                        .and_then(|options| options.resolve_provider)
                        .unwrap_or(false);
                    if can_resolve {
                        server
                            .request::<lsp::request::ResolveCompletionItem>(
                                lsp_completion,
                                request_timeout,
                            )
                            .await
                            .into_response()
                            .context("resolve completion item")
                    } else {
                        anyhow::Ok(lsp_completion)
                    }
                }))
            })?
            .await?;

        let mut documentation_is_markdown = false;
        let lsp_completion = serde_json::to_string(&completion)?.into_bytes();
        let documentation = match completion.documentation {
            Some(lsp::Documentation::String(text)) => text,

            Some(lsp::Documentation::MarkupContent(lsp::MarkupContent { kind, value })) => {
                documentation_is_markdown = kind == lsp::MarkupKind::Markdown;
                value
            }

            _ => String::new(),
        };

        // If we have a new buffer_id, that means we're talking to a new client
        // and want to check for new text_edits in the completion too.
        let mut old_replace_start = None;
        let mut old_replace_end = None;
        let mut old_insert_start = None;
        let mut old_insert_end = None;
        let mut new_text = String::default();
        if let Ok(buffer_id) = BufferId::new(envelope.payload.buffer_id) {
            let buffer_snapshot = this.update(&mut cx, |this, cx| {
                let buffer = this.buffer_store.read(cx).get_existing(buffer_id)?;
                anyhow::Ok(buffer.read(cx).snapshot())
            })?;

            if let Some(text_edit) = completion.text_edit.as_ref() {
                let edit = parse_completion_text_edit(text_edit, &buffer_snapshot);

                if let Some(mut edit) = edit {
                    LineEnding::normalize(&mut edit.new_text);

                    new_text = edit.new_text;
                    old_replace_start = Some(serialize_anchor(&edit.replace_range.start));
                    old_replace_end = Some(serialize_anchor(&edit.replace_range.end));
                    if let Some(insert_range) = edit.insert_range {
                        old_insert_start = Some(serialize_anchor(&insert_range.start));
                        old_insert_end = Some(serialize_anchor(&insert_range.end));
                    }
                }
            }
        }

        Ok(proto::ResolveCompletionDocumentationResponse {
            documentation,
            documentation_is_markdown,
            old_replace_start,
            old_replace_end,
            new_text,
            lsp_completion,
            old_insert_start,
            old_insert_end,
        })
    }

    pub(super) async fn handle_apply_additional_edits_for_completion(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::ApplyCompletionAdditionalEdits>,
        mut cx: AsyncApp,
    ) -> Result<proto::ApplyCompletionAdditionalEditsResponse> {
        let (buffer, completion, all_commit_ranges) = this.update(&mut cx, |this, cx| {
            let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
            let buffer = this.buffer_store.read(cx).get_existing(buffer_id)?;
            let completion = Self::deserialize_completion(
                envelope.payload.completion.context("invalid completion")?,
            )?;
            let all_commit_ranges = envelope
                .payload
                .all_commit_ranges
                .into_iter()
                .map(language::proto::deserialize_anchor_range)
                .collect::<Result<Vec<_>, _>>()?;
            anyhow::Ok((buffer, completion, all_commit_ranges))
        })?;

        let apply_additional_edits = this.update(&mut cx, |this, cx| {
            this.apply_additional_edits_for_completion(
                buffer,
                Rc::new(RefCell::new(Box::new([Completion {
                    replace_range: completion.replace_range,
                    new_text: completion.new_text,
                    source: completion.source,
                    documentation: None,
                    label: CodeLabel::default(),
                    match_start: None,
                    snippet_deduplication_key: None,
                    insert_text_mode: None,
                    icon_path: None,
                    icon_color: None,
                    confirm: None,
                    group: None,
                }]))),
                0,
                false,
                all_commit_ranges,
                cx,
            )
        });

        Ok(proto::ApplyCompletionAdditionalEditsResponse {
            transaction: apply_additional_edits
                .await?
                .as_ref()
                .map(language::proto::serialize_transaction),
        })
    }
}
