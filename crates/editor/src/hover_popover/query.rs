use super::*;

pub(super) fn show_hover(
    editor: &mut Editor,
    anchor: Anchor,
    ignore_timeout: bool,
    window: &mut Window,
    cx: &mut Context<Editor>,
) -> Option<()> {
    if editor.pending_rename.is_some() {
        return None;
    }

    let snapshot = editor.snapshot(window, cx);

    let (buffer_position, _) = editor
        .buffer
        .read(cx)
        .snapshot(cx)
        .anchor_to_buffer_anchor(anchor)?;
    let buffer = editor.buffer.read(cx).buffer(buffer_position.buffer_id)?;

    let language_registry = editor
        .project()
        .map(|project| project.read(cx).languages().clone());
    let provider = editor.semantics_provider.clone()?;

    editor.hover_state.hiding_delay_task = None;
    editor.hover_state.closest_mouse_distance = None;

    if !ignore_timeout {
        if same_info_hover(editor, &snapshot, anchor)
            || same_diagnostic_hover(editor, &snapshot, anchor)
            || editor.hover_state.diagnostic_popover.is_some()
        {
            // Hover triggered from same location as last time. Don't show again.
            return None;
        } else {
            hide_hover(editor, cx);
        }
    }

    let hover_popover_delay = EditorSettings::get_global(cx).hover_popover_delay.0;
    let all_diagnostics_active = editor.all_diagnostics_active();
    let active_group_id = editor.active_diagnostic_group_id();

    let renderer = GlobalDiagnosticRenderer::global(cx);
    let task = cx.spawn_in(window, async move |this, cx| {
        async move {
            // If we need to delay, delay a set amount initially before making the lsp request
            let delay = if ignore_timeout {
                None
            } else {
                let lsp_request_early = hover_popover_delay / 2;
                cx.background_executor()
                    .timer(Duration::from_millis(
                        hover_popover_delay - lsp_request_early,
                    ))
                    .await;

                // Construct delay task to wait for later
                let total_delay = Some(
                    cx.background_executor()
                        .timer(Duration::from_millis(lsp_request_early)),
                );
                total_delay
            };

            let hover_request = cx.update(|_, cx| provider.hover(&buffer, buffer_position, cx))?;

            if let Some(delay) = delay {
                delay.await;
            }
            let offset = anchor.to_offset(&snapshot.buffer_snapshot());
            let local_diagnostic = if all_diagnostics_active {
                None
            } else {
                snapshot
                    .buffer_snapshot()
                    .diagnostics_with_buffer_ids_in_range::<MultiBufferOffset>(offset..offset)
                    .filter(|(_, diagnostic)| {
                        Some(diagnostic.diagnostic.group_id) != active_group_id
                    })
                    // Find the entry with the most specific range
                    .min_by_key(|(_, entry)| entry.range.end - entry.range.start)
            };

            let diagnostic_popover = if let Some((buffer_id, local_diagnostic)) = local_diagnostic {
                let group = snapshot
                    .buffer_snapshot()
                    .diagnostic_group(buffer_id, local_diagnostic.diagnostic.group_id)
                    .collect::<Vec<_>>();
                let point_range = local_diagnostic
                    .range
                    .start
                    .to_point(&snapshot.buffer_snapshot())
                    ..local_diagnostic
                        .range
                        .end
                        .to_point(&snapshot.buffer_snapshot());
                let markdown = cx.update(|_, cx| {
                    renderer
                        .as_ref()
                        .and_then(|renderer| {
                            renderer.render_hover(
                                group,
                                point_range,
                                buffer_id,
                                language_registry.clone(),
                                cx,
                            )
                        })
                        .context("no rendered diagnostic")
                })??;

                let (background_color, border_color) = cx.update(|_, cx| {
                    let status_colors = cx.theme().status();
                    match local_diagnostic.diagnostic.severity {
                        DiagnosticSeverity::ERROR => {
                            (status_colors.error_background, status_colors.error_border)
                        }
                        DiagnosticSeverity::WARNING => (
                            status_colors.warning_background,
                            status_colors.warning_border,
                        ),
                        DiagnosticSeverity::INFORMATION => {
                            (status_colors.info_background, status_colors.info_border)
                        }
                        DiagnosticSeverity::HINT => {
                            (status_colors.hint_background, status_colors.hint_border)
                        }
                        _ => (
                            status_colors.ignored_background,
                            status_colors.ignored_border,
                        ),
                    }
                })?;

                let subscription =
                    this.update(cx, |_, cx| cx.observe(&markdown, |_, _, cx| cx.notify()))?;

                let local_diagnostic = DiagnosticEntry {
                    diagnostic: local_diagnostic.diagnostic.to_owned(),
                    range: snapshot
                        .buffer_snapshot()
                        .anchor_before(local_diagnostic.range.start)
                        ..snapshot
                            .buffer_snapshot()
                            .anchor_after(local_diagnostic.range.end),
                };

                let scroll_handle = ScrollHandle::new();

                Some(DiagnosticPopover {
                    local_diagnostic,
                    markdown,
                    border_color,
                    scroll_handle,
                    background_color,
                    keyboard_grace: Rc::new(RefCell::new(ignore_timeout)),
                    anchor,
                    last_bounds: Rc::new(Cell::new(None)),
                    _subscription: subscription,
                })
            } else {
                None
            };

            this.update(cx, |this, _| {
                this.hover_state.diagnostic_popover = diagnostic_popover;
            })?;

            let invisible_char = if let Some(invisible) = snapshot
                .buffer_snapshot()
                .chars_at(anchor)
                .next()
                .filter(|&c| is_invisible(c))
            {
                let after = snapshot.buffer_snapshot().anchor_after(
                    anchor.to_offset(&snapshot.buffer_snapshot()) + invisible.len_utf8(),
                );
                Some((invisible, anchor..after))
            } else if let Some(invisible) = snapshot
                .buffer_snapshot()
                .reversed_chars_at(anchor)
                .next()
                .filter(|&c| is_invisible(c))
            {
                let before = snapshot.buffer_snapshot().anchor_before(
                    anchor.to_offset(&snapshot.buffer_snapshot()) - invisible.len_utf8(),
                );

                Some((invisible, before..anchor))
            } else {
                None
            };

            let hovers_response = if let Some(hover_request) = hover_request {
                hover_request.await.unwrap_or_default()
            } else {
                Vec::new()
            };
            let snapshot = this.update_in(cx, |this, window, cx| this.snapshot(window, cx))?;
            let mut hover_highlights = Vec::with_capacity(hovers_response.len());
            let mut info_popovers = Vec::with_capacity(
                hovers_response.len() + if invisible_char.is_some() { 1 } else { 0 },
            );

            if let Some((invisible, range)) = invisible_char {
                let blocks = vec![HoverBlock {
                    text: format!("Unicode character U+{:02X}", invisible as u32),
                    kind: HoverBlockKind::PlainText,
                }];
                let parsed_content = parse_blocks(&blocks, language_registry.as_ref(), None, cx);
                let scroll_handle = ScrollHandle::new();
                let subscription = this
                    .update(cx, |_, cx| {
                        parsed_content.as_ref().map(|parsed_content| {
                            cx.observe(parsed_content, |_, _, cx| cx.notify())
                        })
                    })
                    .ok()
                    .flatten();
                info_popovers.push(InfoPopover {
                    symbol_range: RangeInEditor::Text(range),
                    parsed_content,
                    scroll_handle,
                    keyboard_grace: Rc::new(RefCell::new(ignore_timeout)),
                    anchor: Some(anchor),
                    last_bounds: Rc::new(Cell::new(None)),
                    _subscription: subscription,
                })
            }

            let doc_link_task = this
                .update(cx, |editor, cx| {
                    editor.document_links_at(buffer.clone(), buffer_position, cx)
                })
                .ok()
                .flatten();
            let doc_link_tooltips = match doc_link_task {
                Some(task) => task
                    .await
                    .into_iter()
                    .filter_map(|(_, link)| {
                        let multi_buffer_range = snapshot
                            .buffer_snapshot()
                            .buffer_anchor_range_to_anchor_range(link.range.clone())?;
                        let tooltip = link.tooltip?;
                        Some((multi_buffer_range, tooltip))
                    })
                    .collect::<Vec<_>>(),
                None => Vec::new(),
            };

            for hover_result in hovers_response {
                // Create symbol range of anchors for highlighting and filtering of future requests.
                let range = hover_result
                    .range
                    .and_then(|range| {
                        let range = snapshot
                            .buffer_snapshot()
                            .buffer_anchor_range_to_anchor_range(range)?;
                        Some(range)
                    })
                    .or_else(|| {
                        let snapshot = &snapshot.buffer_snapshot();
                        let range = snapshot.syntax_ancestor(anchor..anchor)?.1;
                        Some(snapshot.anchor_before(range.start)..snapshot.anchor_after(range.end))
                    })
                    .unwrap_or_else(|| anchor..anchor);

                let blocks = hover_result.contents;
                let language = hover_result.language;
                let parsed_content =
                    parse_blocks(&blocks, language_registry.as_ref(), language, cx);
                let scroll_handle = ScrollHandle::new();
                hover_highlights.push(range.clone());
                let subscription = this
                    .update(cx, |_, cx| {
                        parsed_content.as_ref().map(|parsed_content| {
                            cx.observe(parsed_content, |_, _, cx| cx.notify())
                        })
                    })
                    .ok()
                    .flatten();
                info_popovers.push(InfoPopover {
                    symbol_range: RangeInEditor::Text(range),
                    parsed_content,
                    scroll_handle,
                    keyboard_grace: Rc::new(RefCell::new(ignore_timeout)),
                    anchor: Some(anchor),
                    last_bounds: Rc::new(Cell::new(None)),
                    _subscription: subscription,
                });
            }

            for (multi_buffer_range, tooltip) in doc_link_tooltips {
                let blocks = vec![HoverBlock {
                    text: tooltip.to_string(),
                    kind: HoverBlockKind::Markdown,
                }];
                let parsed_content = parse_blocks(&blocks, language_registry.as_ref(), None, cx);
                let scroll_handle = ScrollHandle::new();
                let subscription = this
                    .update(cx, |_, cx| {
                        parsed_content.as_ref().map(|parsed_content| {
                            cx.observe(parsed_content, |_, _, cx| cx.notify())
                        })
                    })
                    .ok()
                    .flatten();
                info_popovers.push(InfoPopover {
                    symbol_range: RangeInEditor::Text(multi_buffer_range),
                    parsed_content,
                    scroll_handle,
                    keyboard_grace: Rc::new(RefCell::new(ignore_timeout)),
                    anchor: Some(anchor),
                    last_bounds: Rc::new(Cell::new(None)),
                    _subscription: subscription,
                });
            }

            this.update_in(cx, |editor, window, cx| {
                if hover_highlights.is_empty() {
                    editor.clear_background_highlights(HighlightKey::HoverState, cx);
                } else {
                    // Highlight the selected symbol using a background highlight
                    editor.highlight_background(
                        HighlightKey::HoverState,
                        &hover_highlights,
                        |_, theme| theme.colors().element_hover, // todo update theme
                        cx,
                    );
                }

                editor.hover_state.info_popovers = info_popovers;
                cx.notify();
                window.refresh();
            })?;

            anyhow::Ok(())
        }
        .log_err()
        .await
    });

    editor.hover_state.info_task = Some(task);
    None
}

