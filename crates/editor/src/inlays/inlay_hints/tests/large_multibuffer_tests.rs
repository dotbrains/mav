use super::*;

#[gpui::test]
async fn test_multiple_excerpts_large_multibuffer(cx: &mut gpui::TestAppContext) {
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
                "main.rs": format!("fn main() {{\n{}\n}}", (0..501).map(|i| format!("let i = {i};\n")).collect::<String>()),
                "other.rs": format!("fn main() {{\n{}\n}}", (0..501).map(|j| format!("let j = {j};\n")).collect::<String>()),
            }),
        )
        .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let language = rust_lang();
    language_registry.add(language);
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

    let (buffer_1, _handle1) = project
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
    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(4, 0)..Point::new(11, 0),
                Point::new(22, 0)..Point::new(33, 0),
                Point::new(44, 0)..Point::new(55, 0),
                Point::new(56, 0)..Point::new(66, 0),
                Point::new(67, 0)..Point::new(77, 0),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [
                Point::new(0, 1)..Point::new(2, 1),
                Point::new(4, 1)..Point::new(11, 1),
                Point::new(22, 1)..Point::new(33, 1),
                Point::new(44, 1)..Point::new(55, 1),
                Point::new(56, 1)..Point::new(66, 1),
                Point::new(67, 1)..Point::new(77, 1),
            ],
            0,
            cx,
        );
        multibuffer
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

                // one hint per excerpt
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
                                    "{hint_text}{E} #{i}",
                                    E = if edited { "(edited)" } else { "" },
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
    cx.executor().run_until_parked();

    editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec![
                "main hint #0".to_string(),
                "main hint #1".to_string(),
                "main hint #2".to_string(),
                "main hint #3".to_string(),
                "main hint #4".to_string(),
                "main hint #5".to_string(),
            ];
            assert_eq!(
                expected_hints,
                sorted_cached_hint_labels(editor, cx),
                "When scroll is at the edge of a multibuffer, its visible excerpts only should be queried for inlay hints"
            );
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();

    editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(
                SelectionEffects::scroll(Autoscroll::Next),
                window,
                cx,
                |s| s.select_ranges([Point::new(4, 0)..Point::new(4, 0)]),
            );
            editor.change_selections(
                SelectionEffects::scroll(Autoscroll::Next),
                window,
                cx,
                |s| s.select_ranges([Point::new(22, 0)..Point::new(22, 0)]),
            );
            editor.change_selections(
                SelectionEffects::scroll(Autoscroll::Next),
                window,
                cx,
                |s| s.select_ranges([Point::new(57, 0)..Point::new(57, 0)]),
            );
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec![
                "main hint #0".to_string(),
                "main hint #1".to_string(),
                "main hint #2".to_string(),
                "main hint #3".to_string(),
                "main hint #4".to_string(),
                "main hint #5".to_string(),
            ];
            assert_eq!(expected_hints, sorted_cached_hint_labels(editor, cx),
                "New hints are not shown right after scrolling, we need to wait for the buffer to be registered");
            assert_eq!(expected_hints, visible_hint_labels(editor, cx));
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec![
                "main hint #0".to_string(),
                "main hint #1".to_string(),
                "main hint #2".to_string(),
                "main hint #3".to_string(),
                "main hint #4".to_string(),
                "main hint #5".to_string(),
                "other hint #0".to_string(),
                "other hint #1".to_string(),
                "other hint #2".to_string(),
                "other hint #3".to_string(),
            ];
            assert_eq!(
                expected_hints,
                sorted_cached_hint_labels(editor, cx),
                "After scrolling to the new buffer and waiting for it to be registered, new hints should appear");
            assert_eq!(
                expected_hints,
                visible_hint_labels(editor, cx),
                "Editor should show only visible hints",
            );
        })
        .unwrap();

    editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(
                SelectionEffects::scroll(Autoscroll::Next),
                window,
                cx,
                |s| s.select_ranges([Point::new(100, 0)..Point::new(100, 0)]),
            );
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec![
                "main hint #0".to_string(),
                "main hint #1".to_string(),
                "main hint #2".to_string(),
                "main hint #3".to_string(),
                "main hint #4".to_string(),
                "main hint #5".to_string(),
                "other hint #0".to_string(),
                "other hint #1".to_string(),
                "other hint #2".to_string(),
                "other hint #3".to_string(),
                "other hint #4".to_string(),
                "other hint #5".to_string(),
            ];
            assert_eq!(
                expected_hints,
                sorted_cached_hint_labels(editor, cx),
                "After multibuffer was scrolled to the end, all hints for all excerpts should be fetched"
            );
            assert_eq!(
                expected_hints,
                visible_hint_labels(editor, cx),
                "Editor shows only hints for excerpts that were visible when scrolling"
            );
        })
        .unwrap();

    editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(
                SelectionEffects::scroll(Autoscroll::Next),
                window,
                cx,
                |s| s.select_ranges([Point::new(4, 0)..Point::new(4, 0)]),
            );
        })
        .unwrap();
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec![
                "main hint #0".to_string(),
                "main hint #1".to_string(),
                "main hint #2".to_string(),
                "main hint #3".to_string(),
                "main hint #4".to_string(),
                "main hint #5".to_string(),
                "other hint #0".to_string(),
                "other hint #1".to_string(),
                "other hint #2".to_string(),
                "other hint #3".to_string(),
                "other hint #4".to_string(),
                "other hint #5".to_string(),
            ];
            assert_eq!(
                expected_hints,
                sorted_cached_hint_labels(editor, cx),
                "After multibuffer was scrolled to the end, further scrolls up should not bring more hints"
            );
            assert_eq!(
                expected_hints,
                visible_hint_labels(editor, cx),
            );
        })
        .unwrap();

    // We prepare to change the scrolling on edit, but do not scroll yet
    editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_ranges([Point::new(57, 0)..Point::new(57, 0)])
            });
        })
        .unwrap();
    cx.executor().run_until_parked();
    // Edit triggers the scrolling too
    editor_edited.store(true, Ordering::Release);
    editor
        .update(cx, |editor, window, cx| {
            editor.handle_input("++++more text++++", window, cx);
        })
        .unwrap();
    cx.executor().run_until_parked();
    // Wait again to trigger the inlay hints fetch on scroll
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            let expected_hints = vec![
                "main hint(edited) #0".to_string(),
                "main hint(edited) #1".to_string(),
                "main hint(edited) #2".to_string(),
                "main hint(edited) #3".to_string(),
                "main hint(edited) #4".to_string(),
                "main hint(edited) #5".to_string(),
                "other hint(edited) #0".to_string(),
                "other hint(edited) #1".to_string(),
                "other hint(edited) #2".to_string(),
                "other hint(edited) #3".to_string(),
            ];
            assert_eq!(
                expected_hints,
                sorted_cached_hint_labels(editor, cx),
                "After multibuffer edit, editor gets scrolled back to the last selection; \
            all hints should be invalidated and required for all of its visible excerpts"
            );
            assert_eq!(
                expected_hints,
                visible_hint_labels(editor, cx),
                "All excerpts should get their hints"
            );
        })
        .unwrap();
}
