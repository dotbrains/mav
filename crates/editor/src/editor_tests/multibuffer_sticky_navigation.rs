use super::*;

#[gpui::test]
async fn test_scroll_by_clicking_sticky_header(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.sticky_scroll = Some(settings::StickyScrollContent {
                    enabled: Some(true),
                })
            });
        });
    });
    let mut cx = EditorTestContext::new(cx).await;

    let line_height = cx.update_editor(|editor, window, cx| {
        editor
            .style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });

    let buffer = indoc! {"
            ˇfn foo() {
                let abc = 123;
            }
            struct Bar;
            impl Bar {
                fn new() -> Self {
                    Self
                }
            }
            fn baz() {
            }
        "};
    cx.set_state(&buffer);

    cx.update_editor(|e, _, cx| {
        e.buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| {
                buffer.set_language(Some(rust_lang()), cx);
            })
    });

    let fn_foo = || empty_range(0, 0);
    let impl_bar = || empty_range(4, 0);
    let fn_new = || empty_range(5, 0);

    let mut scroll_and_click = |scroll_offset: ScrollOffset, click_offset: ScrollOffset| {
        cx.update_editor(|e, window, cx| {
            e.scroll(
                gpui::Point {
                    x: 0.,
                    y: scroll_offset,
                },
                None,
                window,
                cx,
            );
        });
        cx.run_until_parked();
        cx.simulate_click(
            gpui::Point {
                x: px(0.),
                y: click_offset as f32 * line_height,
            },
            Modifiers::none(),
        );
        cx.run_until_parked();
        cx.update_editor(|e, _, cx| (e.scroll_position(cx), display_ranges(e, cx)))
    };
    assert_eq!(
        scroll_and_click(
            4.5, // impl Bar is halfway off the screen
            0.0  // click top of screen
        ),
        // scrolled to impl Bar
        (gpui::Point { x: 0., y: 4. }, vec![impl_bar()])
    );

    assert_eq!(
        scroll_and_click(
            4.5,  // impl Bar is halfway off the screen
            0.25  // click middle of impl Bar
        ),
        // scrolled to impl Bar
        (gpui::Point { x: 0., y: 4. }, vec![impl_bar()])
    );

    assert_eq!(
        scroll_and_click(
            4.5, // impl Bar is halfway off the screen
            1.5  // click below impl Bar (e.g. fn new())
        ),
        // scrolled to fn new() - this is below the impl Bar header which has persisted
        (gpui::Point { x: 0., y: 4. }, vec![fn_new()])
    );

    assert_eq!(
        scroll_and_click(
            5.5,  // fn new is halfway underneath impl Bar
            0.75  // click on the overlap of impl Bar and fn new()
        ),
        (gpui::Point { x: 0., y: 4. }, vec![impl_bar()])
    );

    assert_eq!(
        scroll_and_click(
            5.5,  // fn new is halfway underneath impl Bar
            1.25  // click on the visible part of fn new()
        ),
        (gpui::Point { x: 0., y: 4. }, vec![fn_new()])
    );

    assert_eq!(
        scroll_and_click(
            1.5, // fn foo is halfway off the screen
            0.0  // click top of screen
        ),
        (gpui::Point { x: 0., y: 0. }, vec![fn_foo()])
    );

    assert_eq!(
        scroll_and_click(
            1.5,  // fn foo is halfway off the screen
            0.75  // click visible part of let abc...
        )
        .0,
        // no change in scroll
        // we don't assert on the visible_range because if we clicked the gutter, our line is fully selected
        (gpui::Point { x: 0., y: 1.5 })
    );

    // Verify clicking at a specific x position within a sticky header places
    // the cursor at the corresponding column.
    let (text_origin_x, em_width) = cx.update_editor(|editor, _, _| {
        let position_map = editor.last_position_map.as_ref().unwrap();
        (
            position_map.text_hitbox.bounds.origin.x,
            position_map.em_layout_width,
        )
    });

    // Click on "impl Bar {" sticky header at column 5 (the 'B' in 'Bar').
    // The text "impl Bar {" starts at column 0, so column 5 = 'B'.
    let click_x = text_origin_x + em_width * 5.5;
    cx.update_editor(|e, window, cx| {
        e.scroll(gpui::Point { x: 0., y: 4.5 }, None, window, cx);
    });
    cx.run_until_parked();
    cx.simulate_click(
        gpui::Point {
            x: click_x,
            y: 0.25 * line_height,
        },
        Modifiers::none(),
    );
    cx.run_until_parked();
    let (scroll_pos, selections) =
        cx.update_editor(|e, _, cx| (e.scroll_position(cx), display_ranges(e, cx)));
    assert_eq!(scroll_pos, gpui::Point { x: 0., y: 4. });
    assert_eq!(selections, vec![empty_range(4, 5)]);
}

