use super::*;

#[gpui::test]
async fn test_select_to_start_end_of_larger_syntax_node(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    // Test Group 1.1: Cursor in String - First Jump (Select to End)
    let text = r#"let msg = "foo bar baz";"#.unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 14)..DisplayPoint::new(DisplayRow(0), 14)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = "fooˇ bar baz";"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = "foo« bar bazˇ»";"#}, cx);
    });

    // Test Group 1.2: Cursor in String - Second Jump (Select to End)
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = "foo« bar baz"ˇ»;"#}, cx);
    });

    // Test Group 1.3: Cursor in String - Third Jump (Select to End)
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = "foo« bar baz";ˇ»"#}, cx);
    });

    // Test Group 1.4: Cursor in String - First Jump (Select to Start)
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 18)..DisplayPoint::new(DisplayRow(0), 18)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = "foo barˇ baz";"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_start_of_larger_syntax_node(&SelectToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = "«ˇfoo bar» baz";"#}, cx);
    });

    // Test Group 1.5: Cursor in String - Second Jump (Select to Start)
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_start_of_larger_syntax_node(&SelectToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = «ˇ"foo bar» baz";"#}, cx);
    });

    // Test Group 1.6: Cursor in String - Third Jump (Select to Start)
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_start_of_larger_syntax_node(&SelectToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"«ˇlet msg = "foo bar» baz";"#}, cx);
    });

    // Test Group 2.1: Let Statement Progression (Select to End)
    let text = r#"
fn main() {
    let x = "hello";
}
"#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 9)..DisplayPoint::new(DisplayRow(1), 9)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let xˇ = "hello";
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r##"
                fn main() {
                    let x« = "hello";ˇ»
                }
            "##},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x« = "hello";
                }ˇ»
            "#},
            cx,
        );
    });

    // Test Group 2.2a: From Inside String Content Node To String Content Boundary
    let text = r#"let x = "hello";"#.unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 12)..DisplayPoint::new(DisplayRow(0), 12)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "helˇlo";"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_start_of_larger_syntax_node(&SelectToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "«ˇhel»lo";"#}, cx);
    });

    // Test Group 2.2b: From Edge of String Content Node To String Literal Boundary
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 9)..DisplayPoint::new(DisplayRow(0), 9)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "ˇhello";"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_start_of_larger_syntax_node(&SelectToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = «ˇ"»hello";"#}, cx);
    });

    // Test Group 3.1: Create Selection from Cursor (Select to End)
    let text = r#"let x = "hello world";"#.unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 14)..DisplayPoint::new(DisplayRow(0), 14)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "helloˇ world";"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "hello« worldˇ»";"#}, cx);
    });

    // Test Group 3.2: Extend Existing Selection (Select to End)
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 11)..DisplayPoint::new(DisplayRow(0), 17)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "he«llo woˇ»rld";"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "he«llo worldˇ»";"#}, cx);
    });

    // Test Group 4.1: Multiple Cursors - All Expand to Different Syntax Nodes
    let text = r#"let x = "hello"; let y = 42;"#.unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                // Cursor inside string content
                DisplayPoint::new(DisplayRow(0), 12)..DisplayPoint::new(DisplayRow(0), 12),
                // Cursor at let statement semicolon
                DisplayPoint::new(DisplayRow(0), 18)..DisplayPoint::new(DisplayRow(0), 18),
                // Cursor inside integer literal
                DisplayPoint::new(DisplayRow(0), 26)..DisplayPoint::new(DisplayRow(0), 26),
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "helˇlo"; lˇet y = 4ˇ2;"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "hel«loˇ»"; l«et y = 42;ˇ»"#}, cx);
    });

    // Test Group 4.2: Multiple Cursors on Separate Lines
    let text = r#"
let x = "hello";
let y = 42;
"#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 12)..DisplayPoint::new(DisplayRow(0), 12),
                DisplayPoint::new(DisplayRow(1), 9)..DisplayPoint::new(DisplayRow(1), 9),
            ]);
        });
    });

    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                let x = "helˇlo";
                let y = 4ˇ2;
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                let x = "hel«loˇ»";
                let y = 4«2ˇ»;
            "#},
            cx,
        );
    });

    // Test Group 5.1: Nested Function Calls
    let text = r#"let result = foo(bar("arg"));"#.unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 22)..DisplayPoint::new(DisplayRow(0), 22)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let result = foo(bar("ˇarg"));"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let result = foo(bar("«argˇ»"));"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let result = foo(bar("«arg"ˇ»));"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let result = foo(bar("«arg")ˇ»);"#}, cx);
    });

    // Test Group 6.1: Block Comments
    let text = r#"let x = /* multi
                             line
                             comment */;"#
        .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 16)..DisplayPoint::new(DisplayRow(0), 16)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
let x = /* multiˇ
line
comment */;"#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
let x = /* multi«
line
comment */ˇ»;"#},
            cx,
        );
    });

    // Test Group 6.2: Array/Vector Literals
    let text = r#"let arr = [1, 2, 3];"#.unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 11)..DisplayPoint::new(DisplayRow(0), 11)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let arr = [ˇ1, 2, 3];"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let arr = [«1ˇ», 2, 3];"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let arr = [«1, 2, 3]ˇ»;"#}, cx);
    });
}
