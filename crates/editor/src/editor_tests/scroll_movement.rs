use super::*;

#[gpui::test]
async fn test_move_start_of_paragraph_end_of_paragraph(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let line_height = cx.update_editor(|editor, window, cx| {
        editor
            .style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });
    cx.simulate_window_resize(cx.window, size(px(100.), 4. * line_height));

    // The third line only contains a single space so we can later assert that the
    // editor's paragraph movement considers a non-blank line as a paragraph
    // boundary.
    cx.set_state(&"ˇone\ntwo\n \nthree\nfourˇ\nfive\n\nsix");

    cx.update_editor(|editor, window, cx| {
        editor.move_to_end_of_paragraph(&MoveToEndOfParagraph, window, cx)
    });
    cx.assert_editor_state(&"one\ntwo\nˇ \nthree\nfour\nfive\nˇ\nsix");

    cx.update_editor(|editor, window, cx| {
        editor.move_to_end_of_paragraph(&MoveToEndOfParagraph, window, cx)
    });
    cx.assert_editor_state(&"one\ntwo\n \nthree\nfour\nfive\nˇ\nsixˇ");

    cx.update_editor(|editor, window, cx| {
        editor.move_to_end_of_paragraph(&MoveToEndOfParagraph, window, cx)
    });
    cx.assert_editor_state(&"one\ntwo\n \nthree\nfour\nfive\n\nsixˇ");

    cx.update_editor(|editor, window, cx| {
        editor.move_to_start_of_paragraph(&MoveToStartOfParagraph, window, cx)
    });
    cx.assert_editor_state(&"one\ntwo\n \nthree\nfour\nfive\nˇ\nsix");

    cx.update_editor(|editor, window, cx| {
        editor.move_to_start_of_paragraph(&MoveToStartOfParagraph, window, cx)
    });

    cx.assert_editor_state(&"one\ntwo\nˇ \nthree\nfour\nfive\n\nsix");

    cx.update_editor(|editor, window, cx| {
        editor.move_to_start_of_paragraph(&MoveToStartOfParagraph, window, cx)
    });
    cx.assert_editor_state(&"ˇone\ntwo\n \nthree\nfour\nfive\n\nsix");
}

#[gpui::test]
async fn test_scroll_page_up_page_down(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;
    let line_height = cx.update_editor(|editor, window, cx| {
        editor
            .style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });
    let window = cx.window;
    cx.simulate_window_resize(window, size(px(1000.), 4. * line_height + px(0.5)));

    cx.set_state(
        r#"ˇone
        two
        three
        four
        five
        six
        seven
        eight
        nine
        ten
        "#,
    );

    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 0.)
        );
        editor.scroll_screen(&ScrollAmount::Page(1.), window, cx);
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 3.)
        );
        editor.scroll_screen(&ScrollAmount::Page(1.), window, cx);
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 6.)
        );
        editor.scroll_screen(&ScrollAmount::Page(-1.), window, cx);
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 3.)
        );

        editor.scroll_screen(&ScrollAmount::Page(-0.5), window, cx);
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 1.)
        );
        editor.scroll_screen(&ScrollAmount::Page(0.5), window, cx);
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 3.)
        );
    });
}

#[gpui::test]
async fn test_autoscroll(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let line_height = cx.update_editor(|editor, window, cx| {
        editor.set_vertical_scroll_margin(2, cx);
        editor
            .style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });
    let window = cx.window;
    cx.simulate_window_resize(window, size(px(1000.), 6. * line_height));

    cx.set_state(
        r#"ˇone
            two
            three
            four
            five
            six
            seven
            eight
            nine
            ten
        "#,
    );
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 0.0)
        );
    });

    // Add a cursor below the visible area. Since both cursors cannot fit
    // on screen, the editor autoscrolls to reveal the newest cursor, and
    // allows the vertical scroll margin below that cursor.
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |selections| {
            selections.select_ranges([
                Point::new(0, 0)..Point::new(0, 0),
                Point::new(6, 0)..Point::new(6, 0),
            ]);
        })
    });
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 3.0)
        );
    });

    // Move down. The editor cursor scrolls down to track the newest cursor.
    cx.update_editor(|editor, window, cx| {
        editor.move_down(&Default::default(), window, cx);
    });
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 4.0)
        );
    });

    // Add a cursor above the visible area. Since both cursors fit on screen,
    // the editor scrolls to show both.
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |selections| {
            selections.select_ranges([
                Point::new(1, 0)..Point::new(1, 0),
                Point::new(6, 0)..Point::new(6, 0),
            ]);
        })
    });
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 1.0)
        );
    });
}

