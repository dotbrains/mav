use super::*;

pub(super) fn terminal_view_selection(
    terminal_view: &Entity<TerminalView>,
    cx: &App,
) -> Option<String> {
    terminal_view
        .read(cx)
        .terminal()
        .read(cx)
        .last_content
        .selection_text
        .clone()
        .filter(|text| !text.is_empty())
}

pub(super) fn editor_selection_ranges(
    editor: &Entity<Editor>,
    include_current_line: bool,
    cx: &mut App,
) -> Vec<(Entity<Buffer>, Range<text::Anchor>)> {
    editor.update(cx, |editor, cx| {
        let selections = editor.selections.all_adjusted(&editor.display_snapshot(cx));

        let multi_buffer = editor.buffer().read(cx);
        let multi_buffer_snapshot = multi_buffer.snapshot(cx);

        let non_empty_rows: collections::HashSet<u32> = selections
            .iter()
            .filter(|s| !s.is_empty())
            .flat_map(|s| s.start.row..=s.end.row)
            .collect();

        let mut seen_current_line_rows = collections::HashSet::default();
        let mut results = Vec::new();

        for s in selections {
            if s.is_empty() {
                if !include_current_line
                    || non_empty_rows.contains(&s.start.row)
                    || !seen_current_line_rows.insert(s.start.row)
                {
                    continue;
                }
                let Some((buffer, anchor)) = multi_buffer.text_anchor_for_position(s.start, cx)
                else {
                    continue;
                };
                let buffer_snapshot = buffer.read(cx).snapshot();
                let row = anchor.to_point(&buffer_snapshot).row;
                let line_start = text::Point::new(row, 0);
                let line_end = text::Point::new(row, buffer_snapshot.line_len(row));
                let start = buffer_snapshot.anchor_after(line_start);
                let end = buffer_snapshot.anchor_before(line_end);
                if start.to_offset(&buffer_snapshot) == end.to_offset(&buffer_snapshot) {
                    continue;
                }
                results.push((buffer, start..end));
            } else {
                let mb_start = multi_buffer_snapshot.anchor_after(s.start);
                let mb_end = multi_buffer_snapshot.anchor_before(s.end);
                let Some((start_buffer, start)) =
                    multi_buffer.text_anchor_for_position(mb_start, cx)
                else {
                    continue;
                };
                let Some((end_buffer, end)) = multi_buffer.text_anchor_for_position(mb_end, cx)
                else {
                    continue;
                };
                if start_buffer != end_buffer {
                    continue;
                }
                let buffer_snapshot = start_buffer.read(cx).snapshot();
                if start.to_offset(&buffer_snapshot) == end.to_offset(&buffer_snapshot) {
                    continue;
                }
                results.push((start_buffer, start..end));
            }
        }

        results
    })
}

pub(super) type ConfirmCallback =
    Arc<dyn Fn(CompletionIntent, &mut Window, &mut App) -> bool + Send + Sync>;

pub(super) fn completion_text_for_editor_selections(
    source_range: Range<Anchor>,
    editor: WeakEntity<Editor>,
    mention_set: WeakEntity<MentionSet>,
    editor_selections: Vec<(Entity<Buffer>, Range<text::Anchor>)>,
) -> (String, ConfirmCallback) {
    const EDITOR_PLACEHOLDER: &str = "selection ";

    let selections = editor_selections
        .into_iter()
        .enumerate()
        .map(|(ix, (buffer, range))| {
            (
                buffer,
                range,
                (EDITOR_PLACEHOLDER.len() * ix)..(EDITOR_PLACEHOLDER.len() * (ix + 1) - 1),
            )
        })
        .collect::<Vec<_>>();

    let new_text = EDITOR_PLACEHOLDER.repeat(selections.len());

    let callback: ConfirmCallback = Arc::new({
        move |_: CompletionIntent, window: &mut Window, cx: &mut App| {
            let editor = editor.clone();
            let selections = selections.clone();
            let mention_set = mention_set.clone();
            let source_range = source_range.clone();
            window.defer(cx, move |window, cx| {
                if let Some(editor) = editor.upgrade()
                    && !selections.is_empty()
                {
                    mention_set
                        .update(cx, |store, cx| {
                            store.confirm_mention_for_selection(
                                source_range.clone(),
                                selections,
                                editor.clone(),
                                window,
                                cx,
                            )
                        })
                        .ok();
                }
            });
            false
        }
    });

    (new_text, callback)
}

pub(super) fn completion_text_for_terminal_selections(
    source_range: Range<Anchor>,
    editor: WeakEntity<Editor>,
    mention_set: WeakEntity<MentionSet>,
    terminal_selections: Vec<String>,
) -> (String, ConfirmCallback) {
    const TERMINAL_PLACEHOLDER: &str = "terminal ";

    let mut new_text = String::new();
    let terminal_ranges: Vec<(String, std::ops::Range<usize>)> = terminal_selections
        .into_iter()
        .map(|text| {
            let start = new_text.len();
            new_text.push_str(TERMINAL_PLACEHOLDER);
            (text, start..(new_text.len() - 1))
        })
        .collect();

    let callback: ConfirmCallback = Arc::new({
        move |_: CompletionIntent, window: &mut Window, cx: &mut App| {
            let editor = editor.clone();
            let mention_set = mention_set.clone();
            let source_range = source_range.clone();
            let terminal_ranges = terminal_ranges.clone();
            window.defer(cx, move |window, cx| {
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                for (terminal_text, terminal_range) in terminal_ranges {
                    let snapshot = editor.read(cx).buffer().read(cx).snapshot(cx);
                    let Some(start) = snapshot.anchor_in_excerpt(source_range.start) else {
                        return;
                    };
                    let offset = start.to_offset(&snapshot);

                    let line_count = terminal_text.lines().count() as u32;
                    let mention_uri = MentionUri::TerminalSelection { line_count };
                    let range = snapshot.anchor_after(offset + terminal_range.start)
                        ..snapshot.anchor_after(offset + terminal_range.end);

                    let crease = crate::mention_set::crease_for_mention(
                        mention_uri.name().into(),
                        mention_uri.icon_path(cx),
                        None,
                        range,
                        editor.downgrade(),
                    );

                    let Some(crease_id) = editor.update(cx, |editor, cx| {
                        let crease_ids = editor.insert_creases(vec![crease.clone()], cx);
                        editor.fold_creases(vec![crease], false, window, cx);
                        crease_ids.first().copied()
                    }) else {
                        log::error!("insert_creases returned no ids for terminal selection");
                        continue;
                    };

                    mention_set
                        .update(cx, |mention_set, cx| {
                            mention_set.insert_mention(
                                crease_id,
                                mention_uri.clone(),
                                Task::ready(Ok(crate::mention_set::Mention::Text {
                                    content: terminal_text,
                                    tracked_buffers: vec![],
                                }))
                                .shared(),
                                None,
                                cx,
                            );
                        })
                        .ok();
                }
            });
            false
        }
    });

    (new_text, callback)
}
