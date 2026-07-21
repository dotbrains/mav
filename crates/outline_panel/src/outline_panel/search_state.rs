use super::*;

#[derive(Debug)]
pub(super) enum ItemsDisplayMode {
    Search(SearchState),
    Outline,
}

#[derive(Debug)]
pub(super) struct SearchState {
    pub(super) kind: SearchKind,
    pub(super) query: String,
    pub(super) matches: Vec<(Range<editor::Anchor>, Arc<OnceLock<SearchData>>)>,
    pub(super) highlight_search_match_tx: async_channel::Sender<HighlightArguments>,
    pub(super) _search_match_highlighter: Task<()>,
    pub(super) _search_match_notify: Task<()>,
}

pub(super) struct HighlightArguments {
    pub(super) multi_buffer_snapshot: MultiBufferSnapshot,
    pub(super) match_range: Range<editor::Anchor>,
    pub(super) search_data: Arc<OnceLock<SearchData>>,
}

impl SearchState {
    pub(super) fn new(
        kind: SearchKind,
        query: String,
        previous_matches: HashMap<Range<editor::Anchor>, Arc<OnceLock<SearchData>>>,
        new_matches: Vec<Range<editor::Anchor>>,
        theme: Arc<SyntaxTheme>,
        window: &mut Window,
        cx: &mut Context<OutlinePanel>,
    ) -> Self {
        let (highlight_search_match_tx, highlight_search_match_rx) = async_channel::unbounded();
        let (notify_tx, notify_rx) = async_channel::unbounded::<()>();
        Self {
            kind,
            query,
            matches: new_matches
                .into_iter()
                .map(|range| {
                    let search_data = previous_matches
                        .get(&range)
                        .map(Arc::clone)
                        .unwrap_or_default();
                    (range, search_data)
                })
                .collect(),
            highlight_search_match_tx,
            _search_match_highlighter: cx.background_spawn(async move {
                while let Ok(highlight_arguments) = highlight_search_match_rx.recv().await {
                    let needs_init = highlight_arguments.search_data.get().is_none();
                    let search_data = highlight_arguments.search_data.get_or_init(|| {
                        SearchData::new(
                            &highlight_arguments.match_range,
                            &highlight_arguments.multi_buffer_snapshot,
                        )
                    });
                    if needs_init {
                        notify_tx.try_send(()).ok();
                    }

                    let highlight_data = &search_data.highlights_data;
                    if highlight_data.get().is_some() {
                        continue;
                    }
                    let mut left_whitespaces_count = 0;
                    let mut non_whitespace_symbol_occurred = false;
                    let context_offset_range = search_data
                        .context_range
                        .to_offset(&highlight_arguments.multi_buffer_snapshot);
                    let mut offset = context_offset_range.start;
                    let mut context_text = String::new();
                    let mut highlight_ranges = Vec::new();
                    for mut chunk in highlight_arguments.multi_buffer_snapshot.chunks(
                        context_offset_range.start..context_offset_range.end,
                        LanguageAwareStyling {
                            tree_sitter: true,
                            diagnostics: true,
                        },
                    ) {
                        if !non_whitespace_symbol_occurred {
                            for c in chunk.text.chars() {
                                if c.is_whitespace() {
                                    left_whitespaces_count += c.len_utf8();
                                } else {
                                    non_whitespace_symbol_occurred = true;
                                    break;
                                }
                            }
                        }

                        if chunk.text.len() > context_offset_range.end - offset {
                            chunk.text = &chunk.text[0..(context_offset_range.end - offset)];
                            offset = context_offset_range.end;
                        } else {
                            offset += chunk.text.len();
                        }
                        let style = chunk
                            .syntax_highlight_id
                            .and_then(|highlight| theme.get(highlight).cloned());

                        if let Some(style) = style {
                            let start = context_text.len();
                            let end = start + chunk.text.len();
                            highlight_ranges.push((start..end, style));
                        }
                        context_text.push_str(chunk.text);
                        if offset >= context_offset_range.end {
                            break;
                        }
                    }

                    highlight_ranges.iter_mut().for_each(|(range, _)| {
                        range.start = range.start.saturating_sub(left_whitespaces_count);
                        range.end = range.end.saturating_sub(left_whitespaces_count);
                    });
                    if highlight_data.set(highlight_ranges).ok().is_some() {
                        notify_tx.try_send(()).ok();
                    }

                    let trimmed_text = context_text[left_whitespaces_count..].to_owned();
                    debug_assert_eq!(
                        trimmed_text, search_data.context_text,
                        "Highlighted text that does not match the buffer text"
                    );
                }
            }),
            _search_match_notify: cx.spawn_in(window, async move |outline_panel, cx| {
                loop {
                    match notify_rx.recv().await {
                        Ok(()) => {}
                        Err(_) => break,
                    };
                    while let Ok(()) = notify_rx.try_recv() {
                        //
                    }
                    let update_result = outline_panel.update(cx, |_, cx| {
                        cx.notify();
                    });
                    if update_result.is_err() {
                        break;
                    }
                }
            }),
        }
    }
}

