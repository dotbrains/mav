use super::*;

impl Editor {
    pub(crate) fn refresh_document_highlights(&mut self, cx: &mut Context<Self>) -> Option<()> {
        if self.pending_rename.is_some() {
            return None;
        }

        let provider = self.semantics_provider.clone()?;
        let buffer = self.buffer.read(cx);
        let newest_selection = self.selections.newest_anchor().clone();
        let cursor_position = newest_selection.head();
        let (cursor_buffer, cursor_buffer_position) =
            buffer.text_anchor_for_position(cursor_position, cx)?;
        let (tail_buffer, tail_buffer_position) =
            buffer.text_anchor_for_position(newest_selection.tail(), cx)?;
        if cursor_buffer != tail_buffer {
            return None;
        }

        let snapshot = cursor_buffer.read(cx).snapshot();
        let word_ranges = cx.background_spawn(async move {
            let (start_word_range, _) = snapshot.surrounding_word(cursor_buffer_position, None);
            let (end_word_range, _) = snapshot.surrounding_word(tail_buffer_position, None);
            (start_word_range, end_word_range)
        });

        let debounce = EditorSettings::get_global(cx).lsp_highlight_debounce.0;
        self.document_highlights_task = Some(cx.spawn(async move |this, cx| {
            let (start_word_range, end_word_range) = word_ranges.await;
            if start_word_range != end_word_range {
                this.update(cx, |this, cx| {
                    this.document_highlights_task.take();
                    this.clear_background_highlights(HighlightKey::DocumentHighlightRead, cx);
                    this.clear_background_highlights(HighlightKey::DocumentHighlightWrite, cx);
                })
                .ok();
                return;
            }
            cx.background_executor()
                .timer(Duration::from_millis(debounce))
                .await;

            let highlights = if let Some(highlights) = cx.update(|cx| {
                provider.document_highlights(&cursor_buffer, cursor_buffer_position, cx)
            }) {
                highlights.await.log_err()
            } else {
                None
            };

            if let Some(highlights) = highlights {
                this.update(cx, |this, cx| {
                    if this.pending_rename.is_some() {
                        return;
                    }

                    let buffer = this.buffer.read(cx);
                    if buffer
                        .text_anchor_for_position(cursor_position, cx)
                        .is_none_or(|(buffer, _)| buffer != cursor_buffer)
                    {
                        return;
                    }

                    let mut write_ranges = Vec::new();
                    let mut read_ranges = Vec::new();
                    let multibuffer_snapshot = buffer.snapshot(cx);
                    for highlight in highlights {
                        for range in
                            multibuffer_snapshot.buffer_range_to_excerpt_ranges(highlight.range)
                        {
                            if highlight.kind == lsp::DocumentHighlightKind::WRITE {
                                write_ranges.push(range);
                            } else {
                                read_ranges.push(range);
                            }
                        }
                    }

                    this.highlight_background(
                        HighlightKey::DocumentHighlightRead,
                        &read_ranges,
                        |_, theme| theme.colors().editor_document_highlight_read_background,
                        cx,
                    );
                    this.highlight_background(
                        HighlightKey::DocumentHighlightWrite,
                        &write_ranges,
                        |_, theme| theme.colors().editor_document_highlight_write_background,
                        cx,
                    );
                    cx.notify();
                })
                .log_err();
            }
        }));
        None
    }

