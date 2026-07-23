use super::*;

pub(super) struct FeedbackCompletionProvider;

impl FeedbackCompletionProvider {
    pub(super) const FAILURE_MODES: &'static [(&'static str, &'static str)] = &[
        ("@location", "Unexpected location"),
        ("@malformed", "Incomplete, cut off, or syntax error"),
        (
            "@deleted",
            "Deleted code that should be kept (use `@reverted` if it undid a recent edit)",
        ),
        ("@style", "Wrong coding style or conventions"),
        ("@repetitive", "Repeated existing code"),
        ("@hallucinated", "Referenced non-existent symbols"),
        ("@formatting", "Wrong indentation or structure"),
        ("@aggressive", "Changed more than expected"),
        ("@conservative", "Too cautious, changed too little"),
        ("@context", "Ignored or misunderstood context"),
        ("@reverted", "Undid recent edits"),
        ("@cursor_position", "Cursor placed in unhelpful position"),
        ("@whitespace", "Unwanted whitespace or newline changes"),
    ];
}

impl editor::CompletionProvider for FeedbackCompletionProvider {
    fn completions(
        &self,
        buffer: &Entity<Buffer>,
        buffer_position: language::Anchor,
        _trigger: editor::CompletionContext,
        _window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> gpui::Task<anyhow::Result<Vec<CompletionResponse>>> {
        let buffer = buffer.read(cx);
        let mut count_back = 0;

        for char in buffer.reversed_chars_at(buffer_position) {
            if char.is_ascii_alphanumeric() || char == '_' || char == '@' {
                count_back += 1;
            } else {
                break;
            }
        }

        let start_anchor = buffer.anchor_before(
            buffer_position
                .to_offset(&buffer)
                .saturating_sub(count_back),
        );

        let replace_range = start_anchor..buffer_position;
        let snapshot = buffer.text_snapshot();
        let query: String = snapshot.text_for_range(replace_range.clone()).collect();

        if !query.starts_with('@') {
            return gpui::Task::ready(Ok(vec![CompletionResponse {
                completions: vec![],
                display_options: CompletionDisplayOptions {
                    dynamic_width: true,
                },
                is_incomplete: false,
            }]));
        }

        let query_lower = query.to_lowercase();

        let completions: Vec<Completion> = Self::FAILURE_MODES
            .iter()
            .filter(|(key, _description)| key.starts_with(&query_lower))
            .map(|(key, description)| Completion {
                replace_range: replace_range.clone(),
                new_text: format!("{} {}", key, description),
                label: CodeLabel::plain(format!("{}: {}", key, description), None),
                documentation: None,
                source: CompletionSource::Custom,
                icon_path: None,
                icon_color: None,
                match_start: None,
                snippet_deduplication_key: None,
                insert_text_mode: None,
                confirm: None,
                group: None,
            })
            .collect();

        gpui::Task::ready(Ok(vec![CompletionResponse {
            completions,
            display_options: CompletionDisplayOptions {
                dynamic_width: true,
            },
            is_incomplete: false,
        }]))
    }

    fn is_completion_trigger(
        &self,
        _buffer: &Entity<Buffer>,
        _position: language::Anchor,
        text: &str,
        _trigger_in_words: bool,
        _cx: &mut Context<Editor>,
    ) -> bool {
        text.chars()
            .last()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '@')
    }
}
