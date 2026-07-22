use super::*;
// When not streaming tool calls, we strip backticks as part of parsing the model's
// plain text response. This is a regression test for a bug where we stripped
// backticks incorrectly.
#[gpui::test]
async fn test_allows_model_to_output_backticks(cx: &mut TestAppContext) {
    init_test(cx);
    let text = "- Improved; `cmd+click` behavior. Now requires `cmd` to be pressed before the click starts or it doesn't run. ([#44579](https://github.com/mav-industries/mav/pull/44579); thanks [Zachiah](https://github.com/Zachiah))";
    let buffer = cx.new(|cx| Buffer::local("", cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let range = buffer.read_with(cx, |buffer, cx| {
        let snapshot = buffer.snapshot(cx);
        snapshot.anchor_before(Point::new(0, 0))..snapshot.anchor_after(Point::new(0, 0))
    });
    let prompt_builder = Arc::new(PromptBuilder::new(None).unwrap());
    let codegen = cx.new(|cx| {
        CodegenAlternative::new(
            buffer.clone(),
            range.clone(),
            true,
            prompt_builder,
            Uuid::new_v4(),
            cx,
        )
    });

    let events_tx = simulate_tool_based_completion(&codegen, cx);
    let chunk_len = text.find('`').unwrap();
    events_tx
        .unbounded_send(rewrite_tool_use("tool_1", &text[..chunk_len], false))
        .unwrap();
    events_tx
        .unbounded_send(rewrite_tool_use("tool_1", &text, true))
        .unwrap();
    events_tx
        .unbounded_send(LanguageModelCompletionEvent::Stop(StopReason::EndTurn))
        .unwrap();
    drop(events_tx);
    cx.run_until_parked();

    assert_eq!(
        buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx).text()),
        text
    );
}

// Regression test: a second rewrite tool use with a *shorter* replacement_text
// than the first would cause an index-out-of-bounds panic because the
// chars_read_so_far counter was shared across all tool use IDs.
#[gpui::test]
async fn test_separate_tool_uses_have_independent_char_counters(cx: &mut TestAppContext) {
    init_test(cx);
    let buffer = cx.new(|cx| Buffer::local("", cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let range = buffer.read_with(cx, |buffer, cx| {
        let snapshot = buffer.snapshot(cx);
        snapshot.anchor_before(Point::new(0, 0))..snapshot.anchor_after(Point::new(0, 0))
    });
    let prompt_builder = Arc::new(PromptBuilder::new(None).unwrap());
    let codegen = cx.new(|cx| {
        CodegenAlternative::new(
            buffer.clone(),
            range.clone(),
            true,
            prompt_builder,
            Uuid::new_v4(),
            cx,
        )
    });

    let events_tx = simulate_tool_based_completion(&codegen, cx);
    // tool_1 has longer text; tool_2 has shorter text. With the old shared
    // counter, processing tool_2 would attempt replacement_text[N..] where
    // N > replacement_text.len(), panicking with index out of bounds.
    events_tx
        .unbounded_send(rewrite_tool_use("tool_1", "longer replacement text", true))
        .unwrap();
    events_tx
        .unbounded_send(rewrite_tool_use("tool_2", "short", true))
        .unwrap();
    events_tx
        .unbounded_send(LanguageModelCompletionEvent::Stop(StopReason::EndTurn))
        .unwrap();
    drop(events_tx);
    cx.run_until_parked();

    assert_eq!(
        buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx).text()),
        "longer replacement textshort"
    );
}

#[gpui::test]
async fn test_strip_invalid_spans_from_codeblock() {
    assert_chunks("Lorem ipsum dolor", "Lorem ipsum dolor").await;
    assert_chunks("```\nLorem ipsum dolor", "Lorem ipsum dolor").await;
    assert_chunks("```\nLorem ipsum dolor\n```", "Lorem ipsum dolor").await;
    assert_chunks(
        "```html\n```js\nLorem ipsum dolor\n```\n```",
        "```js\nLorem ipsum dolor\n```",
    )
    .await;
    assert_chunks("``\nLorem ipsum dolor\n```", "``\nLorem ipsum dolor\n```").await;
    assert_chunks("Lorem<|CURSOR|> ipsum", "Lorem ipsum").await;
    assert_chunks("Lorem ipsum", "Lorem ipsum").await;
    assert_chunks("```\n<|CURSOR|>Lorem ipsum\n```", "Lorem ipsum").await;

    async fn assert_chunks(text: &str, expected_text: &str) {
        for chunk_size in 1..=text.len() {
            let actual_text = StripInvalidSpans::new(chunks(text, chunk_size))
                .map(|chunk| chunk.unwrap())
                .collect::<String>()
                .await;
            assert_eq!(
                actual_text, expected_text,
                "failed to strip invalid spans, chunk size: {}",
                chunk_size
            );
        }
    }

    fn chunks(text: &str, size: usize) -> impl Stream<Item = Result<String>> {
        stream::iter(
            text.chars()
                .collect::<Vec<_>>()
                .chunks(size)
                .map(|chunk| Ok(chunk.iter().collect::<String>()))
                .collect::<Vec<_>>(),
        )
    }
}

fn init_test(cx: &mut TestAppContext) {
    cx.update(LanguageModelRegistry::test);
    cx.set_global(cx.update(SettingsStore::test));
}

fn simulate_response_stream(
    codegen: &Entity<CodegenAlternative>,
    cx: &mut TestAppContext,
) -> mpsc::UnboundedSender<String> {
    let (chunks_tx, chunks_rx) = mpsc::unbounded();
    let model = Arc::new(FakeLanguageModel::default());
    codegen.update(cx, |codegen, cx| {
        codegen.generation = codegen.handle_stream(
            model,
            /* strip_invalid_spans: */ false,
            future::ready(Ok(LanguageModelTextStream {
                message_id: None,
                stream: chunks_rx.map(Ok).boxed(),
                last_token_usage: Arc::new(Mutex::new(TokenUsage::default())),
            })),
            cx,
        );
    });
    chunks_tx
}

fn simulate_tool_based_completion(
    codegen: &Entity<CodegenAlternative>,
    cx: &mut TestAppContext,
) -> mpsc::UnboundedSender<LanguageModelCompletionEvent> {
    let (events_tx, events_rx) = mpsc::unbounded();
    let model = Arc::new(FakeLanguageModel::default());
    codegen.update(cx, |codegen, cx| {
        let completion_stream = Task::ready(Ok(events_rx.map(Ok).boxed()
            as BoxStream<
                'static,
                Result<LanguageModelCompletionEvent, LanguageModelCompletionError>,
            >));
        codegen.generation = codegen.handle_completion(model, completion_stream, cx);
    });
    events_tx
}

fn rewrite_tool_use(
    id: &str,
    replacement_text: &str,
    is_complete: bool,
) -> LanguageModelCompletionEvent {
    let input = RewriteSectionInput {
        replacement_text: replacement_text.into(),
    };
    LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse {
        id: id.into(),
        name: REWRITE_SECTION_TOOL_NAME.into(),
        raw_input: serde_json::to_string(&input).unwrap(),
        input: serde_json::to_value(&input).unwrap(),
        is_input_complete: is_complete,
        thought_signature: None,
    })
}
