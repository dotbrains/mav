use super::*;

#[gpui::test]
async fn test_invalidation_and_addition_race(cx: &mut gpui::TestAppContext) {
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
            "main.rs": r#"fn main() {
                let x = 1;
                ////
                ////
                ////
                ////
                ////
                ////
                ////
                ////
                ////
                ////
                ////
                ////
                ////
                ////
                ////
                ////
                ////
                let x = "2";
            }
"#,
            "lib.rs": r#"fn aaa() {
                let aa = 22;
            }
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //
            //

            fn bb() {
                let bb = 33;
            }
"#
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let language = rust_lang();
    language_registry.add(language);

    let requests_count = Arc::new(AtomicUsize::new(0));
    let closure_requests_count = requests_count.clone();
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "rust-analyzer",
            capabilities: lsp::ServerCapabilities {
                inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new(move |fake_server| {
                let requests_count = closure_requests_count.clone();
                fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                    move |params, _| {
                        let requests_count = requests_count.clone();
                        async move {
                            requests_count.fetch_add(1, Ordering::Release);
                            if params.text_document.uri
                                == lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap()
                            {
                                Ok(Some(vec![
                                    lsp::InlayHint {
                                        position: lsp::Position::new(1, 9),
                                        label: lsp::InlayHintLabel::String(": i32".to_owned()),
                                        kind: Some(lsp::InlayHintKind::TYPE),
                                        text_edits: None,
                                        tooltip: None,
                                        padding_left: None,
                                        padding_right: None,
                                        data: None,
                                    },
                                    lsp::InlayHint {
                                        position: lsp::Position::new(19, 9),
                                        label: lsp::InlayHintLabel::String(": i33".to_owned()),
                                        kind: Some(lsp::InlayHintKind::TYPE),
                                        text_edits: None,
                                        tooltip: None,
                                        padding_left: None,
                                        padding_right: None,
                                        data: None,
                                    },
                                ]))
                            } else if params.text_document.uri
                                == lsp::Uri::from_file_path(path!("/a/lib.rs")).unwrap()
                            {
                                Ok(Some(vec![
                                    lsp::InlayHint {
                                        position: lsp::Position::new(1, 10),
                                        label: lsp::InlayHintLabel::String(": i34".to_owned()),
                                        kind: Some(lsp::InlayHintKind::TYPE),
                                        text_edits: None,
                                        tooltip: None,
                                        padding_left: None,
                                        padding_right: None,
                                        data: None,
                                    },
                                    lsp::InlayHint {
                                        position: lsp::Position::new(29, 10),
                                        label: lsp::InlayHintLabel::String(": i35".to_owned()),
                                        kind: Some(lsp::InlayHintKind::TYPE),
                                        text_edits: None,
                                        tooltip: None,
                                        padding_left: None,
                                        padding_right: None,
                                        data: None,
                                    },
                                ]))
                            } else {
                                panic!("Unexpected file path {:?}", params.text_document.uri);
                            }
                        }
                    },
                );
            })),
            ..FakeLspAdapter::default()
        },
    );

    // Add another server that does send the same, duplicate hints back
    let mut fake_servers_2 = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "CrabLang-ls",
            capabilities: lsp::ServerCapabilities {
                inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new(move |fake_server| {
                fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                    move |params, _| async move {
                        if params.text_document.uri
                            == lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap()
                        {
                            Ok(Some(vec![
                                lsp::InlayHint {
                                    position: lsp::Position::new(1, 9),
                                    label: lsp::InlayHintLabel::String(": i32".to_owned()),
                                    kind: Some(lsp::InlayHintKind::TYPE),
                                    text_edits: None,
                                    tooltip: None,
                                    padding_left: None,
                                    padding_right: None,
                                    data: None,
                                },
                                lsp::InlayHint {
                                    position: lsp::Position::new(19, 9),
                                    label: lsp::InlayHintLabel::String(": i33".to_owned()),
                                    kind: Some(lsp::InlayHintKind::TYPE),
                                    text_edits: None,
                                    tooltip: None,
                                    padding_left: None,
                                    padding_right: None,
                                    data: None,
                                },
                            ]))
                        } else if params.text_document.uri
                            == lsp::Uri::from_file_path(path!("/a/lib.rs")).unwrap()
                        {
                            Ok(Some(vec![
                                lsp::InlayHint {
                                    position: lsp::Position::new(1, 10),
                                    label: lsp::InlayHintLabel::String(": i34".to_owned()),
                                    kind: Some(lsp::InlayHintKind::TYPE),
                                    text_edits: None,
                                    tooltip: None,
                                    padding_left: None,
                                    padding_right: None,
                                    data: None,
                                },
                                lsp::InlayHint {
                                    position: lsp::Position::new(29, 10),
                                    label: lsp::InlayHintLabel::String(": i35".to_owned()),
                                    kind: Some(lsp::InlayHintKind::TYPE),
                                    text_edits: None,
                                    tooltip: None,
                                    padding_left: None,
                                    padding_right: None,
                                    data: None,
                                },
                            ]))
                        } else {
                            panic!("Unexpected file path {:?}", params.text_document.uri);
                        }
                    },
                );
            })),
            ..FakeLspAdapter::default()
        },
    );

    let (buffer_1, _handle_1) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let (buffer_2, _handle_2) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/a/lib.rs"), cx)
        })
        .await
        .unwrap();
    let multi_buffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_2.clone(),
            [
                Point::new(0, 0)..Point::new(10, 0),
                Point::new(23, 0)..Point::new(34, 0),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_1.clone(),
            [
                Point::new(0, 0)..Point::new(10, 0),
                Point::new(13, 0)..Point::new(23, 0),
            ],
            0,
            cx,
        );
        multibuffer
    });

    let editor = cx.add_window(|window, cx| {
        let mut editor = Editor::for_multibuffer(multi_buffer, Some(project.clone()), window, cx);
        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges([Point::new(3, 3)..Point::new(3, 3)])
        });
        editor
    });

    let fake_server = fake_servers.next().await.unwrap();
    let _fake_server_2 = fake_servers_2.next().await.unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();

    editor
        .update(cx, |editor, _window, cx| {
            assert_eq!(
                vec![
                    ": i32".to_string(),
                    ": i32".to_string(),
                    ": i33".to_string(),
                    ": i33".to_string(),
                    ": i34".to_string(),
                    ": i34".to_string(),
                    ": i35".to_string(),
                    ": i35".to_string(),
                ],
                sorted_cached_hint_labels(editor, cx),
                "We receive duplicate hints from 2 servers and cache them all"
            );
            assert_eq!(
                vec![
                    ": i34".to_string(),
                    ": i35".to_string(),
                    ": i32".to_string(),
                    ": i33".to_string(),
                ],
                visible_hint_labels(editor, cx),
                "lib.rs is added before main.rs , so its excerpts should be visible first; hints should be deduplicated per label"
            );
        })
        .unwrap();
    assert_eq!(
        requests_count.load(Ordering::Acquire),
        2,
        "Should have queried hints once per each file"
    );

    // Scroll all the way down so the 1st buffer is out of sight.
    // The selection is on the 1st buffer still.
    editor
        .update(cx, |editor, window, cx| {
            editor.scroll_screen(&ScrollAmount::Line(88.0), window, cx);
        })
        .unwrap();
    // Emulate a language server refresh request, coming in the background..
    editor
        .update(cx, |editor, _, cx| {
            editor.refresh_inlay_hints(
                InlayHintRefreshReason::RefreshRequested {
                    server_id: fake_server.server.server_id(),
                    request_id: Some(1),
                },
                cx,
            );
        })
        .unwrap();
    // Edit the 1st buffer while scrolled down and not seeing that.
    // The edit will auto scroll to the edit (1st buffer).
    editor
        .update(cx, |editor, window, cx| {
            editor.handle_input("a", window, cx);
        })
        .unwrap();
    // Add more racy additive hint tasks.
    editor
        .update(cx, |editor, window, cx| {
            editor.scroll_screen(&ScrollAmount::Line(0.2), window, cx);
        })
        .unwrap();

    cx.executor().advance_clock(Duration::from_millis(1000));
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            assert_eq!(
                vec![
                    ": i32".to_string(),
                    ": i32".to_string(),
                    ": i33".to_string(),
                    ": i33".to_string(),
                    ": i34".to_string(),
                    ": i34".to_string(),
                    ": i35".to_string(),
                    ": i35".to_string(),
                ],
                sorted_cached_hint_labels(editor, cx),
                "No hint changes/duplicates should occur in the cache",
            );
            assert_eq!(
                vec![
                    ": i34".to_string(),
                    ": i35".to_string(),
                    ": i32".to_string(),
                    ": i33".to_string(),
                ],
                visible_hint_labels(editor, cx),
                "No hint changes/duplicates should occur in the editor excerpts",
            );
        })
        .unwrap();
    assert_eq!(
        requests_count.load(Ordering::Acquire),
        4,
        "Should have queried hints once more per each file, after editing the file once"
    );
}
