use super::*;
use super::*;
use futures::{
    Stream,
    stream::{self},
};
use gpui::TestAppContext;
use indoc::indoc;
use language::{Buffer, Point};
use language_model::fake_provider::FakeLanguageModel;
use language_model::{
    LanguageModelCompletionError, LanguageModelCompletionEvent, LanguageModelRegistry,
    LanguageModelToolUse, StopReason, TokenUsage,
};
use languages::rust_lang;
use rand::prelude::*;
use settings::SettingsStore;
use std::{future, sync::Arc};

#[gpui::test(iterations = 10)]
async fn test_transform_autoindent(cx: &mut TestAppContext, mut rng: StdRng) {
    init_test(cx);

    let text = indoc! {"
            fn main() {
                let x = 0;
                for _ in 0..10 {
                    x += 1;
                }
            }
        "};
    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(rust_lang(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let range = buffer.read_with(cx, |buffer, cx| {
        let snapshot = buffer.snapshot(cx);
        snapshot.anchor_before(Point::new(1, 0))..snapshot.anchor_after(Point::new(4, 5))
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

    let chunks_tx = simulate_response_stream(&codegen, cx);

    let mut new_text = concat!(
        "       let mut x = 0;\n",
        "       while x < 10 {\n",
        "           x += 1;\n",
        "       }",
    );
    while !new_text.is_empty() {
        let max_len = cmp::min(new_text.len(), 10);
        let len = rng.random_range(1..=max_len);
        let (chunk, suffix) = new_text.split_at(len);
        chunks_tx.unbounded_send(chunk.to_string()).unwrap();
        new_text = suffix;
        cx.background_executor.run_until_parked();
    }
    drop(chunks_tx);
    cx.background_executor.run_until_parked();

    assert_eq!(
        buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx).text()),
        indoc! {"
                fn main() {
                    let mut x = 0;
                    while x < 10 {
                        x += 1;
                    }
                }
            "}
    );
}

#[gpui::test(iterations = 10)]
async fn test_autoindent_when_generating_past_indentation(
    cx: &mut TestAppContext,
    mut rng: StdRng,
) {
    init_test(cx);

    let text = indoc! {"
            fn main() {
                le
            }
        "};
    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(rust_lang(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let range = buffer.read_with(cx, |buffer, cx| {
        let snapshot = buffer.snapshot(cx);
        snapshot.anchor_before(Point::new(1, 6))..snapshot.anchor_after(Point::new(1, 6))
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

    let chunks_tx = simulate_response_stream(&codegen, cx);

    cx.background_executor.run_until_parked();

    let mut new_text = concat!(
        "t mut x = 0;\n",
        "while x < 10 {\n",
        "    x += 1;\n",
        "}", //
    );
    while !new_text.is_empty() {
        let max_len = cmp::min(new_text.len(), 10);
        let len = rng.random_range(1..=max_len);
        let (chunk, suffix) = new_text.split_at(len);
        chunks_tx.unbounded_send(chunk.to_string()).unwrap();
        new_text = suffix;
        cx.background_executor.run_until_parked();
    }
    drop(chunks_tx);
    cx.background_executor.run_until_parked();

    assert_eq!(
        buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx).text()),
        indoc! {"
                fn main() {
                    let mut x = 0;
                    while x < 10 {
                        x += 1;
                    }
                }
            "}
    );
}

#[gpui::test(iterations = 10)]
async fn test_autoindent_when_generating_before_indentation(
    cx: &mut TestAppContext,
    mut rng: StdRng,
) {
    init_test(cx);

    let text = concat!(
        "fn main() {\n",
        "  \n",
        "}\n" //
    );
    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(rust_lang(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let range = buffer.read_with(cx, |buffer, cx| {
        let snapshot = buffer.snapshot(cx);
        snapshot.anchor_before(Point::new(1, 2))..snapshot.anchor_after(Point::new(1, 2))
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

    let chunks_tx = simulate_response_stream(&codegen, cx);

    cx.background_executor.run_until_parked();

    let mut new_text = concat!(
        "let mut x = 0;\n",
        "while x < 10 {\n",
        "    x += 1;\n",
        "}", //
    );
    while !new_text.is_empty() {
        let max_len = cmp::min(new_text.len(), 10);
        let len = rng.random_range(1..=max_len);
        let (chunk, suffix) = new_text.split_at(len);
        chunks_tx.unbounded_send(chunk.to_string()).unwrap();
        new_text = suffix;
        cx.background_executor.run_until_parked();
    }
    drop(chunks_tx);
    cx.background_executor.run_until_parked();

    assert_eq!(
        buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx).text()),
        indoc! {"
                fn main() {
                    let mut x = 0;
                    while x < 10 {
                        x += 1;
                    }
                }
            "}
    );
}

#[gpui::test(iterations = 10)]
async fn test_autoindent_respects_tabs_in_selection(cx: &mut TestAppContext) {
    init_test(cx);

    let text = indoc! {"
            func main() {
            \tx := 0
            \tfor i := 0; i < 10; i++ {
            \t\tx++
            \t}
            }
        "};
    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let range = buffer.read_with(cx, |buffer, cx| {
        let snapshot = buffer.snapshot(cx);
        snapshot.anchor_before(Point::new(0, 0))..snapshot.anchor_after(Point::new(4, 2))
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

    let chunks_tx = simulate_response_stream(&codegen, cx);
    let new_text = concat!(
        "func main() {\n",
        "\tx := 0\n",
        "\tfor x < 10 {\n",
        "\t\tx++\n",
        "\t}", //
    );
    chunks_tx.unbounded_send(new_text.to_string()).unwrap();
    drop(chunks_tx);
    cx.background_executor.run_until_parked();

    assert_eq!(
        buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx).text()),
        indoc! {"
                func main() {
                \tx := 0
                \tfor x < 10 {
                \t\tx++
                \t}
                }
            "}
    );
}

#[gpui::test]
async fn test_inactive_codegen_alternative(cx: &mut TestAppContext) {
    init_test(cx);

    let text = indoc! {"
            fn main() {
                let x = 0;
            }
        "};
    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(rust_lang(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let range = buffer.read_with(cx, |buffer, cx| {
        let snapshot = buffer.snapshot(cx);
        snapshot.anchor_before(Point::new(1, 0))..snapshot.anchor_after(Point::new(1, 14))
    });
    let prompt_builder = Arc::new(PromptBuilder::new(None).unwrap());
    let codegen = cx.new(|cx| {
        CodegenAlternative::new(
            buffer.clone(),
            range.clone(),
            false,
            prompt_builder,
            Uuid::new_v4(),
            cx,
        )
    });

    let chunks_tx = simulate_response_stream(&codegen, cx);
    chunks_tx
        .unbounded_send("let mut x = 0;\nx += 1;".to_string())
        .unwrap();
    drop(chunks_tx);
    cx.run_until_parked();

    // The codegen is inactive, so the buffer doesn't get modified.
    assert_eq!(
        buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx).text()),
        text
    );

    // Activating the codegen applies the changes.
    codegen.update(cx, |codegen, cx| codegen.set_active(true, cx));
    assert_eq!(
        buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx).text()),
        indoc! {"
                fn main() {
                    let mut x = 0;
                    x += 1;
                }
            "}
    );

    // Deactivating the codegen undoes the changes.
    codegen.update(cx, |codegen, cx| codegen.set_active(false, cx));
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx).text()),
        text
    );
}
