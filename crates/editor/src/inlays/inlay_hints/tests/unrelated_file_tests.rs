use super::*;

#[gpui::test]
async fn test_no_hint_updates_for_unrelated_language_files(cx: &mut gpui::TestAppContext) {
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
            "main.rs": "fn main() { a } // and some long comment to ensure inlays are not trimmed out",
            "other.md": "Test md file with some text",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let mut rs_fake_servers = None;
    let mut md_fake_servers = None;
    for (name, path_suffix) in [("Rust", "rs"), ("Markdown", "md")] {
        language_registry.add(Arc::new(Language::new(
            LanguageConfig {
                name: name.into(),
                matcher: LanguageMatcher {
                    path_suffixes: vec![path_suffix.to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )));
        let fake_servers = language_registry.register_fake_lsp(
            name,
            FakeLspAdapter {
                name,
                capabilities: lsp::ServerCapabilities {
                    inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                    ..Default::default()
                },
                initializer: Some(Box::new({
                    move |fake_server| {
                        let rs_lsp_request_count = Arc::new(AtomicU32::new(0));
                        let md_lsp_request_count = Arc::new(AtomicU32::new(0));
                        fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                            move |params, _| {
                                let i = match name {
                                    "Rust" => {
                                        assert_eq!(
                                            params.text_document.uri,
                                            lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                                        );
                                        rs_lsp_request_count.fetch_add(1, Ordering::Release) + 1
                                    }
                                    "Markdown" => {
                                        assert_eq!(
                                            params.text_document.uri,
                                            lsp::Uri::from_file_path(path!("/a/other.md")).unwrap(),
                                        );
                                        md_lsp_request_count.fetch_add(1, Ordering::Release) + 1
                                    }
                                    unexpected => {
                                        panic!("Unexpected language: {unexpected}")
                                    }
                                };

                                async move {
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
                    }
                })),
                ..Default::default()
            },
        );
        match name {
            "Rust" => rs_fake_servers = Some(fake_servers),
            "Markdown" => md_fake_servers = Some(fake_servers),
            _ => unreachable!(),
        }
    }

    let rs_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let rs_editor = cx
        .add_window(|window, cx| Editor::for_buffer(rs_buffer, Some(project.clone()), window, cx));
    cx.executor().run_until_parked();

    let _rs_fake_server = rs_fake_servers.unwrap().next().await.unwrap();
    cx.executor().run_until_parked();

    // Establish a viewport so the editor considers itself visible and the hint refresh
    // pipeline runs. Then explicitly trigger a refresh.
    rs_editor
        .update(cx, |editor, window, cx| {
            editor.set_visible_line_count(50.0, window, cx);
            editor.set_visible_column_count(120.0);
            editor.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    rs_editor
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

    cx.executor().run_until_parked();
    let md_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/a/other.md"), cx)
        })
        .await
        .unwrap();
    let md_editor =
        cx.add_window(|window, cx| Editor::for_buffer(md_buffer, Some(project), window, cx));
    cx.executor().run_until_parked();

    let _md_fake_server = md_fake_servers.unwrap().next().await.unwrap();
    cx.executor().run_until_parked();

    // Establish a viewport so the editor considers itself visible and the hint refresh
    // pipeline runs. Then explicitly trigger a refresh.
    md_editor
        .update(cx, |editor, window, cx| {
            editor.set_visible_line_count(50.0, window, cx);
            editor.set_visible_column_count(120.0);
            editor.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    md_editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec!["1".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Markdown editor should have a separate version, repeating Rust editor rules"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();

    rs_editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_ranges([MultiBufferOffset(13)..MultiBufferOffset(13)])
            });
            editor.handle_input("some rs change", window, cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    rs_editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec!["2".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Rust inlay cache should change after the edit"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();
    md_editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec!["1".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Markdown editor should not be affected by Rust editor changes"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();

    md_editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_ranges([MultiBufferOffset(13)..MultiBufferOffset(13)])
            });
            editor.handle_input("some md change", window, cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    md_editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec!["2".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Rust editor should not be affected by Markdown editor changes"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();
    rs_editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec!["2".to_string()];
            assert_eq!(
                expected_hints,
                cached_hint_labels(editor, cx),
                "Markdown editor should also change independently"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();
}
