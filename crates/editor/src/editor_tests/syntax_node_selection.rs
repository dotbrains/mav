use super::*;

#[gpui::test]
async fn test_select_larger_smaller_syntax_node(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let text = r#"
        use mod1::mod2::{mod3, mod4};

        fn fn_1(param1: bool, param2: &str) {
            let var1 = "text";
        }
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 25)..DisplayPoint::new(DisplayRow(0), 25),
                DisplayPoint::new(DisplayRow(2), 24)..DisplayPoint::new(DisplayRow(2), 12),
                DisplayPoint::new(DisplayRow(3), 18)..DisplayPoint::new(DisplayRow(3), 18),
            ]);
        });
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, «mod4ˇ»};

                fn fn_1«ˇ(param1: bool, param2: &str)» {
                    let var1 = "«textˇ»";
                }
            "#},
            cx,
        );
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::«{mod3, mod4}ˇ»;

                «ˇfn fn_1(param1: bool, param2: &str) {
                    let var1 = "text";
                }»
            "#},
            cx,
        );
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    assert_eq!(
        editor.update(cx, |editor, cx| editor
            .selections
            .display_ranges(&editor.display_snapshot(cx))),
        &[DisplayPoint::new(DisplayRow(5), 0)..DisplayPoint::new(DisplayRow(0), 0)]
    );

    // Trying to expand the selected syntax node one more time has no effect.
    editor.update_in(cx, |editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    assert_eq!(
        editor.update(cx, |editor, cx| editor
            .selections
            .display_ranges(&editor.display_snapshot(cx))),
        &[DisplayPoint::new(DisplayRow(5), 0)..DisplayPoint::new(DisplayRow(0), 0)]
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.select_smaller_syntax_node(&SelectSmallerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::«{mod3, mod4}ˇ»;

                «ˇfn fn_1(param1: bool, param2: &str) {
                    let var1 = "text";
                }»
            "#},
            cx,
        );
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.select_smaller_syntax_node(&SelectSmallerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, «mod4ˇ»};

                fn fn_1«ˇ(param1: bool, param2: &str)» {
                    let var1 = "«textˇ»";
                }
            "#},
            cx,
        );
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.select_smaller_syntax_node(&SelectSmallerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, moˇd4};

                fn fn_1(para«ˇm1: bool, pa»ram2: &str) {
                    let var1 = "teˇxt";
                }
            "#},
            cx,
        );
    });

    // Trying to shrink the selected syntax node one more time has no effect.
    editor.update_in(cx, |editor, window, cx| {
        editor.select_smaller_syntax_node(&SelectSmallerSyntaxNode, window, cx);
    });
    editor.update_in(cx, |editor, _, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, moˇd4};

                fn fn_1(para«ˇm1: bool, pa»ram2: &str) {
                    let var1 = "teˇxt";
                }
            "#},
            cx,
        );
    });

    // Ensure that we keep expanding the selection if the larger selection starts or ends within
    // a fold.
    editor.update_in(cx, |editor, window, cx| {
        editor.fold_creases(
            vec![
                Crease::simple(
                    Point::new(0, 21)..Point::new(0, 24),
                    FoldPlaceholder::test(),
                ),
                Crease::simple(
                    Point::new(3, 20)..Point::new(3, 22),
                    FoldPlaceholder::test(),
                ),
            ],
            true,
            window,
            cx,
        );
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::«{mod3, mod4}ˇ»;

                fn fn_1«ˇ(param1: bool, param2: &str)» {
                    let var1 = "«textˇ»";
                }
            "#},
            cx,
        );
    });

    // Ensure multiple cursors have consistent direction after expanding
    editor.update_in(cx, |editor, window, cx| {
        editor.unfold_all(&UnfoldAll, window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 25)..DisplayPoint::new(DisplayRow(0), 25),
                DisplayPoint::new(DisplayRow(3), 18)..DisplayPoint::new(DisplayRow(3), 18),
            ]);
        });
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, «mod4ˇ»};

                fn fn_1(param1: bool, param2: &str) {
                    let var1 = "«textˇ»";
                }
            "#},
            cx,
        );
    });
}
