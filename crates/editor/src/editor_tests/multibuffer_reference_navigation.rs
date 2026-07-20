use super::*;

#[gpui::test]
async fn test_multibuffer_selections_with_folding(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let (editor, cx) = cx.add_window_view(|window, cx| {
        let multi_buffer = MultiBuffer::build_multi(
            [
                ("1\n2\n3\n", vec![Point::row_range(0..3)]),
                ("1\n2\n3\n", vec![Point::row_range(0..3)]),
            ],
            cx,
        );
        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    let mut cx = EditorTestContext::for_editor_in(editor.clone(), cx).await;
    let buffer_ids = cx.multibuffer(|mb, cx| {
        mb.snapshot(cx)
            .excerpts()
            .map(|excerpt| excerpt.context.start.buffer_id)
            .collect::<Vec<_>>()
    });

    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        ˇ1
        2
        3
        [EXCERPT]
        1
        2
        3
        "});

    // Scenario 1: Unfolded buffers, position cursor on "2", select all matches, then insert
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(None.into(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(2)..MultiBufferOffset(3)]);
        });
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        2ˇ
        3
        [EXCERPT]
        1
        2
        3
        "});

    cx.update_editor(|editor, window, cx| {
        editor
            .select_all_matches(&SelectAllMatches, window, cx)
            .unwrap();
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        2ˇ
        3
        [EXCERPT]
        1
        2ˇ
        3
        "});

    cx.update_editor(|editor, window, cx| {
        editor.handle_input("X", window, cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        Xˇ
        3
        [EXCERPT]
        1
        Xˇ
        3
        "});

    // Scenario 2: Select "2", then fold second buffer before insertion
    cx.update_multibuffer(|mb, cx| {
        for buffer_id in buffer_ids.iter() {
            let buffer = mb.buffer(*buffer_id).unwrap();
            buffer.update(cx, |buffer, cx| {
                buffer.edit([(0..buffer.len(), "1\n2\n3\n")], None, cx);
            });
        }
    });

    // Select "2" and select all matches
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(None.into(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(2)..MultiBufferOffset(3)]);
        });
        editor
            .select_all_matches(&SelectAllMatches, window, cx)
            .unwrap();
    });

    // Fold second buffer - should remove selections from folded buffer
    cx.update_editor(|editor, _, cx| {
        editor.fold_buffer(buffer_ids[1], cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        2ˇ
        3
        [EXCERPT]
        [FOLDED]
        "});

    // Insert text - should only affect first buffer
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("Y", window, cx);
    });
    cx.update_editor(|editor, _, cx| {
        editor.unfold_buffer(buffer_ids[1], cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        Yˇ
        3
        [EXCERPT]
        1
        2
        3
        "});

    // Scenario 3: Select "2", then fold first buffer before insertion
    cx.update_multibuffer(|mb, cx| {
        for buffer_id in buffer_ids.iter() {
            let buffer = mb.buffer(*buffer_id).unwrap();
            buffer.update(cx, |buffer, cx| {
                buffer.edit([(0..buffer.len(), "1\n2\n3\n")], None, cx);
            });
        }
    });

    // Select "2" and select all matches
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(None.into(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(2)..MultiBufferOffset(3)]);
        });
        editor
            .select_all_matches(&SelectAllMatches, window, cx)
            .unwrap();
    });

    // Fold first buffer - should remove selections from folded buffer
    cx.update_editor(|editor, _, cx| {
        editor.fold_buffer(buffer_ids[0], cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        1
        2ˇ
        3
        "});

    // Insert text - should only affect second buffer
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("Z", window, cx);
    });
    cx.update_editor(|editor, _, cx| {
        editor.unfold_buffer(buffer_ids[0], cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        2
        3
        [EXCERPT]
        1
        Zˇ
        3
        "});

    // Test correct folded header is selected upon fold
    cx.update_editor(|editor, _, cx| {
        editor.fold_buffer(buffer_ids[0], cx);
        editor.fold_buffer(buffer_ids[1], cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        ˇ[FOLDED]
        "});

    // Test selection inside folded buffer unfolds it on type
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("W", window, cx);
    });
    cx.update_editor(|editor, _, cx| {
        editor.unfold_buffer(buffer_ids[0], cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        2
        3
        [EXCERPT]
        Wˇ1
        Z
        3
        "});
}

#[gpui::test]
async fn test_multibuffer_scroll_cursor_top_margin(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let (editor, cx) = cx.add_window_view(|window, cx| {
        let multi_buffer = MultiBuffer::build_multi(
            [
                ("1\n2\n3\n", vec![Point::row_range(0..3)]),
                ("1\n2\n3\n4\n5\n6\n7\n8\n9\n", vec![Point::row_range(0..9)]),
            ],
            cx,
        );
        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    let mut cx = EditorTestContext::for_editor_in(editor.clone(), cx).await;

    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        ˇ1
        2
        3
        [EXCERPT]
        1
        2
        3
        4
        5
        6
        7
        8
        9
        "});

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(None.into(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(19)..MultiBufferOffset(19)]);
        });
    });

    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        2
        3
        [EXCERPT]
        1
        2
        3
        4
        5
        6
        ˇ7
        8
        9
        "});

    cx.update_editor(|editor, _window, cx| {
        editor.set_vertical_scroll_margin(0, cx);
    });

    cx.update_editor(|editor, window, cx| {
        assert_eq!(editor.vertical_scroll_margin(), 0);
        editor.scroll_cursor_top(&ScrollCursorTop, window, cx);
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 12.0)
        );
    });

    cx.update_editor(|editor, _window, cx| {
        editor.set_vertical_scroll_margin(3, cx);
    });

    cx.update_editor(|editor, window, cx| {
        assert_eq!(editor.vertical_scroll_margin(), 3);
        editor.scroll_cursor_top(&ScrollCursorTop, window, cx);
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 9.0)
        );
    });
}

#[gpui::test]
async fn test_find_references_single_case(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            references_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let before = indoc!(
        r#"
        fn main() {
            let aˇbc = 123;
            let xyz = abc;
        }
        "#
    );
    let after = indoc!(
        r#"
        fn main() {
            let abc = 123;
            let xyz = ˇabc;
        }
        "#
    );

    cx.lsp
        .set_request_handler::<lsp::request::References, _, _>(async move |params, _| {
            Ok(Some(vec![
                lsp::Location {
                    uri: params.text_document_position.text_document.uri.clone(),
                    range: lsp::Range::new(lsp::Position::new(1, 8), lsp::Position::new(1, 11)),
                },
                lsp::Location {
                    uri: params.text_document_position.text_document.uri,
                    range: lsp::Range::new(lsp::Position::new(2, 14), lsp::Position::new(2, 17)),
                },
            ]))
        });

    cx.set_state(before);

    let action = FindAllReferences {
        always_open_multibuffer: false,
    };

    let navigated = cx
        .update_editor(|editor, window, cx| editor.find_all_references(&action, window, cx))
        .expect("should have spawned a task")
        .await
        .unwrap();

    assert_eq!(navigated, Navigated::No);

    cx.run_until_parked();

    cx.assert_editor_state(after);
}
