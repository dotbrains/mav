use super::*;

pub fn show_link_definition(
    shift_held: bool,
    editor: &mut Editor,
    trigger_point: TriggerPoint,
    snapshot: &EditorSnapshot,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    let preferred_kind = match trigger_point {
        TriggerPoint::Text(_) if !shift_held => GotoDefinitionKind::Symbol,
        _ => GotoDefinitionKind::Type,
    };

    let (mut hovered_link_state, is_cached) =
        if let Some(existing) = editor.hovered_link_state.take() {
            (existing, true)
        } else {
            (
                HoveredLinkState {
                    last_trigger_point: trigger_point.clone(),
                    symbol_range: None,
                    preferred_kind,
                    links: vec![],
                    task: None,
                },
                false,
            )
        };

    if editor.pending_rename.is_some() {
        return;
    }

    let anchor = trigger_point.anchor().bias_left(snapshot.buffer_snapshot());
    let Some((anchor, _)) = snapshot.buffer_snapshot().anchor_to_buffer_anchor(anchor) else {
        return;
    };
    let Some(buffer) = editor.buffer.read(cx).buffer(anchor.buffer_id) else {
        return;
    };
    let same_kind = hovered_link_state.preferred_kind == preferred_kind
        || hovered_link_state
            .links
            .first()
            .is_some_and(|d| matches!(d, HoverLink::Url(_) | HoverLink::LspLocation(_, _)));

    if same_kind {
        if is_cached && (hovered_link_state.last_trigger_point == trigger_point)
            || hovered_link_state
                .symbol_range
                .as_ref()
                .is_some_and(|symbol_range| {
                    symbol_range.point_within_range(&trigger_point, snapshot)
                })
        {
            editor.hovered_link_state = Some(hovered_link_state);
            return;
        }
    } else {
        editor.hide_hovered_link(cx)
    }
    let project = editor.project.clone();
    let provider = editor.semantics_provider.clone();

    // Record the requested position so a mouse move on the same point short-circuits
    // instead of re-querying, even when the server returns no `originSelectionRange`
    // (which would otherwise leave `symbol_range` empty).
    hovered_link_state.last_trigger_point = trigger_point.clone();

    hovered_link_state.task = Some(cx.spawn_in(window, async move |this, cx| {
        async move {
            // LSP document links take priority: the server explicitly
            // declares which ranges are clickable, so they are more
            // accurate than the heuristic-based URL/file detection.
            //
            // Resolution is deduplicated by `LspStore`; awaiting here only
            // blocks until either the cached resolved entry is returned or
            // the in-flight `Shared` task completes.
            let resolved_document_links = this
                .update(cx, |editor, cx| {
                    editor.document_links_at(buffer.clone(), anchor, cx)
                })
                .ok()
                .flatten();
            let resolved_document_links = match resolved_document_links {
                Some(task) => task.await,
                None => Vec::new(),
            };
            let snapshot = this.read_with(cx, |editor, cx| editor.buffer.read(cx).snapshot(cx))?;
            let detected_document_link =
                resolved_document_links
                    .into_iter()
                    .find_map(|(server_id, link)| {
                        let multi_buffer_range =
                            snapshot.buffer_anchor_range_to_anchor_range(link.range.clone())?;
                        Some((link.range, multi_buffer_range, link.target, server_id))
                    });
            drop(snapshot);

            let result = match &trigger_point {
                TriggerPoint::Text(_) => {
                    let mut links = Vec::new();
                    let mut symbol_range = None;

                    // LSP-provided document link wins over heuristic URL/file
                    // detection at the same position: the server tells us the
                    // exact range and target, while `find_url`/`find_file` are
                    // best-effort text matches.
                    if let Some((_, multi_buffer_range, Some(target), server_id)) =
                        detected_document_link.clone()
                    {
                        symbol_range = Some(RangeInEditor::Text(multi_buffer_range));
                        links.push(document_link_target_to_hover_link(&target, server_id));
                    } else if let Some((url_range, url)) = find_url(&buffer, anchor, cx) {
                        let snapshot =
                            this.read_with(cx, |editor, cx| editor.buffer.read(cx).snapshot(cx))?;
                        if let Some(range) = snapshot.buffer_anchor_range_to_anchor_range(url_range)
                        {
                            symbol_range = Some(RangeInEditor::Text(range));
                        }
                        links.push(HoverLink::Url(url));
                    } else if let Some((filename_range, file_target)) =
                        find_file(&buffer, project.clone(), anchor, cx).await
                    {
                        let snapshot =
                            this.read_with(cx, |editor, cx| editor.buffer.read(cx).snapshot(cx))?;
                        if let Some(range) =
                            snapshot.buffer_anchor_range_to_anchor_range(filename_range)
                        {
                            symbol_range = Some(RangeInEditor::Text(range));
                        }
                        links.push(HoverLink::File(file_target));
                    }

                    // Always also collect LSP definitions so that cmd-click
                    // reveals every applicable target (e.g. a position that
                    // carries both a document link and a definition).
                    if let Some(provider) = provider {
                        let task = cx.update(|_, cx| {
                            provider.definitions(&buffer, anchor, preferred_kind, cx)
                        })?;
                        if let Some(task) = task
                            && let Some(definition_result) = task.await.ok().flatten()
                        {
                            if symbol_range.is_none() {
                                let snapshot = this.read_with(cx, |editor, cx| {
                                    editor.buffer.read(cx).snapshot(cx)
                                })?;
                                symbol_range = definition_result.iter().find_map(|link| {
                                    link.origin.as_ref().and_then(|origin| {
                                        let range = snapshot.buffer_anchor_range_to_anchor_range(
                                            origin.range.clone(),
                                        )?;
                                        Some(RangeInEditor::Text(range))
                                    })
                                });
                            }
                            links.extend(definition_result.into_iter().map(HoverLink::Text));
                        }
                    }

                    if links.is_empty() {
                        None
                    } else {
                        Some((symbol_range, links))
                    }
                }
                TriggerPoint::InlayHint(highlight, lsp_location, server_id) => Some((
                    Some(RangeInEditor::Inlay(highlight.clone())),
                    vec![HoverLink::LspLocation(lsp_location.clone(), *server_id)],
                )),
            };

            this.update(cx, |editor, cx| {
                // Clear any existing highlights
                editor.clear_highlights(HighlightKey::HoveredLinkState, cx);
                let Some(hovered_link_state) = editor.hovered_link_state.as_mut() else {
                    editor.hide_hovered_link(cx);
                    return;
                };
                hovered_link_state.preferred_kind = preferred_kind;
                hovered_link_state.symbol_range = result
                    .as_ref()
                    .and_then(|(symbol_range, _)| symbol_range.clone())
                    .or_else(|| {
                        // Even if we have no click target yet (e.g. an
                        // unresolved document link), record the link's range
                        // so subsequent mouse moves on the same link
                        // short-circuit in `show_link_definition`.
                        detected_document_link
                            .as_ref()
                            .map(|(_, multi_buffer_range, _, _)| {
                                RangeInEditor::Text(multi_buffer_range.clone())
                            })
                    });

                if let Some((symbol_range, definitions)) = result {
                    hovered_link_state.links = definitions;

                    let underline_hovered_link = !hovered_link_state.links.is_empty()
                        || hovered_link_state.symbol_range.is_some();

                    if underline_hovered_link {
                        let style = HighlightStyle {
                            underline: Some(UnderlineStyle {
                                thickness: px(1.),
                                ..UnderlineStyle::default()
                            }),
                            color: Some(cx.theme().colors().link_text_hover),
                            ..HighlightStyle::default()
                        };
                        let highlight_range =
                            symbol_range.unwrap_or_else(|| match &trigger_point {
                                TriggerPoint::Text(trigger_anchor) => {
                                    let snapshot = editor.buffer.read(cx).snapshot(cx);
                                    // If no symbol range returned from language server, use the surrounding word.
                                    let (offset_range, _) =
                                        snapshot.surrounding_word(*trigger_anchor, None);
                                    RangeInEditor::Text(
                                        snapshot.anchor_before(offset_range.start)
                                            ..snapshot.anchor_after(offset_range.end),
                                    )
                                }
                                TriggerPoint::InlayHint(highlight, _, _) => {
                                    RangeInEditor::Inlay(highlight.clone())
                                }
                            });

                        // When the server reports no `originSelectionRange`, fall back
                        // to the highlighted word as the symbol range so that hovering
                        // elsewhere within the same symbol reuses this result instead
                        // of issuing another request.
                        if let Some(hovered_link_state) = editor.hovered_link_state.as_mut()
                            && hovered_link_state.symbol_range.is_none()
                        {
                            hovered_link_state.symbol_range = Some(highlight_range.clone());
                        }

                        match highlight_range {
                            RangeInEditor::Text(text_range) => editor.highlight_text(
                                HighlightKey::HoveredLinkState,
                                vec![text_range],
                                style,
                                cx,
                            ),
                            RangeInEditor::Inlay(highlight) => editor.highlight_inlays(
                                HighlightKey::HoveredLinkState,
                                vec![highlight],
                                style,
                                cx,
                            ),
                        }
                    }
                } else if let Some((_, multi_buffer_range, _, _)) = detected_document_link.as_ref()
                {
                    let style = HighlightStyle {
                        underline: Some(UnderlineStyle {
                            thickness: px(1.),
                            ..UnderlineStyle::default()
                        }),
                        color: Some(cx.theme().colors().link_text_hover),
                        ..HighlightStyle::default()
                    };
                    editor.highlight_text(
                        HighlightKey::HoveredLinkState,
                        vec![multi_buffer_range.clone()],
                        style,
                        cx,
                    );
                } else {
                    // When no links are found, we don't want to completely
                    // throw away the `HoveredLinkState`, we'll want to at least
                    // keep the `trigger_point` around in order to avoid sending
                    // multiple requests for the same point.
                    hovered_link_state.links.clear();
                }
            })?;

            anyhow::Ok(())
        }
        .log_err()
        .await
    }));

    editor.hovered_link_state = Some(hovered_link_state);
}