pub(super) const SEARCH_MATCH_CONTEXT_SIZE: u32 = 40;
pub(super) const TRUNCATED_CONTEXT_MARK: &str = "…";

impl SearchData {
    pub(super) fn new(
        match_range: &Range<editor::Anchor>,
        multi_buffer_snapshot: &MultiBufferSnapshot,
    ) -> Self {
        let match_point_range = match_range.to_point(multi_buffer_snapshot);
        let context_left_border = multi_buffer_snapshot.clip_point(
            language::Point::new(
                match_point_range.start.row,
                match_point_range
                    .start
                    .column
                    .saturating_sub(SEARCH_MATCH_CONTEXT_SIZE),
            ),
            Bias::Left,
        );
        let context_right_border = multi_buffer_snapshot.clip_point(
            language::Point::new(
                match_point_range.end.row,
                match_point_range.end.column + SEARCH_MATCH_CONTEXT_SIZE,
            ),
            Bias::Right,
        );

        let context_anchor_range =
            (context_left_border..context_right_border).to_anchors(multi_buffer_snapshot);
        let context_offset_range = context_anchor_range.to_offset(multi_buffer_snapshot);
        let match_offset_range = match_range.to_offset(multi_buffer_snapshot);

        let mut search_match_indices = vec![
            match_offset_range.start - context_offset_range.start
                ..match_offset_range.end - context_offset_range.start,
        ];

        let entire_context_text = multi_buffer_snapshot
            .text_for_range(context_offset_range.clone())
            .collect::<String>();
        let left_whitespaces_offset = entire_context_text
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(|c| c.len_utf8())
            .sum::<usize>();

        let mut extended_context_left_border = context_left_border;
        extended_context_left_border.column = extended_context_left_border.column.saturating_sub(1);
        let extended_context_left_border =
            multi_buffer_snapshot.clip_point(extended_context_left_border, Bias::Left);
        let mut extended_context_right_border = context_right_border;
        extended_context_right_border.column += 1;
        let extended_context_right_border =
            multi_buffer_snapshot.clip_point(extended_context_right_border, Bias::Right);

        let truncated_left = left_whitespaces_offset == 0
            && extended_context_left_border < context_left_border
            && multi_buffer_snapshot
                .chars_at(extended_context_left_border)
                .last()
                .is_some_and(|c| !c.is_whitespace());
        let truncated_right = entire_context_text
            .chars()
            .last()
            .is_none_or(|c| !c.is_whitespace())
            && extended_context_right_border > context_right_border
            && multi_buffer_snapshot
                .chars_at(extended_context_right_border)
                .next()
                .is_some_and(|c| !c.is_whitespace());
        search_match_indices.iter_mut().for_each(|range| {
            range.start = range.start.saturating_sub(left_whitespaces_offset);
            range.end = range.end.saturating_sub(left_whitespaces_offset);
        });

        let trimmed_row_offset_range =
            context_offset_range.start + left_whitespaces_offset..context_offset_range.end;
        let trimmed_text = entire_context_text[left_whitespaces_offset..].to_owned();
        Self {
            highlights_data: Arc::default(),
            search_match_indices,
            context_range: trimmed_row_offset_range.to_anchors(multi_buffer_snapshot),
            context_text: trimmed_text,
            truncated_left,
            truncated_right,
        }
    }
}