    pub(crate) fn prepare_highlight_query_from_selection(
        &mut self,
        snapshot: &DisplaySnapshot,
        cx: &mut Context<Editor>,
    ) -> Option<(String, Range<Anchor>)> {
        if matches!(self.mode, EditorMode::SingleLine) {
            return None;
        }
        if !self.use_selection_highlight || !EditorSettings::get_global(cx).selection_highlight {
            return None;
        }
        if self.last_selection_from_search
            && self.has_background_highlights(HighlightKey::BufferSearchHighlights)
        {
            return None;
        }
        if self.selections.count() != 1 || self.selections.line_mode() {
            return None;
        }
        let selection = self.selections.newest::<Point>(&snapshot);
        if selection.start.row != selection.end.row
            || selection.start.column == selection.end.column
        {
            return None;
        }
        let selection_anchor_range = selection.range().to_anchors(snapshot.buffer_snapshot());
        let query = snapshot
            .buffer_snapshot()
            .text_for_range(selection_anchor_range.clone())
            .collect::<String>();
        if query.trim().is_empty() {
            return None;
        }
        Some((query, selection_anchor_range))
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn update_selection_occurrence_highlights(
        &mut self,
        multi_buffer_snapshot: MultiBufferSnapshot,
        query_text: String,
        query_range: Range<Anchor>,
        multi_buffer_range_to_query: Range<Point>,
        use_debounce: bool,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Task<()> {
        cx.spawn_in(window, async move |editor, cx| {
            if use_debounce {
                cx.background_executor()
                    .timer(SELECTION_HIGHLIGHT_DEBOUNCE_TIMEOUT)
                    .await;
            }
            let match_task = cx.background_spawn(async move {
                let buffer_ranges = multi_buffer_snapshot
                    .range_to_buffer_ranges(
                        multi_buffer_range_to_query.start..multi_buffer_range_to_query.end,
                    )
                    .into_iter()
                    .filter(|(_, excerpt_visible_range, _)| !excerpt_visible_range.is_empty());
                let mut match_ranges = Vec::new();
                let Ok(regex) = project::search::SearchQuery::text(
                    query_text,
                    false,
                    false,
                    false,
                    Default::default(),
                    Default::default(),
                    false,
                    None,
                ) else {
                    return Vec::default();
                };
                let query_range = query_range.to_anchors(&multi_buffer_snapshot);
                for (buffer_snapshot, search_range, _) in buffer_ranges {
                    match_ranges.extend(
                        regex
                            .search(
                                &buffer_snapshot,
                                Some(search_range.start.0..search_range.end.0),
                            )
                            .await
                            .into_iter()
                            .filter_map(|match_range| {
                                let match_start = buffer_snapshot
                                    .anchor_after(search_range.start + match_range.start);
                                let match_end = buffer_snapshot
                                    .anchor_before(search_range.start + match_range.end);
                                let range = multi_buffer_snapshot.anchor_in_buffer(match_start)?
                                    ..multi_buffer_snapshot.anchor_in_buffer(match_end)?;
                                Some(range)
                                    .filter(|match_anchor_range| match_anchor_range != &query_range)
                            }),
                    );
                }
                match_ranges
            });
            let match_ranges = match_task.await;
            editor
                .update_in(cx, |editor, _, cx| {
                    if use_debounce {
                        editor.clear_background_highlights(HighlightKey::SelectedTextHighlight, cx);
                        editor.debounced_selection_highlight_complete = true;
                    } else if editor.debounced_selection_highlight_complete {
                        return;
                    }
                    if !match_ranges.is_empty() {
                        editor.highlight_background(
                            HighlightKey::SelectedTextHighlight,
                            &match_ranges,
                            |_, theme| theme.colors().editor_document_highlight_bracket_background,
                            cx,
                        )
                    }
                })
                .log_err();
        })
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn refresh_selected_text_highlights(
        &mut self,
        snapshot: &DisplaySnapshot,
        on_buffer_edit: bool,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let Some((query_text, query_range)) =
            self.prepare_highlight_query_from_selection(snapshot, cx)
        else {
            self.clear_background_highlights(HighlightKey::SelectedTextHighlight, cx);
            self.quick_selection_highlight_task.take();
            self.debounced_selection_highlight_task.take();
            self.debounced_selection_highlight_complete = false;
            return;
        };
        let display_snapshot = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let multi_buffer_snapshot = self.buffer().read(cx).snapshot(cx);
        let query_changed = self
            .quick_selection_highlight_task
            .as_ref()
            .is_none_or(|(prev_anchor_range, _)| prev_anchor_range != &query_range);
        if query_changed {
            self.debounced_selection_highlight_complete = false;
        }
        if on_buffer_edit || query_changed {
            self.quick_selection_highlight_task = Some((
                query_range.clone(),
                self.update_selection_occurrence_highlights(
                    snapshot.buffer.clone(),
                    query_text.clone(),
                    query_range.clone(),
                    self.multi_buffer_visible_range(&display_snapshot, cx),
                    false,
                    window,
                    cx,
                ),
            ));
        }
        if on_buffer_edit
            || self
                .debounced_selection_highlight_task
                .as_ref()
                .is_none_or(|(prev_anchor_range, _)| prev_anchor_range != &query_range)
        {
            let multi_buffer_start = multi_buffer_snapshot
                .anchor_before(MultiBufferOffset(0))
                .to_point(&multi_buffer_snapshot);
            let multi_buffer_end = multi_buffer_snapshot
                .anchor_after(multi_buffer_snapshot.len())
                .to_point(&multi_buffer_snapshot);
            let multi_buffer_full_range = multi_buffer_start..multi_buffer_end;
            self.debounced_selection_highlight_task = Some((
                query_range.clone(),
                self.update_selection_occurrence_highlights(
                    snapshot.buffer.clone(),
                    query_text,
                    query_range,
                    multi_buffer_full_range,
                    true,
                    window,
                    cx,
                ),
            ));
        }
    }
}
