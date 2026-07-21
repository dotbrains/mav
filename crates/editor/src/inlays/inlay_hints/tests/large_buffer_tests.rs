use super::*;

#[gpui::test(iterations = 4)]
async fn test_large_buffer_inlay_requests_split(cx: &mut gpui::TestAppContext) {
    init_test(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            enabled: Some(true),
            ..InlayHintSettingsContent::default()
        })
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": format!("fn main() {{\n{}\n}}", "let i = 5;\n".repeat(500)),
            "other.rs": "// Test file",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    let lsp_request_ranges = Arc::new(Mutex::new(Vec::new()));
    let lsp_request_count = Arc::new(AtomicUsize::new(0));
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new({
                let lsp_request_ranges = lsp_request_ranges.clone();
                let lsp_request_count = lsp_request_count.clone();
                move |fake_server| {
                    let closure_lsp_request_ranges = Arc::clone(&lsp_request_ranges);
                    let closure_lsp_request_count = Arc::clone(&lsp_request_count);
                    fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                        move |params, _| {
                            let task_lsp_request_ranges = Arc::clone(&closure_lsp_request_ranges);
                            let task_lsp_request_count = Arc::clone(&closure_lsp_request_count);
                            async move {
                                assert_eq!(
                                    params.text_document.uri,
                                    lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                                );

                                task_lsp_request_ranges.lock().push(params.range);
                                task_lsp_request_count.fetch_add(1, Ordering::Release);
                                Ok(Some(vec![lsp::InlayHint {
                                    position: params.range.start,
                                    label: lsp::InlayHintLabel::String(
                                        params.range.end.line.to_string(),
                                    ),
                                    kind: None,
                                    text_edits: None,
                                    tooltip: None,
                                    padding_left: None,
                                    padding_right: None,
                                    data: None,
                                }]))
                            }
                        },
                    );
                }
            })),
            ..FakeLspAdapter::default()
        },
    );

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let editor = cx.add_window(|window, cx| Editor::for_buffer(buffer, Some(project), window, cx));
    cx.executor().run_until_parked();
    let _fake_server = fake_servers.next().await.unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();

    let ranges = lsp_request_ranges
        .lock()
        .drain(..)
        .sorted_by_key(|r| r.start)
        .collect::<Vec<_>>();
    assert_eq!(
        ranges.len(),
        1,
        "Should query 1 range initially, but got: {ranges:?}"
    );

    editor
        .update(cx, |editor, window, cx| {
            editor.scroll_screen(&ScrollAmount::Page(1.0), window, cx);
        })
        .unwrap();
    // Wait for the first hints request to fire off
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, window, cx| {
            editor.scroll_screen(&ScrollAmount::Page(1.0), window, cx);
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();
    let visible_range_after_scrolls = editor_visible_range(&editor, cx);
    let visible_line_count = editor
        .update(cx, |editor, _window, _| {
            editor.visible_line_count().unwrap()
        })
        .unwrap();
    let selection_in_cached_range = editor
        .update(cx, |editor, _window, cx| {
            let ranges = lsp_request_ranges
                .lock()
                .drain(..)
                .sorted_by_key(|r| r.start)
                .collect::<Vec<_>>();
            assert_eq!(
                ranges.len(),
                2,
                "Should query 2 ranges after both scrolls, but got: {ranges:?}"
            );
            let first_scroll = &ranges[0];
            let second_scroll = &ranges[1];
            assert_eq!(
                first_scroll.end.line, second_scroll.start.line,
                "Should query 2 adjacent ranges after the scrolls, but got: {ranges:?}"
            );

            let lsp_requests = lsp_request_count.load(Ordering::Acquire);
            assert_eq!(
                lsp_requests, 3,
                "Should query hints initially, and after each scroll (2 times)"
            );
            assert_eq!(
                vec!["50".to_string(), "100".to_string(), "150".to_string()],
                cached_hint_labels(editor, cx),
                "Chunks of 50 line width should have been queried each time"
            );
            assert_eq!(
                vec!["50".to_string(), "100".to_string(), "150".to_string()],
                visible_hint_labels(editor, cx),
                "Editor should show only hints that it's scrolled to"
            );

            let mut selection_in_cached_range = visible_range_after_scrolls.end;
            selection_in_cached_range.row -= visible_line_count.ceil() as u32;
            selection_in_cached_range
        })
        .unwrap();

    editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(
                SelectionEffects::scroll(Autoscroll::center()),
                window,
                cx,
                |s| s.select_ranges([selection_in_cached_range..selection_in_cached_range]),
            );
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();
    editor.update(cx, |_, _, _| {
        let ranges = lsp_request_ranges
            .lock()
            .drain(..)
            .sorted_by_key(|r| r.start)
            .collect::<Vec<_>>();
        assert!(ranges.is_empty(), "No new ranges or LSP queries should be made after returning to the selection with cached hints");
        assert_eq!(lsp_request_count.load(Ordering::Acquire), 3, "No new requests should be made when selecting within cached chunks");
    }).unwrap();

    editor
        .update(cx, |editor, window, cx| {
            editor.handle_input("++++more text++++", window, cx);
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    editor.update(cx, |editor, _window, cx| {
        let mut ranges = lsp_request_ranges.lock().drain(..).collect::<Vec<_>>();
        ranges.sort_by_key(|r| r.start);

        assert_eq!(ranges.len(), 2,
            "On edit, should scroll to selection and query a range around it: that range should split into 2 50 rows wide chunks. Instead, got query ranges {ranges:?}");
        let first_chunk = &ranges[0];
        let second_chunk = &ranges[1];
        assert!(first_chunk.end.line == second_chunk.start.line,
            "First chunk {first_chunk:?} should be before second chunk {second_chunk:?}");
        assert!(first_chunk.start.line < selection_in_cached_range.row,
            "Hints should be queried with the selected range after the query range start");

        let lsp_requests = lsp_request_count.load(Ordering::Acquire);
        assert_eq!(lsp_requests, 5, "Two chunks should be re-queried");
        assert_eq!(vec!["100".to_string(), "150".to_string()], cached_hint_labels(editor, cx),
            "Should have (less) hints from the new LSP response after the edit");
        assert_eq!(vec!["100".to_string(), "150".to_string()], visible_hint_labels(editor, cx), "Should show only visible hints (in the center) from the new cached set");
    }).unwrap();
}

fn editor_visible_range(
    editor: &WindowHandle<Editor>,
    cx: &mut gpui::TestAppContext,
) -> Range<Point> {
    let ranges = editor
        .update(cx, |editor, _window, cx| editor.visible_buffer_ranges(cx))
        .unwrap();
    assert_eq!(
        ranges.len(),
        1,
        "Single buffer should produce a single excerpt with visible range"
    );
    let (buffer_snapshot, visible_range, _) = ranges.into_iter().next().unwrap();
    visible_range.to_point(&buffer_snapshot)
}
