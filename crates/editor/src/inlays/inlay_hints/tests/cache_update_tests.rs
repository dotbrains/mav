use super::*;

#[gpui::test]
async fn test_basic_cache_update_with_duplicate_hints(cx: &mut gpui::TestAppContext) {
    let allowed_hint_kinds = HashSet::from_iter([None, Some(InlayHintKind::Type)]);
    init_test(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            show_value_hints: Some(true),
            enabled: Some(true),
            edit_debounce_ms: Some(0),
            scroll_debounce_ms: Some(0),
            show_type_hints: Some(allowed_hint_kinds.contains(&Some(InlayHintKind::Type))),
            show_parameter_hints: Some(
                allowed_hint_kinds.contains(&Some(InlayHintKind::Parameter)),
            ),
            show_other_hints: Some(allowed_hint_kinds.contains(&None)),
            show_background: Some(false),
            toggle_on_modifiers_press: None,
        })
    });
    let (_, editor, fake_server) = prepare_test_objects(cx, |fake_server, file_with_hints| {
        let lsp_request_count = Arc::new(AtomicU32::new(0));
        fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
            move |params, _| {
                let task_lsp_request_count = Arc::clone(&lsp_request_count);
                async move {
                    let i = task_lsp_request_count.fetch_add(1, Ordering::Release) + 1;
                    assert_eq!(
                        params.text_document.uri,
                        lsp::Uri::from_file_path(file_with_hints).unwrap(),
                    );
                    Ok(Some(vec![lsp::InlayHint {
                        position: lsp::Position::new(0, i),
                        label: lsp::InlayHintLabel::String(i.to_string()),
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
    })
    .await;
    cx.executor().run_until_parked();

    editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec!["1".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Should get its first hints when opening the editor"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
            assert_eq!(
                allowed_hint_kinds_for_editor(editor),
                allowed_hint_kinds,
                "Cache should use editor settings to get the allowed hint kinds"
            );
        })
        .unwrap();

    editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_ranges([MultiBufferOffset(13)..MultiBufferOffset(13)])
            });
            editor.handle_input("some change", window, cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec!["2".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Should get new hints after an edit"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
            assert_eq!(
                allowed_hint_kinds_for_editor(editor),
                allowed_hint_kinds,
                "Cache should use editor settings to get the allowed hint kinds"
            );
        })
        .unwrap();

    fake_server
        .request::<lsp::request::InlayHintRefreshRequest>((), DEFAULT_LSP_REQUEST_TIMEOUT)
        .await
        .into_response()
        .expect("inlay refresh request failed");
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec!["3".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Should get new hints after hint refresh/ request"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
            assert_eq!(
                allowed_hint_kinds_for_editor(editor),
                allowed_hint_kinds,
                "Cache should use editor settings to get the allowed hint kinds"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_racy_cache_updates(cx: &mut gpui::TestAppContext) {
    init_test(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            enabled: Some(true),
            ..InlayHintSettingsContent::default()
        })
    });
    let (_, editor, fake_server) = prepare_test_objects(cx, |fake_server, file_with_hints| {
        let lsp_request_count = Arc::new(AtomicU32::new(0));
        fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
            move |params, _| {
                let task_lsp_request_count = Arc::clone(&lsp_request_count);
                async move {
                    let i = task_lsp_request_count.fetch_add(1, Ordering::Release) + 1;
                    assert_eq!(
                        params.text_document.uri,
                        lsp::Uri::from_file_path(file_with_hints).unwrap(),
                    );
                    Ok(Some(vec![lsp::InlayHint {
                        position: lsp::Position::new(0, i),
                        label: lsp::InlayHintLabel::String(i.to_string()),
                        kind: Some(lsp::InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: None,
                        padding_left: None,
                        padding_right: None,
                        data: None,
                    }]))
                }
            },
        );
    })
    .await;
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();

    editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec!["1".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Should get its first hints when opening the editor"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();

    // Emulate simultaneous events: both editing, refresh and, slightly after, scroll updates are triggered.
    editor
        .update(cx, |editor, window, cx| {
            editor.handle_input("foo", window, cx);
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(5));
    editor
        .update(cx, |editor, _window, cx| {
            editor.refresh_inlay_hints(
                InlayHintRefreshReason::RefreshRequested {
                    server_id: fake_server.server.server_id(),
                    request_id: Some(1),
                },
                cx,
            );
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(5));
    editor
        .update(cx, |editor, _window, cx| {
            editor.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec!["2".to_string()];
            assert_eq!(expected_hints, cached_hint_labels(editor, cx), "Despite multiple simultaneous refreshes, only one inlay hint query should be issued");
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();
}

#[gpui::test]
async fn test_cache_update_on_lsp_completion_tasks(cx: &mut gpui::TestAppContext) {
    init_test(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            show_value_hints: Some(true),
            enabled: Some(true),
            edit_debounce_ms: Some(0),
            scroll_debounce_ms: Some(0),
            show_type_hints: Some(true),
            show_parameter_hints: Some(true),
            show_other_hints: Some(true),
            show_background: Some(false),
            toggle_on_modifiers_press: None,
        })
    });

    let (_, editor, fake_server) = prepare_test_objects(cx, |fake_server, file_with_hints| {
        let lsp_request_count = Arc::new(AtomicU32::new(0));
        fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
            move |params, _| {
                let task_lsp_request_count = Arc::clone(&lsp_request_count);
                async move {
                    assert_eq!(
                        params.text_document.uri,
                        lsp::Uri::from_file_path(file_with_hints).unwrap(),
                    );
                    let current_call_id =
                        Arc::clone(&task_lsp_request_count).fetch_add(1, Ordering::SeqCst);
                    Ok(Some(vec![lsp::InlayHint {
                        position: lsp::Position::new(0, current_call_id),
                        label: lsp::InlayHintLabel::String(current_call_id.to_string()),
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
    })
    .await;
    cx.executor().run_until_parked();

    editor
        .update(cx, |editor, _, cx| {
            let expected_hints = vec!["0".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Should get its first hints when opening the editor"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();

    let progress_token = 42;
    fake_server
        .request::<lsp::request::WorkDoneProgressCreate>(
            lsp::WorkDoneProgressCreateParams {
                token: lsp::ProgressToken::Number(progress_token),
            },
            DEFAULT_LSP_REQUEST_TIMEOUT,
        )
        .await
        .into_response()
        .expect("work done progress create request failed");
    cx.executor().run_until_parked();
    fake_server.notify::<lsp::notification::Progress>(lsp::ProgressParams {
        token: lsp::ProgressToken::Number(progress_token),
        value: lsp::ProgressParamsValue::WorkDone(lsp::WorkDoneProgress::Begin(
            lsp::WorkDoneProgressBegin::default(),
        )),
    });
    cx.executor().run_until_parked();

    editor
        .update(cx, |editor, _, cx| {
            let expected_hints = vec!["0".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Should not update hints while the work task is running"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();

    fake_server.notify::<lsp::notification::Progress>(lsp::ProgressParams {
        token: lsp::ProgressToken::Number(progress_token),
        value: lsp::ProgressParamsValue::WorkDone(lsp::WorkDoneProgress::End(
            lsp::WorkDoneProgressEnd::default(),
        )),
    });
    cx.executor().run_until_parked();

    editor
        .update(cx, |editor, _, cx| {
            let expected_hints = vec!["1".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "New hints should be queried after the work task is done"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();
}
