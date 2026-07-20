use super::*;

#[gpui::test]
async fn test_copy_trim_line_mode(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        «    fn main() {
                1
            }ˇ»
    "});
    cx.update_editor(|editor, _window, _cx| editor.selections.set_line_mode(true));
    cx.update_editor(|editor, window, cx| editor.copy_and_trim(&CopyAndTrim, window, cx));

    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("fn main() {\n    1\n}\n".to_string())
    );

    let clipboard_selections: Vec<ClipboardSelection> = cx
        .read_from_clipboard()
        .and_then(|item| item.entries().first().cloned())
        .and_then(|entry| match entry {
            gpui::ClipboardEntry::String(text) => text.metadata_json(),
            _ => None,
        })
        .expect("should have clipboard selections");

    assert_eq!(clipboard_selections.len(), 1);
    assert!(clipboard_selections[0].is_entire_line);

    cx.set_state(indoc! {"
        «fn main() {
            1
        }ˇ»
    "});
    cx.update_editor(|editor, _window, _cx| editor.selections.set_line_mode(true));
    cx.update_editor(|editor, window, cx| editor.copy_and_trim(&CopyAndTrim, window, cx));

    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("fn main() {\n    1\n}\n".to_string())
    );

    let clipboard_selections: Vec<ClipboardSelection> = cx
        .read_from_clipboard()
        .and_then(|item| item.entries().first().cloned())
        .and_then(|entry| match entry {
            gpui::ClipboardEntry::String(text) => text.metadata_json(),
            _ => None,
        })
        .expect("should have clipboard selections");

    assert_eq!(clipboard_selections.len(), 1);
    assert!(clipboard_selections[0].is_entire_line);
}

#[gpui::test]
async fn test_clipboard_line_numbers_from_multibuffer(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(
        path!("/file.txt"),
        "first line\nsecond line\nthird line\nfourth line\nfifth line\n".into(),
    )
    .await;

    let project = Project::test(fs, [path!("/file.txt").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/file.txt"), cx)
        })
        .await
        .unwrap();

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer.clone(),
            [Point::new(2, 0)..Point::new(5, 0)],
            0,
            cx,
        );
        multibuffer
    });

    let (editor, cx) = cx.add_window_view(|window, cx| {
        build_editor_with_project(project.clone(), multibuffer, window, cx)
    });

    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(editor.text(cx), "third line\nfourth line\nfifth line\n");

        editor.select_all(&SelectAll, window, cx);
        editor.copy(&Copy, window, cx);
    });

    let clipboard_selections: Option<Vec<ClipboardSelection>> = cx
        .read_from_clipboard()
        .and_then(|item| item.entries().first().cloned())
        .and_then(|entry| match entry {
            gpui::ClipboardEntry::String(text) => text.metadata_json(),
            _ => None,
        });

    let selections = clipboard_selections.expect("should have clipboard selections");
    assert_eq!(selections.len(), 1);
    let selection = &selections[0];
    assert_eq!(
        selection.line_range,
        Some(2..=5),
        "line range should be from original file (rows 2-5), not multibuffer rows (0-2)"
    );
}

