use super::*;

pub(super) fn process_completion_for_edit(
    completion: &Completion,
    intent: CompletionIntent,
    buffer: &Entity<Buffer>,
    cursor_position: &text::Anchor,
    cx: &mut Context<Editor>,
) -> CompletionEdit {
    let buffer = buffer.read(cx);
    let buffer_snapshot = buffer.snapshot();
    let (snippet, new_text) = if completion.is_snippet() {
        let mut snippet_source = completion.new_text.clone();
        // Workaround for typescript language server issues so that methods don't expand within
        // strings and functions with type expressions. The previous point is used because the query
        // for function identifier doesn't match when the cursor is immediately after. See PR #30312
        let previous_point = text::ToPoint::to_point(cursor_position, &buffer_snapshot);
        let previous_point = if previous_point.column > 0 {
            cursor_position.to_previous_offset(&buffer_snapshot)
        } else {
            cursor_position.to_offset(&buffer_snapshot)
        };
        if let Some(scope) = buffer_snapshot.language_scope_at(previous_point)
            && scope.prefers_label_for_snippet_in_completion()
            && let Some(label) = completion.label()
            && matches!(
                completion.kind(),
                Some(CompletionItemKind::FUNCTION) | Some(CompletionItemKind::METHOD)
            )
        {
            snippet_source = label;
        }
        match Snippet::parse(&snippet_source).log_err() {
            Some(parsed_snippet) => (Some(parsed_snippet.clone()), parsed_snippet.text),
            None => (None, completion.new_text.clone()),
        }
    } else {
        (None, completion.new_text.clone())
    };

    let mut range_to_replace = {
        let replace_range = &completion.replace_range;
        if let CompletionSource::Lsp {
            insert_range: Some(insert_range),
            ..
        } = &completion.source
        {
            debug_assert_eq!(
                insert_range.start, replace_range.start,
                "insert_range and replace_range should start at the same position"
            );
            debug_assert!(
                insert_range
                    .start
                    .cmp(cursor_position, &buffer_snapshot)
                    .is_le(),
                "insert_range should start before or at cursor position"
            );
            debug_assert!(
                replace_range
                    .start
                    .cmp(cursor_position, &buffer_snapshot)
                    .is_le(),
                "replace_range should start before or at cursor position"
            );

            let should_replace = match intent {
                CompletionIntent::CompleteWithInsert => false,
                CompletionIntent::CompleteWithReplace => true,
                CompletionIntent::Complete | CompletionIntent::Compose => {
                    let insert_mode = LanguageSettings::for_buffer(&buffer, cx)
                        .completions
                        .lsp_insert_mode;
                    match insert_mode {
                        LspInsertMode::Insert => false,
                        LspInsertMode::Replace => true,
                        LspInsertMode::ReplaceSubsequence => {
                            let mut text_to_replace = buffer.chars_for_range(
                                buffer.anchor_before(replace_range.start)
                                    ..buffer.anchor_after(replace_range.end),
                            );
                            let mut current_needle = text_to_replace.next();
                            for haystack_ch in completion.label.text.chars() {
                                if let Some(needle_ch) = current_needle
                                    && haystack_ch.eq_ignore_ascii_case(&needle_ch)
                                {
                                    current_needle = text_to_replace.next();
                                }
                            }
                            current_needle.is_none()
                        }
                        LspInsertMode::ReplaceSuffix => {
                            if replace_range
                                .end
                                .cmp(cursor_position, &buffer_snapshot)
                                .is_gt()
                            {
                                let range_after_cursor = *cursor_position..replace_range.end;
                                let text_after_cursor = buffer
                                    .text_for_range(
                                        buffer.anchor_before(range_after_cursor.start)
                                            ..buffer.anchor_after(range_after_cursor.end),
                                    )
                                    .collect::<String>()
                                    .to_ascii_lowercase();
                                completion
                                    .label
                                    .text
                                    .to_ascii_lowercase()
                                    .ends_with(&text_after_cursor)
                            } else {
                                true
                            }
                        }
                    }
                }
            };

            if should_replace {
                replace_range.clone()
            } else {
                insert_range.clone()
            }
        } else {
            replace_range.clone()
        }
    };

    if range_to_replace
        .end
        .cmp(cursor_position, &buffer_snapshot)
        .is_lt()
    {
        range_to_replace.end = *cursor_position;
    }

    CompletionEdit {
        new_text,
        replace_range: range_to_replace,
        snippet,
    }
}

pub(super) struct CompletionEdit {
    pub(super) new_text: String,
    pub(super) replace_range: Range<text::Anchor>,
    pub(super) snippet: Option<Snippet>,
}
