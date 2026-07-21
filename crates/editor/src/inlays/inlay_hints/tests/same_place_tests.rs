use super::*;

#[gpui::test]
async fn test_inlays_at_the_same_place(cx: &mut gpui::TestAppContext) {
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
            "main.rs": "fn main() {
                let x = 42;
                std::thread::scope(|s| {
                    s.spawn(|| {
                        let _x = x;
                    });
                });
            }",
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
                ..Default::default()
            },
            initializer: Some(Box::new(move |fake_server| {
                fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                    move |params, _| async move {
                        assert_eq!(
                            params.text_document.uri,
                            lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                        );
                        Ok(Some(
                            serde_json::from_value(json!([
                                {
                                    "position": {
                                        "line": 3,
                                        "character": 16
                                    },
                                    "label": "move",
                                    "paddingLeft": false,
                                    "paddingRight": false
                                },
                                {
                                    "position": {
                                        "line": 3,
                                        "character": 16
                                    },
                                    "label": "(",
                                    "paddingLeft": false,
                                    "paddingRight": false
                                },
                                {
                                    "position": {
                                        "line": 3,
                                        "character": 16
                                    },
                                    "label": [
                                        {
                                            "value": "&x"
                                        }
                                    ],
                                    "paddingLeft": false,
                                    "paddingRight": false,
                                    "data": {
                                        "file_id": 0
                                    }
                                },
                                {
                                    "position": {
                                        "line": 3,
                                        "character": 16
                                    },
                                    "label": ")",
                                    "paddingLeft": false,
                                    "paddingRight": true
                                },
                                // not a correct syntax, but checks that same symbols at the same place
                                // are not deduplicated
                                {
                                    "position": {
                                        "line": 3,
                                        "character": 16
                                    },
                                    "label": ")",
                                    "paddingLeft": false,
                                    "paddingRight": true
                                },
                            ]))
                            .unwrap(),
                        ))
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

    // Use a VisualTestContext and explicitly establish a viewport on the editor (the production
    // trigger for `NewLinesShown` / inlay hint refresh) by setting visible line/column counts.
    let (editor_entity, cx) =
        cx.add_window_view(|window, cx| Editor::for_buffer(buffer, Some(project), window, cx));

    editor_entity.update_in(cx, |editor, window, cx| {
        // Establish a viewport. The exact values are not important for this test; we just need
        // the editor to consider itself visible so the refresh pipeline runs.
        editor.set_visible_line_count(50.0, window, cx);
        editor.set_visible_column_count(120.0);

        // Explicitly trigger a refresh now that the viewport exists.
        editor.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
    });
    cx.executor().run_until_parked();

    editor_entity.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(10, 0)..Point::new(10, 0)])
        });
    });
    cx.executor().run_until_parked();

    // Allow any async inlay hint request/response work to complete.
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();

    editor_entity.update(cx, |editor, cx| {
        let expected_hints = vec![
            "move".to_string(),
            "(".to_string(),
            "&x".to_string(),
            ") ".to_string(),
            ") ".to_string(),
        ];
        assert_eq!(
            expected_hints,
            cached_hint_labels(editor, cx),
            "Editor inlay hints should repeat server's order when placed at the same spot"
        );
        assert_eq!(expected_hints, visible_hint_labels(editor, cx));
    });
}