#[gpui::test]
async fn test_autoscroll_relative(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let line_height = cx.update_editor(|editor, window, cx| {
        editor.set_vertical_scroll_margin(0, cx);
        editor
            .style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });
    let window = cx.window;

    // Resize the window such that only 6 lines of text fit on screen.
    cx.simulate_window_resize(window, size(px(1000.), 6. * line_height));

    cx.set_state(
        r#"ˇone
            two
            three
            four
            five
            six
            seven
            eight
            nine
            ten
            eleven
            twelve
            thirteen
            fourteen
            fifteen
        "#,
    );
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 0.0)
        );
    });

    // Placing the cursor at row 7 with a top-relative autoscroll of 2 display
    // rows, should land the scroll position's y coordinate at 5.0 (7 - 2).
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(
            SelectionEffects::scroll(Autoscroll::top_relative(2.0)),
            window,
            cx,
            |selections| selections.select_ranges([Point::new(7, 0)..Point::new(7, 0)]),
        );
    });
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 5.0)
        );
    });

    // Seeing as fractional offsets are supported, with the cursor at row 10 and
    // a top-relative autoscroll of 2.5 display rows, the scroll position's y
    // coordinate lands at 7.5 (10 - 2.5).
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(
            SelectionEffects::scroll(Autoscroll::top_relative(2.5)),
            window,
            cx,
            |selections| selections.select_ranges([Point::new(10, 0)..Point::new(10, 0)]),
        );
    });
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 7.5)
        );
    });

    // When the requested offset would scroll past the top of the buffer,
    // `scroll_position.y` is clamped to 0 rather than going negative.
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(
            SelectionEffects::scroll(Autoscroll::top_relative(4.0)),
            window,
            cx,
            |selections| selections.select_ranges([Point::new(1, 0)..Point::new(1, 0)]),
        );
    });

    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 0.0)
        );
    });
}

#[gpui::test]
async fn test_exclude_overscroll_margin_clamps_scroll_position(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.scroll_beyond_last_line = Some(ScrollBeyondLastLine::OnePage);
    });

    let mut cx = EditorTestContext::new(cx).await;

    let line_height = cx.update_editor(|editor, window, cx| {
        editor.set_mode(EditorMode::Full {
            scale_ui_elements_with_buffer_font_size: false,
            show_active_line_background: false,
            sizing_behavior: SizingBehavior::ExcludeOverscrollMargin,
        });
        editor
            .style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });
    let window = cx.window;
    cx.simulate_window_resize(window, size(px(1000.), 6. * line_height));
    cx.set_state(
        &r#"
        ˇone
        two
        three
        four
        five
        six
        seven
        eight
        nine
        ten
        eleven
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let max_scroll_top =
            (snapshot.max_point().row().as_f64() - editor.visible_line_count().unwrap() + 1.)
                .max(0.);

        editor.set_scroll_position(gpui::Point::new(0., max_scroll_top + 10.), window, cx);

        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., max_scroll_top)
        );
    });
}

#[gpui::test]
async fn test_move_page_up_page_down(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let line_height = cx.update_editor(|editor, window, cx| {
        editor
            .style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });
    let window = cx.window;
    cx.simulate_window_resize(window, size(px(100.), 4. * line_height));
    cx.set_state(
        &r#"
        ˇone
        two
        threeˇ
        four
        five
        six
        seven
        eight
        nine
        ten
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.move_page_down(&MovePageDown::default(), window, cx)
    });
    cx.assert_editor_state(
        &r#"
        one
        two
        three
        ˇfour
        five
        sixˇ
        seven
        eight
        nine
        ten
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.move_page_down(&MovePageDown::default(), window, cx)
    });
    cx.assert_editor_state(
        &r#"
        one
        two
        three
        four
        five
        six
        ˇseven
        eight
        nineˇ
        ten
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| editor.move_page_up(&MovePageUp::default(), window, cx));
    cx.assert_editor_state(
        &r#"
        one
        two
        three
        ˇfour
        five
        sixˇ
        seven
        eight
        nine
        ten
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| editor.move_page_up(&MovePageUp::default(), window, cx));
    cx.assert_editor_state(
        &r#"
        ˇone
        two
        threeˇ
        four
        five
        six
        seven
        eight
        nine
        ten
        "#
        .unindent(),
    );

    // Test select collapsing
    cx.update_editor(|editor, window, cx| {
        editor.move_page_down(&MovePageDown::default(), window, cx);
        editor.move_page_down(&MovePageDown::default(), window, cx);
        editor.move_page_down(&MovePageDown::default(), window, cx);
    });
    cx.assert_editor_state(
        &r#"
        one
        two
        three
        four
        five
        six
        seven
        eight
        nine
        ˇten
        ˇ"#
        .unindent(),
    );
}
