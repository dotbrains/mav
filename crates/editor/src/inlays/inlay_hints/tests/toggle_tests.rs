use super::*;

#[gpui::test]
async fn test_inside_char_boundary_range_hints(cx: &mut gpui::TestAppContext) {
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

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": format!(r#"fn main() {{\n{}\n}}"#, format!("let i = {};\n", "√".repeat(10)).repeat(500)),
            "other.rs": "// Test file",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new(move |fake_server| {
                let lsp_request_count = Arc::new(AtomicU32::new(0));
                fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                    move |params, _| {
                        let i = lsp_request_count.fetch_add(1, Ordering::Release) + 1;
                        async move {
                            assert_eq!(
                                params.text_document.uri,
                                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                            );
                            let query_start = params.range.start;
                            Ok(Some(vec![lsp::InlayHint {
                                position: query_start,
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

    // Allow LSP to initialize
    cx.executor().run_until_parked();

    // Establish a viewport and explicitly trigger hint refresh.
    // This ensures we control exactly when hints are requested.
    editor
        .update(cx, |editor, window, cx| {
            editor.set_visible_line_count(50.0, window, cx);
            editor.set_visible_column_count(120.0);
            editor.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
        })
        .unwrap();

    // Allow LSP initialization and hint request/response to complete.
    // Use multiple advance_clock + run_until_parked cycles to ensure all async work completes.
    for _ in 0..5 {
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.executor().run_until_parked();
    }

    // At this point we should have exactly one hint from our explicit refresh.
    // The test verifies that hints at character boundaries are handled correctly.
    editor
        .update(cx, |editor, _, cx| {
            assert!(
                !cached_hint_labels(editor, cx).is_empty(),
                "Should have at least one hint after refresh"
            );
            assert!(
                !visible_hint_labels(editor, cx).is_empty(),
                "Should have at least one visible hint"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_toggle_inlay_hints(cx: &mut gpui::TestAppContext) {
    init_test(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            show_value_hints: Some(true),
            enabled: Some(false),
            edit_debounce_ms: Some(0),
            scroll_debounce_ms: Some(0),
            show_type_hints: Some(true),
            show_parameter_hints: Some(true),
            show_other_hints: Some(true),
            show_background: Some(false),
            toggle_on_modifiers_press: None,
        })
    });

    let (_, editor, _fake_server) = prepare_test_objects(cx, |fake_server, file_with_hints| {
        let lsp_request_count = Arc::new(AtomicU32::new(0));
        fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
            move |params, _| {
                let lsp_request_count = lsp_request_count.clone();
                async move {
                    assert_eq!(
                        params.text_document.uri,
                        lsp::Uri::from_file_path(file_with_hints).unwrap(),
                    );

                    let i = lsp_request_count.fetch_add(1, Ordering::AcqRel) + 1;
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

    editor
        .update(cx, |editor, window, cx| {
            editor.toggle_inlay_hints(&crate::ToggleInlayHints, window, cx)
        })
        .unwrap();

    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            let expected_hints = vec!["1".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Should display inlays after toggle despite them disabled in settings"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();

    editor
        .update(cx, |editor, window, cx| {
            editor.toggle_inlay_hints(&crate::ToggleInlayHints, window, cx)
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                vec!["1".to_string()],
                cached_hint_labels(editor, cx),
                "Cache does not change because of toggles in the editor"
            );
            assert_eq!(
                Vec::<String>::new(),
                visible_hint_labels(editor, cx),
                "Should clear hints after 2nd toggle"
            );
        })
        .unwrap();

    update_test_language_settings(cx, &|settings| {
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
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            let expected_hints = vec!["1".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Should not query LSP hints after enabling hints in settings, as file version is the same"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();

    editor
        .update(cx, |editor, window, cx| {
            editor.toggle_inlay_hints(&crate::ToggleInlayHints, window, cx)
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                vec!["1".to_string()],
                cached_hint_labels(editor, cx),
                "Cache does not change because of toggles in the editor"
            );
            assert_eq!(
                Vec::<String>::new(),
                visible_hint_labels(editor, cx),
                "Should clear hints after enabling in settings and a 3rd toggle"
            );
        })
        .unwrap();

    editor
        .update(cx, |editor, window, cx| {
            editor.toggle_inlay_hints(&crate::ToggleInlayHints, window, cx)
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor.update(cx, |editor, _, cx| {
        let expected_hints = vec!["1".to_string()];
        assert_eq!(
            expected_hints,
            cached_hint_labels(editor,cx),
            "Should not query LSP hints after enabling hints in settings and toggling them back on"
        );
        assert_eq!(expected_hints, visible_hint_labels(editor, cx));
    }).unwrap();
}

#[gpui::test]
async fn test_modifiers_change(cx: &mut gpui::TestAppContext) {
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

    let (_, editor, _fake_server) = prepare_test_objects(cx, |fake_server, file_with_hints| {
        let lsp_request_count = Arc::new(AtomicU32::new(0));
        fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
            move |params, _| {
                let lsp_request_count = lsp_request_count.clone();
                async move {
                    assert_eq!(
                        params.text_document.uri,
                        lsp::Uri::from_file_path(file_with_hints).unwrap(),
                    );

                    let i = lsp_request_count.fetch_add(1, Ordering::AcqRel) + 1;
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
        .update(cx, |editor, _, cx| {
            assert_eq!(
                vec!["1".to_string()],
                cached_hint_labels(editor, cx),
                "Should display inlays after toggle despite them disabled in settings"
            );
            assert_eq!(vec!["1".to_string()], visible_hint_labels(editor, cx));
        })
        .unwrap();

    editor
        .update(cx, |editor, _, cx| {
            editor.refresh_inlay_hints(InlayHintRefreshReason::ModifiersChanged(true), cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                vec!["1".to_string()],
                cached_hint_labels(editor, cx),
                "Nothing happens with the cache on modifiers change"
            );
            assert_eq!(
                Vec::<String>::new(),
                visible_hint_labels(editor, cx),
                "On modifiers change and hints toggled on, should hide editor inlays"
            );
        })
        .unwrap();
    editor
        .update(cx, |editor, _, cx| {
            editor.refresh_inlay_hints(InlayHintRefreshReason::ModifiersChanged(true), cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(vec!["1".to_string()], cached_hint_labels(editor, cx));
            assert_eq!(
                Vec::<String>::new(),
                visible_hint_labels(editor, cx),
                "Nothing changes on consequent modifiers change of the same kind"
            );
        })
        .unwrap();

    editor
        .update(cx, |editor, _, cx| {
            editor.refresh_inlay_hints(InlayHintRefreshReason::ModifiersChanged(false), cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                vec!["1".to_string()],
                cached_hint_labels(editor, cx),
                "When modifiers change is off, no extra requests are sent"
            );
            assert_eq!(
                vec!["1".to_string()],
                visible_hint_labels(editor, cx),
                "When modifiers change is off, hints are back into the editor"
            );
        })
        .unwrap();
    editor
        .update(cx, |editor, _, cx| {
            editor.refresh_inlay_hints(InlayHintRefreshReason::ModifiersChanged(false), cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(vec!["1".to_string()], cached_hint_labels(editor, cx));
            assert_eq!(
                vec!["1".to_string()],
                visible_hint_labels(editor, cx),
                "Nothing changes on consequent modifiers change of the same kind (2)"
            );
        })
        .unwrap();

    editor
        .update(cx, |editor, window, cx| {
            editor.toggle_inlay_hints(&crate::ToggleInlayHints, window, cx)
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                vec!["1".to_string()],
                cached_hint_labels(editor, cx),
                "Nothing happens with the cache on modifiers change"
            );
            assert_eq!(
                Vec::<String>::new(),
                visible_hint_labels(editor, cx),
                "When toggled off, should hide editor inlays"
            );
        })
        .unwrap();

    editor
        .update(cx, |editor, _, cx| {
            editor.refresh_inlay_hints(InlayHintRefreshReason::ModifiersChanged(true), cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                vec!["1".to_string()],
                cached_hint_labels(editor, cx),
                "Nothing happens with the cache on modifiers change"
            );
            assert_eq!(
                vec!["1".to_string()],
                visible_hint_labels(editor, cx),
                "On modifiers change & hints toggled off, should show editor inlays"
            );
        })
        .unwrap();
    editor
        .update(cx, |editor, _, cx| {
            editor.refresh_inlay_hints(InlayHintRefreshReason::ModifiersChanged(true), cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(vec!["1".to_string()], cached_hint_labels(editor, cx));
            assert_eq!(
                vec!["1".to_string()],
                visible_hint_labels(editor, cx),
                "Nothing changes on consequent modifiers change of the same kind"
            );
        })
        .unwrap();

    editor
        .update(cx, |editor, _, cx| {
            editor.refresh_inlay_hints(InlayHintRefreshReason::ModifiersChanged(false), cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                vec!["1".to_string()],
                cached_hint_labels(editor, cx),
                "When modifiers change is off, no extra requests are sent"
            );
            assert_eq!(
                Vec::<String>::new(),
                visible_hint_labels(editor, cx),
                "When modifiers change is off, editor hints are back into their toggled off state"
            );
        })
        .unwrap();
    editor
        .update(cx, |editor, _, cx| {
            editor.refresh_inlay_hints(InlayHintRefreshReason::ModifiersChanged(false), cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(vec!["1".to_string()], cached_hint_labels(editor, cx));
            assert_eq!(
                Vec::<String>::new(),
                visible_hint_labels(editor, cx),
                "Nothing changes on consequent modifiers change of the same kind (3)"
            );
        })
        .unwrap();
}