fn same_info_hover(editor: &Editor, snapshot: &EditorSnapshot, anchor: Anchor) -> bool {
    editor
        .hover_state
        .info_popovers
        .iter()
        .any(|InfoPopover { symbol_range, .. }| {
            symbol_range
                .as_text_range()
                .map(|range| {
                    let hover_range = range.to_offset(&snapshot.buffer_snapshot());
                    let offset = anchor.to_offset(&snapshot.buffer_snapshot());
                    // LSP returns a hover result for the end index of ranges that should be hovered, so we need to
                    // use an inclusive range here to check if we should dismiss the popover
                    (hover_range.start..=hover_range.end).contains(&offset)
                })
                .unwrap_or(false)
        })
}

fn same_diagnostic_hover(editor: &Editor, snapshot: &EditorSnapshot, anchor: Anchor) -> bool {
    editor
        .hover_state
        .diagnostic_popover
        .as_ref()
        .map(|diagnostic| {
            let hover_range = diagnostic
                .local_diagnostic
                .range
                .to_offset(&snapshot.buffer_snapshot());
            let offset = anchor.to_offset(&snapshot.buffer_snapshot());

            // Here we do basically the same as in `same_info_hover`, see comment there for an explanation
            (hover_range.start..=hover_range.end).contains(&offset)
        })
        .unwrap_or(false)
}
