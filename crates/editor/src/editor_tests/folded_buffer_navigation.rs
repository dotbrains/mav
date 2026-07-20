use super::*;

#[gpui::test]
async fn test_folding_buffer_when_multibuffer_has_only_one_excerpt(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let sample_text = "aaaa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj".to_string();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": sample_text,
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let worktree = project.update(cx, |project, cx| {
        let mut worktrees = project.worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1);
        worktrees.pop().unwrap()
    });
    let worktree_id = worktree.update(cx, |worktree, _| worktree.id());

    let buffer_1 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(ReadWrite);
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)
                ..Point::new(
                    sample_text.chars().filter(|&c| c == '\n').count() as u32 + 1,
                    0,
                )],
            0,
            cx,
        );
        multi_buffer
    });
    let multi_buffer_editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });

    let selection_range = Point::new(1, 0)..Point::new(2, 0);
    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        let multi_buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
        let highlight_range = selection_range.clone().to_anchors(&multi_buffer_snapshot);
        editor.highlight_text(
            HighlightKey::Editor,
            vec![highlight_range.clone()],
            HighlightStyle::color(Hsla::green()),
            cx,
        );
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(Some(highlight_range))
        });
    });

    let full_text = format!("\n\n{sample_text}");
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        full_text,
    );
}

#[gpui::test]
async fn test_multi_buffer_navigation_with_folded_buffers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    cx.update(|cx| {
        let default_key_bindings = settings::KeymapFile::load_asset_allow_partial_failure(
            "keymaps/default-linux.json",
            cx,
        )
        .unwrap();
        cx.bind_keys(default_key_bindings);
    });

    let (editor, cx) = cx.add_window_view(|window, cx| {
        let multi_buffer = MultiBuffer::build_multi(
            [
                ("a0\nb0\nc0\nd0\ne0\n", vec![Point::row_range(0..2)]),
                ("a1\nb1\nc1\nd1\ne1\n", vec![Point::row_range(0..2)]),
                ("a2\nb2\nc2\nd2\ne2\n", vec![Point::row_range(0..2)]),
                ("a3\nb3\nc3\nd3\ne3\n", vec![Point::row_range(0..2)]),
            ],
            cx,
        );
        let mut editor = Editor::new(EditorMode::full(), multi_buffer.clone(), None, window, cx);

        let buffer_ids = multi_buffer
            .read(cx)
            .snapshot(cx)
            .excerpts()
            .map(|excerpt| excerpt.context.start.buffer_id)
            .collect::<Vec<_>>();
        // fold all but the second buffer, so that we test navigating between two
        // adjacent folded buffers, as well as folded buffers at the start and
        // end the multibuffer
        editor.fold_buffer(buffer_ids[0], cx);
        editor.fold_buffer(buffer_ids[2], cx);
        editor.fold_buffer(buffer_ids[3], cx);

        editor
    });
    cx.simulate_resize(size(px(1000.), px(1000.)));

    let mut cx = EditorTestContext::for_editor_in(editor.clone(), cx).await;
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        ˇ[FOLDED]
        [EXCERPT]
        a1
        b1
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("down");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        ˇa1
        b1
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("down");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        a1
        ˇb1
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("down");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        a1
        b1
        ˇ[EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("down");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        a1
        b1
        [EXCERPT]
        ˇ[FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    for _ in 0..5 {
        cx.simulate_keystroke("down");
        cx.assert_excerpts_with_selections(indoc! {"
            [EXCERPT]
            [FOLDED]
            [EXCERPT]
            a1
            b1
            [EXCERPT]
            [FOLDED]
            [EXCERPT]
            ˇ[FOLDED]
            "
        });
    }

    cx.simulate_keystroke("up");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        a1
        b1
        [EXCERPT]
        ˇ[FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("up");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        a1
        b1
        ˇ[EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("up");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        a1
        ˇb1
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("up");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        ˇa1
        b1
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    for _ in 0..5 {
        cx.simulate_keystroke("up");
        cx.assert_excerpts_with_selections(indoc! {"
            [EXCERPT]
            ˇ[FOLDED]
            [EXCERPT]
            a1
            b1
            [EXCERPT]
            [FOLDED]
            [EXCERPT]
            [FOLDED]
            "
        });
    }
}
