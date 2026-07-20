use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_title_generation(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let summary_model = Arc::new(FakeLanguageModel::default());
    thread.update(cx, |thread, cx| {
        thread.set_summarization_model(Some(summary_model.clone()), cx)
    });

    let send = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hello"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_text_chunk("Hey!");
    fake_model.end_last_completion_stream();
    cx.run_until_parked();
    thread.read_with(cx, |thread, _| assert_eq!(thread.title(), None));

    // Ensure the summary model has been invoked to generate a title.
    summary_model.send_last_completion_stream_text_chunk("Hello ");
    summary_model.send_last_completion_stream_text_chunk("world\nG");
    summary_model.send_last_completion_stream_text_chunk("oodnight Moon");
    summary_model.end_last_completion_stream();
    send.collect::<Vec<_>>().await;
    cx.run_until_parked();
    thread.read_with(cx, |thread, _| {
        assert_eq!(thread.title(), Some("Hello world".into()))
    });

    // Send another message, ensuring no title is generated this time.
    let send = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hello again"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_text_chunk("Hey again!");
    fake_model.end_last_completion_stream();
    cx.run_until_parked();
    assert_eq!(summary_model.pending_completions(), Vec::new());
    send.collect::<Vec<_>>().await;
    thread.read_with(cx, |thread, _| {
        assert_eq!(thread.title(), Some("Hello world".into()))
    });
}

#[gpui::test]
async fn test_stream_thread_title_keeps_only_first_line(cx: &mut TestAppContext) {
    let model = Arc::new(FakeLanguageModel::default());
    let request = LanguageModelRequest::default();

    let title_task = cx.spawn({
        let model = model.clone();
        async move |cx| crate::stream_thread_title(model, request, &cx).await
    });

    cx.run_until_parked();

    model.send_last_completion_stream_text_chunk("Hello world\nGoodnight Moon");
    model.end_last_completion_stream();

    let title = title_task.await.unwrap();
    assert_eq!(title, "Hello world");
}

#[gpui::test]
async fn test_stream_thread_title_stops_when_newline_ends_chunk(cx: &mut TestAppContext) {
    let model = Arc::new(FakeLanguageModel::default());
    let request = LanguageModelRequest::default();

    let title_task = cx.spawn({
        let model = model.clone();
        async move |cx| crate::stream_thread_title(model, request, &cx).await
    });

    cx.run_until_parked();

    model.send_last_completion_stream_text_chunk("Hello world\n");
    model.send_last_completion_stream_text_chunk("Goodnight Moon");
    model.end_last_completion_stream();

    let title = title_task.await.unwrap();
    assert_eq!(title, "Hello world");
}

// `Thread::to_markdown` (live native) and `DbThread::to_markdown` (persisted
// native) must stay byte-for-byte identical for the same messages, since both
// back the sidebar's native "Open Thread as Markdown" action. This pins that
// they share a single rendering path.
#[gpui::test]
async fn test_db_thread_markdown_matches_live_thread(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let send = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hello"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_text_chunk("Hey there!");
    fake_model.end_last_completion_stream();
    send.collect::<Vec<_>>().await;
    cx.run_until_parked();

    let db_thread = thread.update(cx, |thread, cx| thread.to_db(cx)).await;
    let live_markdown = thread.read_with(cx, |thread, _| thread.to_markdown());

    assert!(!live_markdown.is_empty());
    assert_eq!(db_thread.to_markdown(), live_markdown);
}

#[gpui::test]
async fn test_title_generation_failure_allows_retry(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let summary_model = Arc::new(FakeLanguageModel::default());
    let fake_summary_model = summary_model.as_fake();
    thread.update(cx, |thread, cx| {
        thread.set_summarization_model(Some(summary_model.clone()), cx)
    });

    let send = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hello"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_text_chunk("Hey!");
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    fake_summary_model.send_last_completion_stream_error(
        LanguageModelCompletionError::UpstreamProviderError {
            message: "Internal server error".to_string(),
            status: gpui::http_client::StatusCode::INTERNAL_SERVER_ERROR,
            retry_after: None,
        },
    );
    fake_summary_model.end_last_completion_stream();
    send.collect::<Vec<_>>().await;
    cx.run_until_parked();

    thread.read_with(cx, |thread, _| {
        assert_eq!(thread.title(), None);
        assert!(thread.has_failed_title_generation());
        assert!(!thread.is_generating_title());
    });

    thread.update(cx, |thread, cx| {
        thread.generate_title(cx);
    });
    cx.run_until_parked();

    thread.read_with(cx, |thread, _| {
        assert!(!thread.has_failed_title_generation());
        assert!(thread.is_generating_title());
    });

    fake_summary_model.send_last_completion_stream_text_chunk("Retried title");
    fake_summary_model.end_last_completion_stream();
    cx.run_until_parked();

    thread.read_with(cx, |thread, _| {
        assert_eq!(thread.title(), Some("Retried title".into()));
        assert!(!thread.has_failed_title_generation());
        assert!(!thread.is_generating_title());
    });
}
