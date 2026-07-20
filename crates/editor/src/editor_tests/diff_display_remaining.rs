use super::*;

#[gpui::test]
async fn test_adjacent_diff_hunks(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        a
        b
        c
        "#
    .unindent();

    cx.set_state(
        &r#"
        ˇA
        b
        C
        "#
        .unindent(),
    );
    cx.set_head_text(&diff_base);
    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    let both_hunks_expanded = r#"
        - a
        + ˇA
          b
        - c
        + C
        "#
    .unindent();

    cx.assert_state_with_diff(both_hunks_expanded.clone());

    let hunk_ranges = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let hunks = editor
            .diff_hunks_in_ranges(&[Anchor::Min..Anchor::Max], &snapshot.buffer_snapshot())
            .collect::<Vec<_>>();
        let multibuffer_snapshot = editor.buffer.read(cx).snapshot(cx);
        hunks
            .into_iter()
            .map(|hunk| {
                multibuffer_snapshot
                    .anchor_in_excerpt(hunk.buffer_range.start)
                    .unwrap()
                    ..multibuffer_snapshot
                        .anchor_in_excerpt(hunk.buffer_range.end)
                        .unwrap()
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(hunk_ranges.len(), 2);

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[0].clone(), cx);
    });
    executor.run_until_parked();

    let second_hunk_expanded = r#"
          ˇA
          b
        - c
        + C
        "#
    .unindent();

    cx.assert_state_with_diff(second_hunk_expanded);

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[0].clone(), cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(both_hunks_expanded.clone());

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[1].clone(), cx);
    });
    executor.run_until_parked();

    let first_hunk_expanded = r#"
        - a
        + ˇA
          b
          C
        "#
    .unindent();

    cx.assert_state_with_diff(first_hunk_expanded);

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[1].clone(), cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(both_hunks_expanded);

    cx.set_state(
        &r#"
        ˇA
        b
        "#
        .unindent(),
    );
    cx.run_until_parked();

    // TODO this cursor position seems bad
    cx.assert_state_with_diff(
        r#"
        - ˇa
        + A
          b
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });

    cx.assert_state_with_diff(
        r#"
            - ˇa
            + A
              b
            - c
            "#
        .unindent(),
    );

    let hunk_ranges = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let hunks = editor
            .diff_hunks_in_ranges(&[Anchor::Min..Anchor::Max], &snapshot.buffer_snapshot())
            .collect::<Vec<_>>();
        let multibuffer_snapshot = snapshot.buffer_snapshot();
        hunks
            .into_iter()
            .map(|hunk| {
                multibuffer_snapshot
                    .anchor_in_excerpt(hunk.buffer_range.start)
                    .unwrap()
                    ..multibuffer_snapshot
                        .anchor_in_excerpt(hunk.buffer_range.end)
                        .unwrap()
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(hunk_ranges.len(), 2);

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[1].clone(), cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        - ˇa
        + A
          b
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_toggle_deletion_hunk_at_start_of_file(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        a
        b
        c
        "#
    .unindent();

    cx.set_state(
        &r#"
        ˇb
        c
        "#
        .unindent(),
    );
    cx.set_head_text(&diff_base);
    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    let hunk_expanded = r#"
        - a
          ˇb
          c
        "#
    .unindent();

    cx.assert_state_with_diff(hunk_expanded.clone());

    let hunk_ranges = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let hunks = editor
            .diff_hunks_in_ranges(&[Anchor::Min..Anchor::Max], &snapshot.buffer_snapshot())
            .collect::<Vec<_>>();
        let multibuffer_snapshot = editor.buffer.read(cx).snapshot(cx);
        hunks
            .into_iter()
            .map(|hunk| {
                multibuffer_snapshot
                    .anchor_in_excerpt(hunk.buffer_range.start)
                    .unwrap()
                    ..multibuffer_snapshot
                        .anchor_in_excerpt(hunk.buffer_range.end)
                        .unwrap()
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(hunk_ranges.len(), 1);

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[0].clone(), cx);
    });
    executor.run_until_parked();

    let hunk_collapsed = r#"
          ˇb
          c
        "#
    .unindent();

    cx.assert_state_with_diff(hunk_collapsed);

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[0].clone(), cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(hunk_expanded);
}

#[gpui::test]
async fn test_select_smaller_syntax_node_after_diff_hunk_collapse(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(rust_lang()), cx));

    cx.set_state(
        &r#"
        fn main() {
            let x = ˇ1;
        }
        "#
        .unindent(),
    );

    let diff_base = r#"
        fn removed_one() {
            println!("this function was deleted");
        }

        fn removed_two() {
            println!("this function was also deleted");
        }

        fn main() {
            let x = 1;
        }
        "#
    .unindent();
    cx.set_head_text(&diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });

    cx.update_editor(|editor, window, cx| {
        editor.collapse_all_diff_hunks(&CollapseAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.select_smaller_syntax_node(&SelectSmallerSyntaxNode, window, cx);
    });
}

