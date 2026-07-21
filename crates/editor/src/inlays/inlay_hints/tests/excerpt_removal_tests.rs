use super::*;

#[gpui::test]
async fn test_excerpts_removed(cx: &mut gpui::TestAppContext) {
    init_test(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            show_value_hints: Some(true),
            enabled: Some(true),
            edit_debounce_ms: Some(0),
            scroll_debounce_ms: Some(0),
            show_type_hints: Some(false),
            show_parameter_hints: Some(false),
            show_other_hints: Some(false),
            show_background: Some(false),
            toggle_on_modifiers_press: None,
        })
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": format!("fn main() {{\n{}\n}}", (0..501).map(|i| format!("let i = {i};\n")).collect::<String>()),
            "other.rs": format!("fn main() {{\n{}\n}}", (0..501).map(|j| format!("let j = {j};\n")).collect::<String>()),
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            ..FakeLspAdapter::default()
        },
    );

    let (buffer_1, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let (buffer_2, _handle2) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/a/other.rs"), cx)
        })
        .await
        .unwrap();
    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(2, 0)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 1)..Point::new(2, 1)],
            0,
            cx,
        );
    });

    cx.executor().run_until_parked();
    let editor = cx.add_window(|window, cx| {
        Editor::for_multibuffer(multibuffer, Some(project.clone()), window, cx)
    });
    let editor_edited = Arc::new(AtomicBool::new(false));
    let fake_server = fake_servers.next().await.unwrap();
    let closure_editor_edited = Arc::clone(&editor_edited);
    fake_server
        .set_request_handler::<lsp::request::InlayHintRequest, _, _>(move |params, _| {
            let task_editor_edited = Arc::clone(&closure_editor_edited);
            async move {
                let hint_text = if params.text_document.uri
                    == lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap()
                {
                    "main hint"
                } else if params.text_document.uri
                    == lsp::Uri::from_file_path(path!("/a/other.rs")).unwrap()
                {
                    "other hint"
                } else {
                    panic!("unexpected uri: {:?}", params.text_document.uri);
                };

                let positions = [
                    lsp::Position::new(0, 2),
                    lsp::Position::new(4, 2),
                    lsp::Position::new(22, 2),
                    lsp::Position::new(44, 2),
                    lsp::Position::new(56, 2),
                    lsp::Position::new(67, 2),
                ];
                let out_of_range_hint = lsp::InlayHint {
                    position: lsp::Position::new(
                        params.range.start.line + 99,
                        params.range.start.character + 99,
                    ),
                    label: lsp::InlayHintLabel::String(
                        "out of excerpt range, should be ignored".to_string(),
                    ),
                    kind: None,
                    text_edits: None,
                    tooltip: None,
                    padding_left: None,
                    padding_right: None,
                    data: None,
                };

                let edited = task_editor_edited.load(Ordering::Acquire);
                Ok(Some(
                    std::iter::once(out_of_range_hint)
                        .chain(positions.into_iter().enumerate().map(|(i, position)| {
                            lsp::InlayHint {
                                position,
                                label: lsp::InlayHintLabel::String(format!(
                                    "{hint_text}{} #{i}",
                                    if edited { "(edited)" } else { "" },
                                )),
                                kind: None,
                                text_edits: None,
                                tooltip: None,
                                padding_left: None,
                                padding_right: None,
                                data: None,
                            }
                        }))
                        .collect(),
                ))
            }
        })
        .next()
        .await;
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                vec![
                    "main hint #0".to_string(),
                    "main hint #1".to_string(),
                    "main hint #2".to_string(),
                    "main hint #3".to_string(),
                    "other hint #0".to_string(),
                    "other hint #1".to_string(),
                    "other hint #2".to_string(),
                    "other hint #3".to_string(),
                ],
                sorted_cached_hint_labels(editor, cx),
                "Cache should update for both excerpts despite hints display was disabled; after selecting 2nd buffer, it's now registered with the langserever and should get its hints"
            );
            assert_eq!(
                Vec::<String>::new(),
                visible_hint_labels(editor, cx),
                "All hints are disabled and should not be shown despite being present in the cache"
            );
        })
        .unwrap();

    editor
        .update(cx, |editor, _, cx| {
            editor.buffer().update(cx, |multibuffer, cx| {
                multibuffer.remove_excerpts(PathKey::sorted(1), cx);
            })
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _, cx| {
            assert_eq!(
                vec![
                    "main hint #0".to_string(),
                    "main hint #1".to_string(),
                    "main hint #2".to_string(),
                    "main hint #3".to_string(),
                ],
                cached_hint_labels(editor, cx),
                "For the removed excerpt, should clean corresponding cached hints as its buffer was dropped"
            );
            assert!(
            visible_hint_labels(editor, cx).is_empty(),
            "All hints are disabled and should not be shown despite being present in the cache"
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
            assert_eq!(
                vec![
                    "main hint #0".to_string(),
                    "main hint #1".to_string(),
                    "main hint #2".to_string(),
                    "main hint #3".to_string(),
                ],
                cached_hint_labels(editor, cx),
                "Hint display settings change should not change the cache"
            );
            assert_eq!(
                vec![
                    "main hint #0".to_string(),
                ],
                visible_hint_labels(editor, cx),
                "Settings change should make cached hints visible, but only the visible ones, from the remaining excerpt"
            );
        })
        .unwrap();
}