#[gpui::test]
async fn test_clicking_sticky_header_sets_character_select_mode(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.sticky_scroll = Some(settings::StickyScrollContent {
                    enabled: Some(true),
                })
            });
        });
    });
    let mut cx = EditorTestContext::new(cx).await;

    let line_height = cx.update_editor(|editor, window, cx| {
        editor
            .style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });

    let buffer = indoc! {"
            fn foo() {
                let abc = 123;
            }
            ˇstruct Bar;
        "};
    cx.set_state(&buffer);

    cx.update_editor(|editor, _, cx| {
        editor
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| {
                buffer.set_language(Some(rust_lang()), cx);
            })
    });

    let text_origin_x = cx.update_editor(|editor, _, _| {
        editor
            .last_position_map
            .as_ref()
            .unwrap()
            .text_hitbox
            .bounds
            .origin
            .x
    });

    cx.update_editor(|editor, window, cx| {
        // Double click on `struct` to select it
        editor.begin_selection(DisplayPoint::new(DisplayRow(3), 1), false, 2, window, cx);
        editor.end_selection(window, cx);

        // Scroll down one row to make `fn foo() {` a sticky header
        editor.scroll(gpui::Point { x: 0., y: 1. }, None, window, cx);
    });
    cx.run_until_parked();

    // Click at the start of the `fn foo() {` sticky header
    cx.simulate_click(
        gpui::Point {
            x: text_origin_x,
            y: 0.5 * line_height,
        },
        Modifiers::none(),
    );
    cx.run_until_parked();

    // Shift-click at the end of `fn foo() {` to select the whole row
    cx.update_editor(|editor, window, cx| {
        editor.extend_selection(DisplayPoint::new(DisplayRow(0), 10), 1, window, cx);
        editor.end_selection(window, cx);
    });
    cx.run_until_parked();

    let selections = cx.update_editor(|editor, _, cx| display_ranges(editor, cx));
    assert_eq!(
        selections,
        vec![DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 10)]
    );
}

#[gpui::test]
async fn test_next_prev_reference(cx: &mut TestAppContext) {
    const CYCLE_POSITIONS: &[&'static str] = &[
        indoc! {"
            fn foo() {
                let ˇabc = 123;
                let x = abc + 1;
                let y = abc + 2;
                let z = abc + 2;
            }
        "},
        indoc! {"
            fn foo() {
                let abc = 123;
                let x = ˇabc + 1;
                let y = abc + 2;
                let z = abc + 2;
            }
        "},
        indoc! {"
            fn foo() {
                let abc = 123;
                let x = abc + 1;
                let y = ˇabc + 2;
                let z = abc + 2;
            }
        "},
        indoc! {"
            fn foo() {
                let abc = 123;
                let x = abc + 1;
                let y = abc + 2;
                let z = ˇabc + 2;
            }
        "},
    ];

    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            references_provider: Some(lsp::OneOf::Left(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    // importantly, the cursor is in the middle
    cx.set_state(indoc! {"
        fn foo() {
            let aˇbc = 123;
            let x = abc + 1;
            let y = abc + 2;
            let z = abc + 2;
        }
    "});

    let reference_ranges = [
        lsp::Position::new(1, 8),
        lsp::Position::new(2, 12),
        lsp::Position::new(3, 12),
        lsp::Position::new(4, 12),
    ]
    .map(|start| lsp::Range::new(start, lsp::Position::new(start.line, start.character + 3)));

    cx.lsp
        .set_request_handler::<lsp::request::References, _, _>(move |params, _cx| async move {
            Ok(Some(
                reference_ranges
                    .map(|range| lsp::Location {
                        uri: params.text_document_position.text_document.uri.clone(),
                        range,
                    })
                    .to_vec(),
            ))
        });

    let _move = async |direction, count, cx: &mut EditorLspTestContext| {
        cx.update_editor(|editor, window, cx| {
            editor.go_to_reference_before_or_after_position(direction, count, window, cx)
        })
        .unwrap()
        .await
        .unwrap()
    };

    _move(Direction::Next, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[1]);

    _move(Direction::Next, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[2]);

    _move(Direction::Next, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[3]);

    // loops back to the start
    _move(Direction::Next, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[0]);

    // loops back to the end
    _move(Direction::Prev, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[3]);

    _move(Direction::Prev, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[2]);

    _move(Direction::Prev, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[1]);

    _move(Direction::Prev, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[0]);

    _move(Direction::Next, 3, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[3]);

    _move(Direction::Prev, 2, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[1]);
}
