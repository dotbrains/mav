use super::*;

#[gpui::test]
async fn test_race_in_multibuffer_save(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            "first.rs": "# First Document\nSome content here.",
            "second.rs": "Plain text content for second file.",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let language = rust_lang();
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(language.clone());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            ..FakeLspAdapter::default()
        },
    );

    let buffer1 = project
        .update(cx, |project, cx| {
            project.open_local_buffer(PathBuf::from(path!("/project/first.rs")), cx)
        })
        .await
        .unwrap();
    let buffer2 = project
        .update(cx, |project, cx| {
            project.open_local_buffer(PathBuf::from(path!("/project/second.rs")), cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(Capability::ReadWrite);
        multi_buffer.set_excerpts_for_path(
            PathKey::for_buffer(&buffer1, cx),
            buffer1.clone(),
            [Point::zero()..buffer1.read(cx).max_point()],
            3,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::for_buffer(&buffer2, cx),
            buffer2.clone(),
            [Point::zero()..buffer1.read(cx).max_point()],
            3,
            cx,
        );
        multi_buffer
    });

    let (editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });

    let fake_language_server = fake_servers.next().await.unwrap();

    buffer1.update(cx, |buffer, cx| buffer.edit([(0..0, "hello!")], None, cx));

    let save = editor.update_in(cx, |editor, window, cx| {
        assert!(editor.is_dirty(cx));

        editor.save(
            SaveOptions {
                format: true,
                force_format: false,
                autosave: true,
            },
            project,
            window,
            cx,
        )
    });
    let (start_edit_tx, start_edit_rx) = oneshot::channel();
    let (done_edit_tx, done_edit_rx) = oneshot::channel();
    let mut done_edit_rx = Some(done_edit_rx);
    let mut start_edit_tx = Some(start_edit_tx);

    fake_language_server.set_request_handler::<lsp::request::Formatting, _, _>(move |_, _| {
        start_edit_tx.take().unwrap().send(()).unwrap();
        let done_edit_rx = done_edit_rx.take().unwrap();
        async move {
            done_edit_rx.await.unwrap();
            Ok(None)
        }
    });

    start_edit_rx.await.unwrap();
    buffer2
        .update(cx, |buffer, cx| buffer.edit([(0..0, "world!")], None, cx))
        .unwrap();

    done_edit_tx.send(()).unwrap();

    save.await.unwrap();
    cx.update(|_, cx| assert!(editor.is_dirty(cx)));
}

#[gpui::test]
fn test_duplicate_line_up_on_last_line_without_newline(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("line1\nline2", cx);
        build_editor(buffer, window, cx)
    });

    editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_display_ranges([
                    DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0)
                ])
            });

            editor.duplicate_line_up(&DuplicateLineUp, window, cx);

            assert_eq!(
                editor.display_text(cx),
                "line1\nline2\nline2",
                "Duplicating last line upward should create duplicate above, not on same line"
            );

            assert_eq!(
                editor
                    .selections
                    .display_ranges(&editor.display_snapshot(cx)),
                vec![DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)],
                "Selection should move to the duplicated line"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_copy_line_without_trailing_newline(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state("line1\nline2ˇ");

    cx.update_editor(|e, window, cx| e.copy(&Copy, window, cx));

    let clipboard_text = cx
        .read_from_clipboard()
        .and_then(|item| item.text().as_deref().map(str::to_string));

    assert_eq!(
        clipboard_text,
        Some("line2\n".to_string()),
        "Copying a line without trailing newline should include a newline"
    );

    cx.set_state("line1\nˇ");

    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));

    cx.assert_editor_state("line1\nline2\nˇ");
}

#[gpui::test]
async fn test_multi_selection_copy_with_newline_between_copied_lines(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state("ˇline1\nˇline2\nˇline3\n");

    cx.update_editor(|e, window, cx| e.copy(&Copy, window, cx));

    let clipboard_text = cx
        .read_from_clipboard()
        .and_then(|item| item.text().as_deref().map(str::to_string));

    assert_eq!(
        clipboard_text,
        Some("line1\nline2\nline3\n".to_string()),
        "Copying multiple lines should include a single newline between lines"
    );

    cx.set_state("lineA\nˇ");

    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));

    cx.assert_editor_state("lineA\nline1\nline2\nline3\nˇ");
}

#[gpui::test]
async fn test_multi_selection_cut_with_newline_between_copied_lines(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state("ˇline1\nˇline2\nˇline3\n");

    cx.update_editor(|e, window, cx| e.cut(&Cut, window, cx));

    let clipboard_text = cx
        .read_from_clipboard()
        .and_then(|item| item.text().as_deref().map(str::to_string));

    assert_eq!(
        clipboard_text,
        Some("line1\nline2\nline3\n".to_string()),
        "Copying multiple lines should include a single newline between lines"
    );

    cx.set_state("lineA\nˇ");

    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));

    cx.assert_editor_state("lineA\nline1\nline2\nline3\nˇ");
}

#[gpui::test]
async fn test_end_of_editor_context(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state("line1\nline2ˇ");
    cx.update_editor(|e, window, cx| {
        e.set_mode(EditorMode::SingleLine);
        assert!(!e.key_context(window, cx).contains("start_of_input"));
        assert!(e.key_context(window, cx).contains("end_of_input"));
    });
    cx.set_state("ˇline1\nline2");
    cx.update_editor(|e, window, cx| {
        e.set_mode(EditorMode::SingleLine);
        assert!(e.key_context(window, cx).contains("start_of_input"));
        assert!(!e.key_context(window, cx).contains("end_of_input"));
    });
    cx.set_state("line1ˇ\nline2");
    cx.update_editor(|e, window, cx| {
        e.set_mode(EditorMode::SingleLine);
        assert!(!e.key_context(window, cx).contains("start_of_input"));
        assert!(!e.key_context(window, cx).contains("end_of_input"));
    });

    cx.set_state("line1\nline2ˇ");
    cx.update_editor(|e, window, cx| {
        e.set_mode(EditorMode::AutoHeight {
            min_lines: 1,
            max_lines: Some(4),
        });
        assert!(!e.key_context(window, cx).contains("start_of_input"));
        assert!(e.key_context(window, cx).contains("end_of_input"));
    });
    cx.set_state("ˇline1\nline2");
    cx.update_editor(|e, window, cx| {
        e.set_mode(EditorMode::AutoHeight {
            min_lines: 1,
            max_lines: Some(4),
        });
        assert!(e.key_context(window, cx).contains("start_of_input"));
        assert!(!e.key_context(window, cx).contains("end_of_input"));
    });
    cx.set_state("line1ˇ\nline2");
    cx.update_editor(|e, window, cx| {
        e.set_mode(EditorMode::AutoHeight {
            min_lines: 1,
            max_lines: Some(4),
        });
        assert!(!e.key_context(window, cx).contains("start_of_input"));
        assert!(!e.key_context(window, cx).contains("end_of_input"));
    });
}
