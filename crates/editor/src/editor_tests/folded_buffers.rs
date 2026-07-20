use super::*;

#[gpui::test]
async fn test_folding_buffers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let sample_text_1 = "aaaa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj".to_string();
    let sample_text_2 = "llll\nmmmm\nnnnn\noooo\npppp\nqqqq\nrrrr\nssss\ntttt\nuuuu".to_string();
    let sample_text_3 = "vvvv\nwwww\nxxxx\nyyyy\nzzzz\n1111\n2222\n3333\n4444\n5555".to_string();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "first.rs": sample_text_1,
            "second.rs": sample_text_2,
            "third.rs": sample_text_3,
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
            project.open_buffer((worktree_id, rel_path("first.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_2 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("second.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_3 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("third.rs")), cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(ReadWrite);
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(10, 4),
            ],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(10, 4),
            ],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(10, 4),
            ],
            0,
            cx,
        );
        multi_buffer
    });
    let multi_buffer_editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer.clone(),
            Some(project.clone()),
            window,
            cx,
        )
    });

    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\naaaa\nbbbb\ncccc\n\nffff\ngggg\n\njjjj\n\n\nllll\nmmmm\nnnnn\n\nqqqq\nrrrr\n\nuuuu\n\n\nvvvv\nwwww\nxxxx\n\n1111\n2222\n\n5555",
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_1.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\nllll\nmmmm\nnnnn\n\nqqqq\nrrrr\n\nuuuu\n\n\nvvvv\nwwww\nxxxx\n\n1111\n2222\n\n5555",
        "After folding the first buffer, its text should not be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_2.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\n\n\nvvvv\nwwww\nxxxx\n\n1111\n2222\n\n5555",
        "After folding the second buffer, its text should not be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_3.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\n\n",
        "After folding the third buffer, its text should not be displayed"
    );

    // Emulate selection inside the fold logic, that should work
    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        editor
            .snapshot(window, cx)
            .next_line_boundary(Point::new(0, 4));
    });

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.unfold_buffer(buffer_2.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\nllll\nmmmm\nnnnn\n\nqqqq\nrrrr\n\nuuuu\n\n",
        "After unfolding the second buffer, its text should be displayed"
    );

    // Typing inside of buffer 1 causes that buffer to be unfolded.
    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        assert_eq!(
            multi_buffer
                .read(cx)
                .snapshot(cx)
                .text_for_range(Point::new(1, 0)..Point::new(1, 4))
                .collect::<String>(),
            "bbbb"
        );
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_ranges(vec![Point::new(1, 0)..Point::new(1, 0)]);
        });
        editor.handle_input("B", window, cx);
    });

    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\naaaa\nBbbbb\ncccc\n\nffff\ngggg\n\njjjj\n\n\nllll\nmmmm\nnnnn\n\nqqqq\nrrrr\n\nuuuu\n\n",
        "After unfolding the first buffer, its and 2nd buffer's text should be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.unfold_buffer(buffer_3.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\naaaa\nBbbbb\ncccc\n\nffff\ngggg\n\njjjj\n\n\nllll\nmmmm\nnnnn\n\nqqqq\nrrrr\n\nuuuu\n\n\nvvvv\nwwww\nxxxx\n\n1111\n2222\n\n5555",
        "After unfolding the all buffers, all original text should be displayed"
    );
}

#[gpui::test]
async fn test_folded_buffers_cleared_on_excerpts_removed(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "file_a.txt": "File A\nFile A\nFile A",
            "file_b.txt": "File B\nFile B\nFile B",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let worktree = project.update(cx, |project, cx| {
        let mut worktrees = project.worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1);
        worktrees.pop().unwrap()
    });
    let worktree_id = worktree.update(cx, |worktree, _| worktree.id());

    let buffer_a = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("file_a.txt")), cx)
        })
        .await
        .unwrap();
    let buffer_b = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("file_b.txt")), cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(ReadWrite);
        let range_a = Point::new(0, 0)..Point::new(2, 4);
        let range_b = Point::new(0, 0)..Point::new(2, 4);

        multi_buffer.set_excerpts_for_path(PathKey::sorted(0), buffer_a.clone(), [range_a], 0, cx);
        multi_buffer.set_excerpts_for_path(PathKey::sorted(1), buffer_b.clone(), [range_b], 0, cx);
        multi_buffer
    });

    let editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer.clone(),
            Some(project.clone()),
            window,
            cx,
        )
    });

    editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_a.read(cx).remote_id(), cx);
    });
    assert!(editor.update(cx, |editor, cx| editor.has_any_buffer_folded(cx)));

    // When the excerpts for `buffer_a` are removed, a
    // `multi_buffer::Event::ExcerptsRemoved` event is emitted, which should be
    // picked up by the editor and update the display map accordingly.
    multi_buffer.update(cx, |multi_buffer, cx| {
        multi_buffer.remove_excerpts(PathKey::sorted(0), cx)
    });
    assert!(!editor.update(cx, |editor, cx| editor.has_any_buffer_folded(cx)));
}

#[gpui::test]
async fn test_folding_buffers_with_one_excerpt(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let sample_text_1 = "1111\n2222\n3333".to_string();
    let sample_text_2 = "4444\n5555\n6666".to_string();
    let sample_text_3 = "7777\n8888\n9999".to_string();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "first.rs": sample_text_1,
            "second.rs": sample_text_2,
            "third.rs": sample_text_3,
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
            project.open_buffer((worktree_id, rel_path("first.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_2 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("second.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_3 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("third.rs")), cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(ReadWrite);
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(3, 0)],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(3, 0)],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [Point::new(0, 0)..Point::new(3, 0)],
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

    let full_text = "\n\n1111\n2222\n3333\n\n\n4444\n5555\n6666\n\n\n7777\n8888\n9999";
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        full_text,
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_1.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\n4444\n5555\n6666\n\n\n7777\n8888\n9999",
        "After folding the first buffer, its text should not be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_2.read(cx).remote_id(), cx)
    });

    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\n\n\n7777\n8888\n9999",
        "After folding the second buffer, its text should not be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_3.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\n\n",
        "After folding the third buffer, its text should not be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.unfold_buffer(buffer_2.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\n4444\n5555\n6666\n\n",
        "After unfolding the second buffer, its text should be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.unfold_buffer(buffer_1.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n1111\n2222\n3333\n\n\n4444\n5555\n6666\n\n",
        "After unfolding the first buffer, its text should be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.unfold_buffer(buffer_3.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        full_text,
        "After unfolding all buffers, all original text should be displayed"
    );
}