#[gpui::test]
async fn test_expand_first_line_diff_hunk_keeps_deleted_lines_visible(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state("ˇnew\nsecond\nthird\n");
    cx.set_head_text("old\nsecond\nthird\n");
    cx.update_editor(|editor, window, cx| {
        editor.scroll(gpui::Point { x: 0., y: 0. }, None, window, cx);
    });
    executor.run_until_parked();
    assert_eq!(cx.update_editor(|e, _, cx| e.scroll_position(cx)).y, 0.0);

    // Expanding a diff hunk at the first line inserts deleted lines above the first buffer line.
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let multibuffer_snapshot = editor.buffer.read(cx).snapshot(cx);
        let hunks = editor
            .diff_hunks_in_ranges(&[Anchor::Min..Anchor::Max], &snapshot.buffer_snapshot())
            .collect::<Vec<_>>();
        assert_eq!(hunks.len(), 1);
        let hunk_range = multibuffer_snapshot
            .anchor_in_excerpt(hunks[0].buffer_range.start)
            .unwrap()
            ..multibuffer_snapshot
                .anchor_in_excerpt(hunks[0].buffer_range.end)
                .unwrap();
        editor.toggle_single_diff_hunk(hunk_range, cx)
    });
    executor.run_until_parked();
    cx.assert_state_with_diff("- old\n+ ˇnew\n  second\n  third\n".to_string());

    // Keep the editor scrolled to the top so the full hunk remains visible.
    assert_eq!(cx.update_editor(|e, _, cx| e.scroll_position(cx)).y, 0.0);
}

#[gpui::test]
async fn test_display_diff_hunks(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/test"),
        json!({
            ".git": {},
            "file-1": "ONE\n",
            "file-2": "TWO\n",
            "file-3": "THREE\n",
        }),
    )
    .await;

    fs.set_head_for_repo(
        path!("/test/.git").as_ref(),
        &[
            ("file-1", "one\n".into()),
            ("file-2", "two\n".into()),
            ("file-3", "three\n".into()),
        ],
        "deadbeef",
    );

    let project = Project::test(fs, [path!("/test").as_ref()], cx).await;
    let mut buffers = vec![];
    for i in 1..=3 {
        let buffer = project
            .update(cx, |project, cx| {
                let path = format!(path!("/test/file-{}"), i);
                project.open_local_buffer(path, cx)
            })
            .await
            .unwrap();
        buffers.push(buffer);
    }

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
        multibuffer.set_all_diff_hunks_expanded(cx);
        for buffer in &buffers {
            let snapshot = buffer.read(cx).snapshot();
            multibuffer.set_excerpts_for_path(
                PathKey::with_sort_prefix(0, buffer.read(cx).file().unwrap().path().clone()),
                buffer.clone(),
                vec![Point::zero()..snapshot.max_point()],
                2,
                cx,
            );
        }
        multibuffer
    });

    let editor = cx.add_window(|window, cx| {
        Editor::new(EditorMode::full(), multibuffer, Some(project), window, cx)
    });
    cx.run_until_parked();

    let snapshot = editor
        .update(cx, |editor, window, cx| editor.snapshot(window, cx))
        .unwrap();
    let hunks = snapshot
        .display_diff_hunks_for_rows(DisplayRow(0)..DisplayRow(u32::MAX), &Default::default())
        .map(|hunk| match hunk {
            DisplayDiffHunk::Unfolded {
                display_row_range, ..
            } => display_row_range,
            DisplayDiffHunk::Folded { .. } => unreachable!(),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        hunks,
        [
            DisplayRow(2)..DisplayRow(4),
            DisplayRow(7)..DisplayRow(9),
            DisplayRow(12)..DisplayRow(14),
        ]
    );
}

#[gpui::test]
async fn test_partially_staged_hunk(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_head_text(indoc! { "
        one
        two
        three
        four
        five
        "
    });
    cx.set_index_text(indoc! { "
        one
        two
        three
        four
        five
        "
    });
    cx.set_state(indoc! {"
        one
        TWO
        ˇTHREE
        FOUR
        five
    "});
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.toggle_staged_selected_diff_hunks(&Default::default(), window, cx);
    });
    cx.run_until_parked();
    cx.assert_index_text(Some(indoc! {"
        one
        TWO
        THREE
        FOUR
        five
    "}));
    cx.set_state(indoc! { "
        one
        TWO
        ˇTHREE-HUNDRED
        FOUR
        five
    "});
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let hunks = editor
            .diff_hunks_in_ranges(&[Anchor::Min..Anchor::Max], &snapshot.buffer_snapshot())
            .collect::<Vec<_>>();
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            hunks[0].status(),
            DiffHunkStatus {
                kind: DiffHunkStatusKind::Modified,
                secondary: DiffHunkSecondaryStatus::OverlapsWithSecondaryHunk
            }
        );

        editor.toggle_staged_selected_diff_hunks(&Default::default(), window, cx);
    });
    cx.run_until_parked();
    cx.assert_index_text(Some(indoc! {"
        one
        TWO
        THREE-HUNDRED
        FOUR
        five
    "}));
}
