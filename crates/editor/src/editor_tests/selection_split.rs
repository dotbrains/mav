use super::*;

#[gpui::test]
async fn test_split_selection_into_lines(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    #[track_caller]
    fn test(cx: &mut EditorTestContext, initial_state: &'static str, expected_state: &'static str) {
        cx.set_state(initial_state);
        cx.update_editor(|e, window, cx| {
            e.split_selection_into_lines(&Default::default(), window, cx)
        });
        cx.assert_editor_state(expected_state);
    }

    // Selection starts and ends at the middle of lines, left-to-right
    test(
        &mut cx,
        "aa\nb«ˇb\ncc\ndd\ne»e\nff",
        "aa\nbbˇ\nccˇ\nddˇ\neˇe\nff",
    );
    // Same thing, right-to-left
    test(
        &mut cx,
        "aa\nb«b\ncc\ndd\neˇ»e\nff",
        "aa\nbbˇ\nccˇ\nddˇ\neˇe\nff",
    );

    // Whole buffer, left-to-right, last line *doesn't* end with newline
    test(
        &mut cx,
        "«ˇaa\nbb\ncc\ndd\nee\nff»",
        "aaˇ\nbbˇ\nccˇ\nddˇ\neeˇ\nffˇ",
    );
    // Same thing, right-to-left
    test(
        &mut cx,
        "«aa\nbb\ncc\ndd\nee\nffˇ»",
        "aaˇ\nbbˇ\nccˇ\nddˇ\neeˇ\nffˇ",
    );

    // Whole buffer, left-to-right, last line ends with newline
    test(
        &mut cx,
        "«ˇaa\nbb\ncc\ndd\nee\nff\n»",
        "aaˇ\nbbˇ\nccˇ\nddˇ\neeˇ\nffˇ\n",
    );
    // Same thing, right-to-left
    test(
        &mut cx,
        "«aa\nbb\ncc\ndd\nee\nff\nˇ»",
        "aaˇ\nbbˇ\nccˇ\nddˇ\neeˇ\nffˇ\n",
    );

    // Starts at the end of a line, ends at the start of another
    test(
        &mut cx,
        "aa\nbb«ˇ\ncc\ndd\nee\n»ff\n",
        "aa\nbbˇ\nccˇ\nddˇ\neeˇ\nff\n",
    );
}

#[gpui::test]
async fn test_split_selection_into_lines_does_not_scroll(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let large_body = "\nline".repeat(300);
    cx.set_state(&format!("«ˇstart{large_body}\nend»"));
    let initial_scroll_position = cx.update_editor(|editor, _, cx| editor.scroll_position(cx));

    cx.update_editor(|editor, window, cx| {
        editor.split_selection_into_lines(&Default::default(), window, cx);
    });

    let scroll_position_after_split = cx.update_editor(|editor, _, cx| editor.scroll_position(cx));
    assert_eq!(
        initial_scroll_position, scroll_position_after_split,
        "Scroll position should not change after splitting selection into lines"
    );
}

#[gpui::test]
async fn test_split_selection_into_lines_interacting_with_creases(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(&sample_text(9, 5, 'a'), cx);
        build_editor(buffer, window, cx)
    });

    // setup
    _ = editor.update(cx, |editor, window, cx| {
        editor.fold_creases(
            vec![
                Crease::simple(Point::new(0, 2)..Point::new(1, 2), FoldPlaceholder::test()),
                Crease::simple(Point::new(2, 3)..Point::new(4, 1), FoldPlaceholder::test()),
                Crease::simple(Point::new(7, 0)..Point::new(8, 4), FoldPlaceholder::test()),
            ],
            true,
            window,
            cx,
        );
        assert_eq!(
            editor.display_text(cx),
            "aa⋯bbb\nccc⋯eeee\nfffff\nggggg\n⋯i"
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 2),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0),
                DisplayPoint::new(DisplayRow(4), 4)..DisplayPoint::new(DisplayRow(4), 4),
            ])
        });
        editor.split_selection_into_lines(&Default::default(), window, cx);
        assert_eq!(
            editor.display_text(cx),
            "aaaaa\nbbbbb\nccc⋯eeee\nfffff\nggggg\n⋯i"
        );
    });
    EditorTestContext::for_editor(editor, cx)
        .await
        .assert_editor_state("aˇaˇaaa\nbbbbb\nˇccccc\nddddd\neeeee\nfffff\nggggg\nhhhhh\niiiiiˇ");

    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(5), 0)..DisplayPoint::new(DisplayRow(0), 1)
            ])
        });
        editor.split_selection_into_lines(&Default::default(), window, cx);
        assert_eq!(
            editor.display_text(cx),
            "aaaaa\nbbbbb\nccccc\nddddd\neeeee\nfffff\nggggg\nhhhhh\niiiii"
        );
        assert_eq!(
            display_ranges(editor, cx),
            [
                DisplayPoint::new(DisplayRow(0), 5)..DisplayPoint::new(DisplayRow(0), 5),
                DisplayPoint::new(DisplayRow(1), 5)..DisplayPoint::new(DisplayRow(1), 5),
                DisplayPoint::new(DisplayRow(2), 5)..DisplayPoint::new(DisplayRow(2), 5),
                DisplayPoint::new(DisplayRow(3), 5)..DisplayPoint::new(DisplayRow(3), 5),
                DisplayPoint::new(DisplayRow(4), 5)..DisplayPoint::new(DisplayRow(4), 5),
                DisplayPoint::new(DisplayRow(5), 5)..DisplayPoint::new(DisplayRow(5), 5),
                DisplayPoint::new(DisplayRow(6), 5)..DisplayPoint::new(DisplayRow(6), 5)
            ]
        );
    });
    EditorTestContext::for_editor(editor, cx)
        .await
        .assert_editor_state(
            "aaaaaˇ\nbbbbbˇ\ncccccˇ\ndddddˇ\neeeeeˇ\nfffffˇ\ngggggˇ\nhhhhh\niiiii",
        );
}
