use super::*;

#[gpui::test]
fn test_insert_with_old_selections(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("a( X ), b( Y ), c( Z )", cx);
        let mut editor = build_editor(buffer, window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                MultiBufferOffset(3)..MultiBufferOffset(4),
                MultiBufferOffset(11)..MultiBufferOffset(12),
                MultiBufferOffset(19)..MultiBufferOffset(20),
            ])
        });
        editor
    });

    _ = editor.update(cx, |editor, window, cx| {
        // Edit the buffer directly, deleting ranges surrounding the editor's selections
        editor.buffer.update(cx, |buffer, cx| {
            buffer.edit(
                [
                    (MultiBufferOffset(2)..MultiBufferOffset(5), ""),
                    (MultiBufferOffset(10)..MultiBufferOffset(13), ""),
                    (MultiBufferOffset(18)..MultiBufferOffset(21), ""),
                ],
                None,
                cx,
            );
            assert_eq!(buffer.read(cx).text(), "a(), b(), c()".unindent());
        });
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            &[
                MultiBufferOffset(2)..MultiBufferOffset(2),
                MultiBufferOffset(7)..MultiBufferOffset(7),
                MultiBufferOffset(12)..MultiBufferOffset(12)
            ],
        );

        editor.insert("Z", window, cx);
        assert_eq!(editor.text(cx), "a(Z), b(Z), c(Z)");

        // The selections are moved after the inserted characters
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            &[
                MultiBufferOffset(3)..MultiBufferOffset(3),
                MultiBufferOffset(9)..MultiBufferOffset(9),
                MultiBufferOffset(15)..MultiBufferOffset(15)
            ],
        );
    });
}

#[gpui::test]
async fn test_tab(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(3)
    });

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(indoc! {"
        ˇabˇc
        ˇ🏀ˇ🏀ˇefg
        dˇ
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
           ˇab ˇc
           ˇ🏀  ˇ🏀  ˇefg
        d  ˇ
    "});

    cx.set_state(indoc! {"
        a
        «🏀ˇ»🏀«🏀ˇ»🏀«🏀ˇ»
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        a
           «🏀ˇ»🏀«🏀ˇ»🏀«🏀ˇ»
    "});
}

#[gpui::test]
async fn test_tab_in_leading_whitespace_auto_indents_lines(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = Arc::new(
        Language::new(
            LanguageConfig::default(),
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_indents_query(r#"(_ "(" ")" @end) @indent"#)
        .unwrap(),
    );
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test when all cursors are not at suggested indent
    // then simply move to their suggested indent location
    cx.set_state(indoc! {"
        const a: B = (
            c(
        ˇ
        ˇ    )
        );
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: B = (
            c(
                ˇ
            ˇ)
        );
    "});

    // test cursor already at suggested indent not moving when
    // other cursors are yet to reach their suggested indents
    cx.set_state(indoc! {"
        ˇ
        const a: B = (
            c(
                d(
        ˇ
                )
        ˇ
        ˇ    )
        );
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        ˇ
        const a: B = (
            c(
                d(
                    ˇ
                )
                ˇ
            ˇ)
        );
    "});
    // test when all cursors are at suggested indent then tab is inserted
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
            ˇ
        const a: B = (
            c(
                d(
                        ˇ
                )
                    ˇ
                ˇ)
        );
    "});

    // test when current indent is less than suggested indent,
    // we adjust line to match suggested indent and move cursor to it
    //
    // when no other cursor is at word boundary, all of them should move
    cx.set_state(indoc! {"
        const a: B = (
            c(
                d(
        ˇ
        ˇ   )
        ˇ   )
        );
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: B = (
            c(
                d(
                    ˇ
                ˇ)
            ˇ)
        );
    "});

    // test when current indent is less than suggested indent,
    // we adjust line to match suggested indent and move cursor to it
    //
    // when some other cursor is at word boundary, it should not move
    cx.set_state(indoc! {"
        const a: B = (
            c(
                d(
        ˇ
        ˇ   )
           ˇ)
        );
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: B = (
            c(
                d(
                    ˇ
                ˇ)
            ˇ)
        );
    "});

    // test when current indent is more than suggested indent,
    // we just move cursor to current indent instead of suggested indent
    //
    // when no other cursor is at word boundary, all of them should move
    cx.set_state(indoc! {"
        const a: B = (
            c(
                d(
        ˇ
        ˇ                )
        ˇ   )
        );
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: B = (
            c(
                d(
                    ˇ
                        ˇ)
            ˇ)
        );
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: B = (
            c(
                d(
                        ˇ
                            ˇ)
                ˇ)
        );
    "});

    // test when current indent is more than suggested indent,
    // we just move cursor to current indent instead of suggested indent
    //
    // when some other cursor is at word boundary, it doesn't move
    cx.set_state(indoc! {"
        const a: B = (
            c(
                d(
        ˇ
        ˇ                )
            ˇ)
        );
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: B = (
            c(
                d(
                    ˇ
                        ˇ)
            ˇ)
        );
    "});

    // handle auto-indent when there are multiple cursors on the same line
    cx.set_state(indoc! {"
        const a: B = (
            c(
        ˇ    ˇ
        ˇ    )
        );
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: B = (
            c(
                ˇ
            ˇ)
        );
    "});
}

#[gpui::test]
async fn test_tab_with_mixed_whitespace_txt(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(3)
    });

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(indoc! {"
         ˇ
        \t ˇ
        \t  ˇ
        \t   ˇ
         \t  \t\t \t      \t\t   \t\t    \t \t ˇ
    "});

    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
           ˇ
        \t   ˇ
        \t   ˇ
        \t      ˇ
         \t  \t\t \t      \t\t   \t\t    \t \t   ˇ
    "});
}

#[gpui::test]
async fn test_tab_with_mixed_whitespace_rust(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(4)
    });

    let language = Arc::new(
        Language::new(
            LanguageConfig::default(),
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_indents_query(r#"(_ "{" "}" @end) @indent"#)
        .unwrap(),
    );

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
    cx.set_state(indoc! {"
        fn a() {
            if b {
        \t ˇc
            }
        }
    "});

    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        fn a() {
            if b {
                ˇc
            }
        }
    "});
}
