use super::*;

#[gpui::test]
async fn test_multibuffer_reverts(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let base_text_1 = "aaaa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj";
    let base_text_2 = "llll\nmmmm\nnnnn\noooo\npppp\nqqqq\nrrrr\nssss\ntttt\nuuuu";
    let base_text_3 =
        "vvvv\nwwww\nxxxx\nyyyy\nzzzz\n{{{{\n||||\n}}}}\n~~~~\n\u{7f}\u{7f}\u{7f}\u{7f}";

    let text_1 = edit_first_char_of_every_line(base_text_1);
    let text_2 = edit_first_char_of_every_line(base_text_2);
    let text_3 = edit_first_char_of_every_line(base_text_3);

    let buffer_1 = cx.new(|cx| Buffer::local(text_1.clone(), cx));
    let buffer_2 = cx.new(|cx| Buffer::local(text_2.clone(), cx));
    let buffer_3 = cx.new(|cx| Buffer::local(text_3.clone(), cx));

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multibuffer
    });

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [path!("/").as_ref()], cx).await;
    let (editor, cx) = cx
        .add_window_view(|window, cx| build_editor_with_project(project, multibuffer, window, cx));
    editor.update_in(cx, |editor, _window, cx| {
        for (buffer, diff_base) in [
            (buffer_1.clone(), base_text_1),
            (buffer_2.clone(), base_text_2),
            (buffer_3.clone(), base_text_3),
        ] {
            let diff = cx.new(|cx| {
                BufferDiff::new_with_base_text(diff_base, &buffer.read(cx).text_snapshot(), cx)
            });
            editor
                .buffer
                .update(cx, |buffer, cx| buffer.add_diff(diff, cx));
        }
    });
    cx.executor().run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(editor.display_text(cx), "\n\nXaaa\nXbbb\nXccc\n\nXfff\nXggg\n\nXjjj\n\n\nXlll\nXmmm\nXnnn\n\nXqqq\nXrrr\n\nXuuu\n\n\nXvvv\nXwww\nXxxx\n\nX{{{\nX|||\n\nX\u{7f}\u{7f}\u{7f}");
        editor.select_all(&SelectAll, window, cx);
        editor.git_restore(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();

    // When all ranges are selected, all buffer hunks are reverted.
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.display_text(cx), "\n\naaaa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj\n\n\n\n\n\n\nllll\nmmmm\nnnnn\noooo\npppp\nqqqq\nrrrr\nssss\ntttt\nuuuu\n\n\n\n\n\n\nvvvv\nwwww\nxxxx\nyyyy\nzzzz\n{{{{\n||||\n}}}}\n~~~~\n\u{7f}\u{7f}\u{7f}\u{7f}\n\n\n\n");
    });
    buffer_1.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), base_text_1);
    });
    buffer_2.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), base_text_2);
    });
    buffer_3.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), base_text_3);
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.undo(&Default::default(), window, cx);
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(Some(Point::new(0, 0)..Point::new(5, 0)));
        });
        editor.git_restore(&Default::default(), window, cx);
    });

    // Now, when all ranges selected belong to buffer_1, the revert should succeed,
    // but not affect buffer_2 and its related excerpts.
    editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.display_text(cx),
            "\n\naaaa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj\n\n\n\n\n\n\nXlll\nXmmm\nXnnn\n\nXqqq\nXrrr\n\nXuuu\n\n\nXvvv\nXwww\nXxxx\n\nX{{{\nX|||\n\nX\u{7f}\u{7f}\u{7f}"
        );
    });
    buffer_1.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), base_text_1);
    });
    buffer_2.update(cx, |buffer, _| {
        assert_eq!(
            buffer.text(),
            "Xlll\nXmmm\nXnnn\nXooo\nXppp\nXqqq\nXrrr\nXsss\nXttt\nXuuu"
        );
    });
    buffer_3.update(cx, |buffer, _| {
        assert_eq!(
            buffer.text(),
            "Xvvv\nXwww\nXxxx\nXyyy\nXzzz\nX{{{\nX|||\nX}}}\nX~~~\nX\u{7f}\u{7f}\u{7f}"
        );
    });

    fn edit_first_char_of_every_line(text: &str) -> String {
        text.split('\n')
            .map(|line| format!("X{}", &line[1..]))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[gpui::test]
async fn test_multibuffer_in_navigation_history(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let cols = 4;
    let rows = 10;
    let sample_text_1 = sample_text(rows, cols, 'a');
    assert_eq!(
        sample_text_1,
        "aaaa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj"
    );
    let sample_text_2 = sample_text(rows, cols, 'l');
    assert_eq!(
        sample_text_2,
        "llll\nmmmm\nnnnn\noooo\npppp\nqqqq\nrrrr\nssss\ntttt\nuuuu"
    );
    let sample_text_3 = sample_text(rows, cols, 'v');
    assert_eq!(
        sample_text_3,
        "vvvv\nwwww\nxxxx\nyyyy\nzzzz\n{{{{\n||||\n}}}}\n~~~~\n\u{7f}\u{7f}\u{7f}\u{7f}"
    );

    let buffer_1 = cx.new(|cx| Buffer::local(sample_text_1.clone(), cx));
    let buffer_2 = cx.new(|cx| Buffer::local(sample_text_2.clone(), cx));
    let buffer_3 = cx.new(|cx| Buffer::local(sample_text_3.clone(), cx));

    let multi_buffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multibuffer
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/a",
        json!({
            "main.rs": sample_text_1,
            "other.rs": sample_text_2,
            "lib.rs": sample_text_3,
        }),
    )
    .await;
    let project = Project::test(fs, ["/a".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let multi_buffer_editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });
    let multibuffer_item_id = workspace.update_in(cx, |workspace, window, cx| {
        assert!(
            workspace.active_item(cx).is_none(),
            "active item should be None before the first item is added"
        );
        workspace.add_item_to_active_pane(
            Box::new(multi_buffer_editor.clone()),
            None,
            true,
            window,
            cx,
        );
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after adding the multi buffer");
        assert_eq!(
            active_item.buffer_kind(cx),
            ItemBufferKind::Multibuffer,
            "A multi buffer was expected to active after adding"
        );
        active_item.item_id()
    });

    cx.executor().run_until_parked();

    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(
            SelectionEffects::scroll(Autoscroll::Next),
            window,
            cx,
            |s| s.select_ranges(Some(MultiBufferOffset(1)..MultiBufferOffset(2))),
        );
        editor.open_excerpts(&OpenExcerpts, window, cx);
    });
    cx.executor().run_until_parked();
    let first_item_id = workspace.update_in(cx, |workspace, window, cx| {
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after navigating into the 1st buffer");
        let first_item_id = active_item.item_id();
        assert_ne!(
            first_item_id, multibuffer_item_id,
            "Should navigate into the 1st buffer and activate it"
        );
        assert_eq!(
            active_item.buffer_kind(cx),
            ItemBufferKind::Singleton,
            "New active item should be a singleton buffer"
        );
        assert_eq!(
            active_item
                .act_as::<Editor>(cx)
                .expect("should have navigated into an editor for the 1st buffer")
                .read(cx)
                .text(cx),
            sample_text_1
        );

        workspace
            .go_back(workspace.active_pane().downgrade(), window, cx)
            .detach_and_log_err(cx);

        first_item_id
    });

    cx.executor().run_until_parked();
    workspace.update_in(cx, |workspace, _, cx| {
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after navigating back");
        assert_eq!(
            active_item.item_id(),
            multibuffer_item_id,
            "Should navigate back to the multi buffer"
        );
        assert_eq!(active_item.buffer_kind(cx), ItemBufferKind::Multibuffer);
    });

    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(
            SelectionEffects::scroll(Autoscroll::Next),
            window,
            cx,
            |s| s.select_ranges(Some(MultiBufferOffset(39)..MultiBufferOffset(40))),
        );
        editor.open_excerpts(&OpenExcerpts, window, cx);
    });
    cx.executor().run_until_parked();
    let second_item_id = workspace.update_in(cx, |workspace, window, cx| {
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after navigating into the 2nd buffer");
        let second_item_id = active_item.item_id();
        assert_ne!(
            second_item_id, multibuffer_item_id,
            "Should navigate away from the multibuffer"
        );
        assert_ne!(
            second_item_id, first_item_id,
            "Should navigate into the 2nd buffer and activate it"
        );
        assert_eq!(
            active_item.buffer_kind(cx),
            ItemBufferKind::Singleton,
            "New active item should be a singleton buffer"
        );
        assert_eq!(
            active_item
                .act_as::<Editor>(cx)
                .expect("should have navigated into an editor")
                .read(cx)
                .text(cx),
            sample_text_2
        );

        workspace
            .go_back(workspace.active_pane().downgrade(), window, cx)
            .detach_and_log_err(cx);

        second_item_id
    });

    cx.executor().run_until_parked();
    workspace.update_in(cx, |workspace, _, cx| {
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after navigating back from the 2nd buffer");
        assert_eq!(
            active_item.item_id(),
            multibuffer_item_id,
            "Should navigate back from the 2nd buffer to the multi buffer"
        );
        assert_eq!(active_item.buffer_kind(cx), ItemBufferKind::Multibuffer);
    });

    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(
            SelectionEffects::scroll(Autoscroll::Next),
            window,
            cx,
            |s| s.select_ranges(Some(MultiBufferOffset(70)..MultiBufferOffset(70))),
        );
        editor.open_excerpts(&OpenExcerpts, window, cx);
    });
    cx.executor().run_until_parked();
    workspace.update_in(cx, |workspace, window, cx| {
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after navigating into the 3rd buffer");
        let third_item_id = active_item.item_id();
        assert_ne!(
            third_item_id, multibuffer_item_id,
            "Should navigate into the 3rd buffer and activate it"
        );
        assert_ne!(third_item_id, first_item_id);
        assert_ne!(third_item_id, second_item_id);
        assert_eq!(
            active_item.buffer_kind(cx),
            ItemBufferKind::Singleton,
            "New active item should be a singleton buffer"
        );
        assert_eq!(
            active_item
                .act_as::<Editor>(cx)
                .expect("should have navigated into an editor")
                .read(cx)
                .text(cx),
            sample_text_3
        );

        workspace
            .go_back(workspace.active_pane().downgrade(), window, cx)
            .detach_and_log_err(cx);
    });

    cx.executor().run_until_parked();
    workspace.update_in(cx, |workspace, _, cx| {
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after navigating back from the 3rd buffer");
        assert_eq!(
            active_item.item_id(),
            multibuffer_item_id,
            "Should navigate back from the 3rd buffer to the multi buffer"
        );
        assert_eq!(active_item.buffer_kind(cx), ItemBufferKind::Multibuffer);
    });
}
