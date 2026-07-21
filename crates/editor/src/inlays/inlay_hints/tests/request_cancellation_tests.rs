use super::*;

#[gpui::test]
async fn test_hint_request_cancellation(cx: &mut gpui::TestAppContext) {
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

    let lsp_request_count = Arc::new(AtomicU32::new(0));
    let (_, editor, _) = prepare_test_objects(cx, {
        let lsp_request_count = lsp_request_count.clone();
        move |fake_server, file_with_hints| {
            let lsp_request_count = lsp_request_count.clone();
            fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                move |params, _| {
                    let lsp_request_count = lsp_request_count.clone();
                    async move {
                        let i = lsp_request_count.fetch_add(1, Ordering::SeqCst) + 1;
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
        }
    })
    .await;

    let mut expected_changes = Vec::new();
    for change_after_opening in [
        "initial change #1",
        "initial change #2",
        "initial change #3",
    ] {
        editor
            .update(cx, |editor, window, cx| {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_ranges([MultiBufferOffset(13)..MultiBufferOffset(13)])
                });
                editor.handle_input(change_after_opening, window, cx);
            })
            .unwrap();
        expected_changes.push(change_after_opening);
    }

    cx.executor().run_until_parked();

    editor
        .update(cx, |editor, _window, cx| {
            let current_text = editor.text(cx);
            for change in &expected_changes {
                assert!(
                    current_text.contains(change),
                    "Should apply all changes made"
                );
            }
            assert_eq!(
                lsp_request_count.load(Ordering::Relaxed),
                2,
                "Should query new hints twice: for editor init and for the last edit that interrupted all others"
            );
            let expected_hints = vec!["2".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Should get hints from the last edit landed only"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();

    let mut edits = Vec::new();
    for async_later_change in [
        "another change #1",
        "another change #2",
        "another change #3",
    ] {
        expected_changes.push(async_later_change);
        let task_editor = editor;
        edits.push(cx.spawn(|mut cx| async move {
            task_editor
                .update(&mut cx, |editor, window, cx| {
                    editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                        s.select_ranges([MultiBufferOffset(13)..MultiBufferOffset(13)])
                    });
                    editor.handle_input(async_later_change, window, cx);
                })
                .unwrap();
        }));
    }
    let _ = future::join_all(edits).await;
    cx.executor().run_until_parked();

    editor
        .update(cx, |editor, _, cx| {
            let current_text = editor.text(cx);
            for change in &expected_changes {
                assert!(
                    current_text.contains(change),
                    "Should apply all changes made"
                );
            }
            assert_eq!(
                lsp_request_count.load(Ordering::SeqCst),
                3,
                "Should query new hints one more time, for the last edit only"
            );
            let expected_hints = vec!["3".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Should get hints from the last edit landed only"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();
}
