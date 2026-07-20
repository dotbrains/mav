use super::*;

#[gpui::test]
async fn test_toggle_block_comment(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let html_language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "HTML".into(),
                block_comment: Some(BlockCommentConfig {
                    start: "<!-- ".into(),
                    prefix: "".into(),
                    end: " -->".into(),
                    tab_size: 0,
                }),
                ..Default::default()
            },
            Some(tree_sitter_html::LANGUAGE.into()),
        )
        .with_injection_query(
            r#"
            (script_element
                (raw_text) @injection.content
                (#set! injection.language "javascript"))
            "#,
        )
        .unwrap(),
    );

    let javascript_language = Arc::new(Language::new(
        LanguageConfig {
            name: "JavaScript".into(),
            line_comments: vec!["// ".into()],
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
    ));

    cx.language_registry().add(html_language.clone());
    cx.language_registry().add(javascript_language);
    cx.update_buffer(|buffer, cx| {
        buffer.set_language(Some(html_language), cx);
    });

    // Toggle comments for empty selections
    cx.set_state(
        &r#"
            <p>A</p>ˇ
            <p>B</p>ˇ
            <p>C</p>ˇ
        "#
        .unindent(),
    );
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(&ToggleComments::default(), window, cx)
    });
    cx.assert_editor_state(
        &r#"
            <!-- <p>A</p>ˇ -->
            <!-- <p>B</p>ˇ -->
            <!-- <p>C</p>ˇ -->
        "#
        .unindent(),
    );
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(&ToggleComments::default(), window, cx)
    });
    cx.assert_editor_state(
        &r#"
            <p>A</p>ˇ
            <p>B</p>ˇ
            <p>C</p>ˇ
        "#
        .unindent(),
    );

    // Toggle comments for mixture of empty and non-empty selections, where
    // multiple selections occupy a given line.
    cx.set_state(
        &r#"
            <p>A«</p>
            <p>ˇ»B</p>ˇ
            <p>C«</p>
            <p>ˇ»D</p>ˇ
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(&ToggleComments::default(), window, cx)
    });
    cx.assert_editor_state(
        &r#"
            <!-- <p>A«</p>
            <p>ˇ»B</p>ˇ -->
            <!-- <p>C«</p>
            <p>ˇ»D</p>ˇ -->
        "#
        .unindent(),
    );
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(&ToggleComments::default(), window, cx)
    });
    cx.assert_editor_state(
        &r#"
            <p>A«</p>
            <p>ˇ»B</p>ˇ
            <p>C«</p>
            <p>ˇ»D</p>ˇ
        "#
        .unindent(),
    );

    // Toggle comments when different languages are active for different
    // selections.
    cx.set_state(
        &r#"
            ˇ<script>
                ˇvar x = new Y();
            ˇ</script>
        "#
        .unindent(),
    );
    cx.executor().run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(&ToggleComments::default(), window, cx)
    });
    // TODO this is how it actually worked in Mav Stable, which is not very ergonomic.
    // Uncommenting and commenting from this position brings in even more wrong artifacts.
    cx.assert_editor_state(
        &r#"
            <!-- ˇ<script> -->
                // ˇvar x = new Y();
            <!-- ˇ</script> -->
        "#
        .unindent(),
    );
}

#[gpui::test]
fn test_editing_disjoint_excerpts(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let buffer = cx.new(|cx| Buffer::local(sample_text(6, 4, 'a'), cx));
    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer.clone(),
            [
                Point::new(0, 0)..Point::new(0, 4),
                Point::new(5, 0)..Point::new(5, 4),
            ],
            0,
            cx,
        );
        assert_eq!(multibuffer.read(cx).text(), "aaaa\nffff");
        multibuffer
    });

    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(multibuffer, window, cx));
    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(editor.text(cx), "aaaa\nffff");
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                Point::new(0, 0)..Point::new(0, 0),
                Point::new(1, 0)..Point::new(1, 0),
            ])
        });

        editor.handle_input("X", window, cx);
        assert_eq!(editor.text(cx), "Xaaaa\nXffff");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [
                Point::new(0, 1)..Point::new(0, 1),
                Point::new(1, 1)..Point::new(1, 1),
            ]
        );

        // Ensure the cursor's head is respected when deleting across an excerpt boundary.
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 2)..Point::new(1, 2)])
        });
        editor.backspace(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "Xa\nfff");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [Point::new(1, 0)..Point::new(1, 0)]
        );

        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(1, 1)..Point::new(0, 1)])
        });
        editor.backspace(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "X\nff");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [Point::new(0, 1)..Point::new(0, 1)]
        );
    });
}

