use super::*;

#[gpui::test]
async fn test_add_selection_after_moving_with_multiple_cursors(cx: &mut TestAppContext) {
    // Regression test for issue #11671
    // Previously, adding a cursor after moving multiple cursors would reset
    // the cursor count instead of adding to the existing cursors.
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    // Create a simple buffer with cursor at start
    cx.set_state(indoc! {"
        ˇaaaa
        bbbb
        cccc
        dddd
        eeee
        ffff
        gggg
        hhhh"});

    // Add 2 cursors below (so we have 3 total)
    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
        editor.add_selection_below(&Default::default(), window, cx);
    });

    // Verify we have 3 cursors
    let initial_count = cx.update_editor(|editor, _, _| editor.selections.count());
    assert_eq!(
        initial_count, 3,
        "Should have 3 cursors after adding 2 below"
    );

    // Move down one line
    cx.update_editor(|editor, window, cx| {
        editor.move_down(&MoveDown, window, cx);
    });

    // Add another cursor below
    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    // Should now have 4 cursors (3 original + 1 new)
    let final_count = cx.update_editor(|editor, _, _| editor.selections.count());
    assert_eq!(
        final_count, 4,
        "Should have 4 cursors after moving and adding another"
    );
}

#[gpui::test]
async fn test_add_selection_skip_soft_wrap_option(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc!(
        r#"ˇThis is a very long line that will be wrapped when soft wrapping is enabled
           Second line here"#
    ));

    cx.update_editor(|editor, window, cx| {
        // Enable soft wrapping with a narrow width to force soft wrapping and
        // confirm that more than 2 rows are being displayed.
        editor.set_wrap_width(Some(100.0.into()), cx);
        assert!(editor.display_text(cx).lines().count() > 2);

        editor.add_selection_below(
            &AddSelectionBelow {
                skip_soft_wrap: true,
            },
            window,
            cx,
        );

        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(8), 0)..DisplayPoint::new(DisplayRow(8), 0),
            ]
        );

        editor.add_selection_above(
            &AddSelectionAbove {
                skip_soft_wrap: true,
            },
            window,
            cx,
        );

        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)]
        );

        editor.add_selection_below(
            &AddSelectionBelow {
                skip_soft_wrap: false,
            },
            window,
            cx,
        );

        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0),
            ]
        );

        editor.add_selection_above(
            &AddSelectionAbove {
                skip_soft_wrap: false,
            },
            window,
            cx,
        );

        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)]
        );
    });

    // Set up text where selections are in the middle of a soft-wrapped line.
    // When adding selection below with `skip_soft_wrap` set to `true`, the new
    // selection should be at the same buffer column, not the same pixel
    // position.
    cx.set_state(indoc!(
        r#"1. Very long line to show «howˇ» a wrapped line would look
           2. Very long line to show how a wrapped line would look"#
    ));

    cx.update_editor(|editor, window, cx| {
        // Enable soft wrapping with a narrow width to force soft wrapping and
        // confirm that more than 2 rows are being displayed.
        editor.set_wrap_width(Some(100.0.into()), cx);
        assert!(editor.display_text(cx).lines().count() > 2);

        editor.add_selection_below(
            &AddSelectionBelow {
                skip_soft_wrap: true,
            },
            window,
            cx,
        );

        // Assert that there's now 2 selections, both selecting the same column
        // range in the buffer row.
        let display_map = editor.display_map.update(cx, |map, cx| map.snapshot(cx));
        let selections = editor.selections.all::<Point>(&display_map);
        assert_eq!(selections.len(), 2);
        assert_eq!(selections[0].start.column, selections[1].start.column);
        assert_eq!(selections[0].end.column, selections[1].end.column);
    });
}
