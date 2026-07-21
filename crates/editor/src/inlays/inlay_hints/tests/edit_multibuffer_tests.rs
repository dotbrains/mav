use super::*;

#[gpui::test]
async fn test_editing_in_multi_buffer(cx: &mut gpui::TestAppContext) {
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
            "main.rs": format!("fn main() {{\n{}\n}}", (0..200).map(|i| format!("let i = {i};\n")).collect::<String>()),
            "lib.rs": r#"let a = 1;
let b = 2;
let c = 3;"#
        }),
    )
    .await;

    let lsp_request_ranges = Arc::new(Mutex::new(Vec::new()));

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let language = rust_lang();
    language_registry.add(language);

    let closure_ranges_fetched = lsp_request_ranges.clone();
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new(move |fake_server| {
                let closure_ranges_fetched = closure_ranges_fetched.clone();
                fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                    move |params, _| {
                        let closure_ranges_fetched = closure_ranges_fetched.clone();
                        async move {
                            let prefix = if params.text_document.uri
                                == lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap()
                            {
                                closure_ranges_fetched
                                    .lock()
                                    .push(("main.rs", params.range));
                                "main.rs"
                            } else if params.text_document.uri
                                == lsp::Uri::from_file_path(path!("/a/lib.rs")).unwrap()
                            {
                                closure_ranges_fetched.lock().push(("lib.rs", params.range));
                                "lib.rs"
                            } else {
                                panic!("Unexpected file path {:?}", params.text_document.uri);
                            };
                            Ok(Some(
                                (params.range.start.line..params.range.end.line)
                                    .map(|row| lsp::InlayHint {
                                        position: lsp::Position::new(row, 0),
                                        label: lsp::InlayHintLabel::String(format!(
                                            "{prefix} Inlay hint #{row}"
                                        )),
                                        kind: Some(lsp::InlayHintKind::TYPE),
                                        text_edits: None,
                                        tooltip: None,
                                        padding_left: None,
                                        padding_right: None,
                                        data: None,
                                    })
                                    .collect(),
                            ))
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
            buffer_1.clone(),
            [
                Point::new(49, 0)..Point::new(53, 0),
                Point::new(70, 0)..Point::new(73, 0),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(4, 0)],
            0,
            cx,
        );
        multibuffer
    });

    let editor = cx.add_window(|window, cx| {
        let mut editor = Editor::for_multibuffer(multi_buffer, Some(project.clone()), window, cx);
        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(0)..MultiBufferOffset(0)])
        });
        editor
    });

    let _fake_server = fake_servers.next().await.unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();

    assert_eq!(
        vec![
            (
                "lib.rs",
                lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(2, 10))
            ),
            (
                "main.rs",
                lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(50, 0))
            ),
            (
                "main.rs",
                lsp::Range::new(lsp::Position::new(50, 0), lsp::Position::new(100, 0))
            ),
        ],
        lsp_request_ranges
            .lock()
            .drain(..)
            .sorted_by_key(|(prefix, r)| (prefix.to_owned(), r.start))
            .collect::<Vec<_>>(),
        "For large buffers, should query chunks that cover both visible excerpt"
    );
    editor
        .update(cx, |editor, _window, cx| {
            assert_eq!(
                (0..2)
                    .map(|i| format!("lib.rs Inlay hint #{i}"))
                    .chain((0..100).map(|i| format!("main.rs Inlay hint #{i}")))
                    .collect::<Vec<_>>(),
                sorted_cached_hint_labels(editor, cx),
                "Both chunks should provide their inlay hints"
            );
            assert_eq!(
                vec![
                    "main.rs Inlay hint #49".to_owned(),
                    "main.rs Inlay hint #50".to_owned(),
                    "main.rs Inlay hint #51".to_owned(),
                    "main.rs Inlay hint #52".to_owned(),
                    "main.rs Inlay hint #53".to_owned(),
                    "main.rs Inlay hint #70".to_owned(),
                    "main.rs Inlay hint #71".to_owned(),
                    "main.rs Inlay hint #72".to_owned(),
                    "main.rs Inlay hint #73".to_owned(),
                    "lib.rs Inlay hint #0".to_owned(),
                    "lib.rs Inlay hint #1".to_owned(),
                ],
                visible_hint_labels(editor, cx),
                "Only hints from visible excerpt should be added into the editor"
            );
        })
        .unwrap();

    editor
        .update(cx, |editor, window, cx| {
            editor.handle_input("a", window, cx);
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(1000));
    cx.executor().run_until_parked();
    assert_eq!(
        vec![
            (
                "lib.rs",
                lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(2, 10))
            ),
            (
                "main.rs",
                lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(50, 0))
            ),
            (
                "main.rs",
                lsp::Range::new(lsp::Position::new(50, 0), lsp::Position::new(100, 0))
            ),
        ],
        lsp_request_ranges
            .lock()
            .drain(..)
            .sorted_by_key(|(prefix, r)| (prefix.to_owned(), r.start))
            .collect::<Vec<_>>(),
        "Same chunks should be re-queried on edit"
    );
    editor
        .update(cx, |editor, _window, cx| {
            assert_eq!(
                (0..2)
                    .map(|i| format!("lib.rs Inlay hint #{i}"))
                    .chain((0..100).map(|i| format!("main.rs Inlay hint #{i}")))
                    .collect::<Vec<_>>(),
                sorted_cached_hint_labels(editor, cx),
                "Same hints should be re-inserted after the edit"
            );
            assert_eq!(
                vec![
                    "main.rs Inlay hint #49".to_owned(),
                    "main.rs Inlay hint #50".to_owned(),
                    "main.rs Inlay hint #51".to_owned(),
                    "main.rs Inlay hint #52".to_owned(),
                    "main.rs Inlay hint #53".to_owned(),
                    "main.rs Inlay hint #70".to_owned(),
                    "main.rs Inlay hint #71".to_owned(),
                    "main.rs Inlay hint #72".to_owned(),
                    "main.rs Inlay hint #73".to_owned(),
                    "lib.rs Inlay hint #0".to_owned(),
                    "lib.rs Inlay hint #1".to_owned(),
                ],
                visible_hint_labels(editor, cx),
                "Same hints should be re-inserted into the editor after the edit"
            );
        })
        .unwrap();
}