#[gpui::test]
async fn test_paste_multiline(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(rust_lang()), cx));

    // Cut an indented block, without the leading whitespace.
    cx.set_state(indoc! {"
        const a: B = (
            c(),
            «d(
                e,
                f
            )ˇ»
        );
    "});
    cx.update_editor(|e, window, cx| e.cut(&Cut, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: B = (
            c(),
            ˇ
        );
    "});

    // Paste it at the same position.
    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: B = (
            c(),
            d(
                e,
                f
            )ˇ
        );
    "});

    // Paste it at a line with a lower indent level.
    cx.set_state(indoc! {"
        ˇ
        const a: B = (
            c(),
        );
    "});
    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state(indoc! {"
        d(
            e,
            f
        )ˇ
        const a: B = (
            c(),
        );
    "});

    // Cut an indented block, with the leading whitespace.
    cx.set_state(indoc! {"
        const a: B = (
            c(),
        «    d(
                e,
                f
            )
        ˇ»);
    "});
    cx.update_editor(|e, window, cx| e.cut(&Cut, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: B = (
            c(),
        ˇ);
    "});

    // Paste it at the same position.
    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: B = (
            c(),
            d(
                e,
                f
            )
        ˇ);
    "});

    // Paste it at a line with a higher indent level.
    cx.set_state(indoc! {"
        const a: B = (
            c(),
            d(
                e,
                fˇ
            )
        );
    "});
    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: B = (
            c(),
            d(
                e,
                f    d(
                    e,
                    f
                )
        ˇ
            )
        );
    "});

    // Copy an indented block, starting mid-line
    cx.set_state(indoc! {"
        const a: B = (
            c(),
            somethin«g(
                e,
                f
            )ˇ»
        );
    "});
    cx.update_editor(|e, window, cx| e.copy(&Copy, window, cx));

    // Paste it on a line with a lower indent level
    cx.update_editor(|e, window, cx| e.move_to_end(&Default::default(), window, cx));
    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: B = (
            c(),
            something(
                e,
                f
            )
        );
        g(
            e,
            f
        )ˇ"});
}

#[gpui::test]
async fn test_paste_undo_does_not_include_preceding_edits(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.update_editor(|e, _, cx| {
        e.buffer().update(cx, |buffer, cx| {
            buffer.set_group_interval(Duration::from_secs(10), cx)
        })
    });
    // Type some text
    cx.set_state("ˇ");
    cx.update_editor(|e, window, cx| e.insert("hello", window, cx));
    // cx.assert_editor_state("helloˇ");

    // Paste some text immediately after typing
    cx.write_to_clipboard(ClipboardItem::new_string(" world".into()));
    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state("hello worldˇ");

    // Undo should only undo the paste, not the preceding typing
    cx.update_editor(|e, window, cx| e.undo(&Undo, window, cx));
    cx.assert_editor_state("helloˇ");

    // Undo again should undo the typing
    cx.update_editor(|e, window, cx| e.undo(&Undo, window, cx));
    cx.assert_editor_state("ˇ");
}

#[gpui::test]
async fn test_paste_content_from_other_app(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    cx.write_to_clipboard(ClipboardItem::new_string(
        "    d(\n        e\n    );\n".into(),
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(rust_lang()), cx));
    cx.run_until_parked();

    cx.set_state(indoc! {"
        fn a() {
            b();
            if c() {
                ˇ
            }
        }
    "});

    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state(indoc! {"
        fn a() {
            b();
            if c() {
                d(
                    e
                );
        ˇ
            }
        }
    "});

    cx.set_state(indoc! {"
        fn a() {
            b();
            ˇ
        }
    "});

    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state(indoc! {"
        fn a() {
            b();
            d(
                e
            );
        ˇ
        }
    "});
}

#[gpui::test]
async fn test_paste_multiline_from_other_app_into_matching_cursors(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    cx.write_to_clipboard(ClipboardItem::new_string("alpha\nbeta\ngamma".into()));

    let mut cx = EditorTestContext::new(cx).await;

    // Paste into 3 cursors: each cursor should receive one line.
    cx.set_state("ˇ one ˇ two ˇ three");
    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state("alphaˇ one betaˇ two gammaˇ three");

    // Paste into 2 cursors: line count doesn't match, so paste entire text at each cursor.
    cx.write_to_clipboard(ClipboardItem::new_string("alpha\nbeta\ngamma".into()));
    cx.set_state("ˇ one ˇ two");
    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state("alpha\nbeta\ngammaˇ one alpha\nbeta\ngammaˇ two");

    // Paste into a single cursor: should paste everything as-is.
    cx.write_to_clipboard(ClipboardItem::new_string("alpha\nbeta\ngamma".into()));
    cx.set_state("ˇ one");
    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state("alpha\nbeta\ngammaˇ one");

    // Paste with selections: each selection is replaced with its corresponding line.
    cx.write_to_clipboard(ClipboardItem::new_string("xx\nyy\nzz".into()));
    cx.set_state("«aˇ» one «bˇ» two «cˇ» three");
    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));
    cx.assert_editor_state("xxˇ one yyˇ two zzˇ three");
}

#[gpui::test]
fn test_select_all(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("abc\nde\nfgh", cx);
        build_editor(buffer, window, cx)
    });
    _ = editor.update(cx, |editor, window, cx| {
        editor.select_all(&SelectAll, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(2), 3)]
        );
    });
}

#[gpui::test]
fn test_select_line(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(&sample_text(6, 5, 'a'), cx);
        build_editor(buffer, window, cx)
    });
    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 2),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0),
                DisplayPoint::new(DisplayRow(4), 2)..DisplayPoint::new(DisplayRow(4), 2),
            ])
        });
        editor.select_line(&SelectLine, window, cx);
        // Adjacent line selections should NOT merge (only overlapping ones do)
        assert_eq!(
            display_ranges(editor, cx),
            vec![
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(1), 0),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(2), 0),
                DisplayPoint::new(DisplayRow(4), 0)..DisplayPoint::new(DisplayRow(5), 0),
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.select_line(&SelectLine, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            vec![
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(3), 0),
                DisplayPoint::new(DisplayRow(4), 0)..DisplayPoint::new(DisplayRow(5), 5),
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.select_line(&SelectLine, window, cx);
        // Adjacent but not overlapping, so they stay separate
        assert_eq!(
            display_ranges(editor, cx),
            vec![
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(4), 0),
                DisplayPoint::new(DisplayRow(4), 0)..DisplayPoint::new(DisplayRow(5), 5),
            ]
        );
    });
}