#[gpui::test]
fn test_header_jump_data_uses_selection_excerpt(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // 25-line buffer so excerpts at rows 1, 10, and 20 (each a 1-line range,
    // expanded by 2 context lines) can't merge into a single excerpt.
    let buffer_text = (0..25)
        .map(|row| format!("line {row}"))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = cx.new(|cx| Buffer::local(buffer_text, cx));
    let buffer_id = buffer.read_with(cx, |buffer, _| buffer.remote_id());

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer.clone(),
            [
                Point::new(1, 0)..Point::new(1, 0),
                Point::new(10, 0)..Point::new(10, 0),
                Point::new(20, 0)..Point::new(20, 0),
            ],
            2,
            cx,
        );
        multibuffer
    });

    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(multibuffer, window, cx));

    editor.update_in(cx, |editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let display_snapshot = editor.display_snapshot(cx);

        // Ensure the three ranges landed in three separate excerpts.
        let excerpts: Vec<_> = snapshot
            .buffer_snapshot()
            .excerpts_for_buffer(buffer_id)
            .collect();
        assert_eq!(excerpts.len(), 3);

        // Place the cursor at the start of the third excerpt, expressed in
        // terms of the underlying buffer.
        let selection_buffer_row = 20;
        let buffer_entity = editor.buffer().read(cx).buffer(buffer_id).unwrap();
        let selection_anchor = editor.buffer().update(cx, |multibuffer, cx| {
            multibuffer
                .buffer_point_to_anchor(&buffer_entity, Point::new(selection_buffer_row, 0), cx)
                .expect("buffer row 20 maps to a multibuffer anchor")
        });
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_anchor_ranges([selection_anchor..selection_anchor])
        });

        let mut latest_selection_anchors: HashMap<BufferId, Anchor> = HashMap::default();
        for selection in editor.selections.all_anchors(&display_snapshot).iter() {
            let head = selection.head();
            if let Some((text_anchor, _)) = snapshot.buffer_snapshot().anchor_to_buffer_anchor(head)
            {
                latest_selection_anchors.insert(text_anchor.buffer_id, head);
            }
        }

        // The sticky buffer header represents the FIRST excerpt of its buffer,
        // even when the cursor is in a later excerpt. That mismatch is the
        // precondition for the regression.
        let first_excerpt = snapshot
            .buffer_snapshot()
            .excerpt_boundaries_in_range(MultiBufferOffset(0)..snapshot.buffer_snapshot().len())
            .next()
            .expect("multibuffer has at least one excerpt")
            .next;

        let jump_data = header_jump_data(
            &snapshot,
            DisplayRow(0),
            FILE_HEADER_HEIGHT + MULTI_BUFFER_EXCERPT_HEADER_HEIGHT,
            &first_excerpt,
            &latest_selection_anchors,
        );

        match jump_data {
            JumpData::MultiBufferPoint {
                position,
                line_offset_from_top,
                ..
            } => {
                assert_eq!(
                    position.row, selection_buffer_row,
                    "jump should target the cursor's buffer row, not the first excerpt's row"
                );
                assert!(
                    line_offset_from_top < selection_buffer_row,
                    "line_offset_from_top ({line_offset_from_top}) should be measured from the \
                     selection's excerpt, not the first excerpt; expected less than \
                     selection_buffer_row ({selection_buffer_row})"
                );
            }
            JumpData::MultiBufferRow { .. } => {
                panic!("expected MultiBufferPoint jump data when a selection is present")
            }
        }
    });
}
