use super::*;

#[gpui::test]
async fn test_autoclose_quotes_with_multibyte_characters(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("python", tree_sitter_python::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    cx.set_state(indoc! {r#"
        def main():
            items = ["🎉", ˇ]
    "#});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("\"", window, cx);
    });
    cx.assert_editor_state(indoc! {r#"
        def main():
            items = ["🎉", "ˇ"]
    "#});
}

#[gpui::test]
async fn test_surround_with_pair(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig {
            brackets: BracketPairConfig {
                pairs: vec![
                    BracketPair {
                        start: "{".to_string(),
                        end: "}".to_string(),
                        close: true,
                        surround: true,
                        newline: true,
                    },
                    BracketPair {
                        start: "/* ".to_string(),
                        end: "*/".to_string(),
                        close: true,
                        surround: true,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let text = r#"
        a
        b
        c
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
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(2), 0)..DisplayPoint::new(DisplayRow(2), 1),
            ])
        });

        editor.handle_input("{", window, cx);
        editor.handle_input("{", window, cx);
        editor.handle_input("{", window, cx);
        assert_eq!(
            editor.text(cx),
            "
                {{{a}}}
                {{{b}}}
                {{{c}}}
            "
            .unindent()
        );
        assert_eq!(
            display_ranges(editor, cx),
            [
                DisplayPoint::new(DisplayRow(0), 3)..DisplayPoint::new(DisplayRow(0), 4),
                DisplayPoint::new(DisplayRow(1), 3)..DisplayPoint::new(DisplayRow(1), 4),
                DisplayPoint::new(DisplayRow(2), 3)..DisplayPoint::new(DisplayRow(2), 4)
            ]
        );

        editor.undo(&Undo, window, cx);
        editor.undo(&Undo, window, cx);
        editor.undo(&Undo, window, cx);
        assert_eq!(
            editor.text(cx),
            "
                a
                b
                c
            "
            .unindent()
        );
        assert_eq!(
            display_ranges(editor, cx),
            [
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(2), 0)..DisplayPoint::new(DisplayRow(2), 1)
            ]
        );

        // Ensure inserting the first character of a multi-byte bracket pair
        // doesn't surround the selections with the bracket.
        editor.handle_input("/", window, cx);
        assert_eq!(
            editor.text(cx),
            "
                /
                /
                /
            "
            .unindent()
        );
        assert_eq!(
            display_ranges(editor, cx),
            [
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(1), 1)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(2), 1)..DisplayPoint::new(DisplayRow(2), 1)
            ]
        );

        editor.undo(&Undo, window, cx);
        assert_eq!(
            editor.text(cx),
            "
                a
                b
                c
            "
            .unindent()
        );
        assert_eq!(
            display_ranges(editor, cx),
            [
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(2), 0)..DisplayPoint::new(DisplayRow(2), 1)
            ]
        );

        // Ensure inserting the last character of a multi-byte bracket pair
        // doesn't surround the selections with the bracket.
        editor.handle_input("*", window, cx);
        assert_eq!(
            editor.text(cx),
            "
                *
                *
                *
            "
            .unindent()
        );
        assert_eq!(
            display_ranges(editor, cx),
            [
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(1), 1)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(2), 1)..DisplayPoint::new(DisplayRow(2), 1)
            ]
        );
    });
}

#[gpui::test]
async fn test_delete_autoclose_pair(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig {
            brackets: BracketPairConfig {
                pairs: vec![BracketPair {
                    start: "{".to_string(),
                    end: "}".to_string(),
                    close: true,
                    surround: true,
                    newline: true,
                }],
                ..Default::default()
            },
            autoclose_before: "}".to_string(),
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let text = r#"
        a
        b
        c
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
            s.select_ranges([
                Point::new(0, 1)..Point::new(0, 1),
                Point::new(1, 1)..Point::new(1, 1),
                Point::new(2, 1)..Point::new(2, 1),
            ])
        });

        editor.handle_input("{", window, cx);
        editor.handle_input("{", window, cx);
        editor.handle_input("_", window, cx);
        assert_eq!(
            editor.text(cx),
            "
                a{{_}}
                b{{_}}
                c{{_}}
            "
            .unindent()
        );
        assert_eq!(
            editor
                .selections
                .ranges::<Point>(&editor.display_snapshot(cx)),
            [
                Point::new(0, 4)..Point::new(0, 4),
                Point::new(1, 4)..Point::new(1, 4),
                Point::new(2, 4)..Point::new(2, 4)
            ]
        );

        editor.backspace(&Default::default(), window, cx);
        editor.backspace(&Default::default(), window, cx);
        assert_eq!(
            editor.text(cx),
            "
                a{}
                b{}
                c{}
            "
            .unindent()
        );
        assert_eq!(
            editor
                .selections
                .ranges::<Point>(&editor.display_snapshot(cx)),
            [
                Point::new(0, 2)..Point::new(0, 2),
                Point::new(1, 2)..Point::new(1, 2),
                Point::new(2, 2)..Point::new(2, 2)
            ]
        );

        editor.delete_to_previous_word_start(&Default::default(), window, cx);
        assert_eq!(
            editor.text(cx),
            "
                a
                b
                c
            "
            .unindent()
        );
        assert_eq!(
            editor
                .selections
                .ranges::<Point>(&editor.display_snapshot(cx)),
            [
                Point::new(0, 1)..Point::new(0, 1),
                Point::new(1, 1)..Point::new(1, 1),
                Point::new(2, 1)..Point::new(2, 1)
            ]
        );
    });
}

#[gpui::test]
async fn test_always_treat_brackets_as_autoclosed_delete(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.always_treat_brackets_as_autoclosed = Some(true);
    });

    let mut cx = EditorTestContext::new(cx).await;

    let language = Arc::new(Language::new(
        LanguageConfig {
            brackets: BracketPairConfig {
                pairs: vec![
                    BracketPair {
                        start: "{".to_string(),
                        end: "}".to_string(),
                        close: true,
                        surround: true,
                        newline: true,
                    },
                    BracketPair {
                        start: "(".to_string(),
                        end: ")".to_string(),
                        close: true,
                        surround: true,
                        newline: true,
                    },
                    BracketPair {
                        start: "[".to_string(),
                        end: "]".to_string(),
                        close: false,
                        surround: true,
                        newline: true,
                    },
                ],
                ..Default::default()
            },
            autoclose_before: "})]".to_string(),
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    cx.language_registry().add(language.clone());
    cx.update_buffer(|buffer, cx| {
        buffer.set_language(Some(language), cx);
    });

    cx.set_state(
        &"
            {(ˇ)}
            [[ˇ]]
            {(ˇ)}
        "
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.backspace(&Default::default(), window, cx);
        editor.backspace(&Default::default(), window, cx);
    });

    cx.assert_editor_state(
        &"
            ˇ
            ˇ]]
            ˇ
        "
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.handle_input("{", window, cx);
        editor.handle_input("{", window, cx);
        editor.move_right(&MoveRight, window, cx);
        editor.move_right(&MoveRight, window, cx);
        editor.move_left(&MoveLeft, window, cx);
        editor.move_left(&MoveLeft, window, cx);
        editor.backspace(&Default::default(), window, cx);
    });

    cx.assert_editor_state(
        &"
            {ˇ}
            {ˇ}]]
            {ˇ}
        "
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.backspace(&Default::default(), window, cx);
    });

    cx.assert_editor_state(
        &"
            ˇ
            ˇ]]
            ˇ
        "
        .unindent(),
    );
}

#[gpui::test]
async fn test_auto_replace_emoji_shortcode(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let buffer = cx.new(|cx| Buffer::local("", cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));
    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.set_auto_replace_emoji_shortcode(true);

        editor.handle_input("Hello ", window, cx);
        editor.handle_input(":wave", window, cx);
        assert_eq!(editor.text(cx), "Hello :wave".unindent());

        editor.handle_input(":", window, cx);
        assert_eq!(editor.text(cx), "Hello 👋".unindent());

        editor.handle_input(" :smile", window, cx);
        assert_eq!(editor.text(cx), "Hello 👋 :smile".unindent());

        editor.handle_input(":", window, cx);
        assert_eq!(editor.text(cx), "Hello 👋 😄".unindent());

        // Ensure shortcode gets replaced when it is part of a word that only consists of emojis
        editor.handle_input(":wave", window, cx);
        assert_eq!(editor.text(cx), "Hello 👋 😄:wave".unindent());

        editor.handle_input(":", window, cx);
        assert_eq!(editor.text(cx), "Hello 👋 😄👋".unindent());

        editor.handle_input(":1", window, cx);
        assert_eq!(editor.text(cx), "Hello 👋 😄👋:1".unindent());

        editor.handle_input(":", window, cx);
        assert_eq!(editor.text(cx), "Hello 👋 😄👋:1:".unindent());

        // Ensure shortcode does not get replaced when it is part of a word
        editor.handle_input(" Test:wave", window, cx);
        assert_eq!(editor.text(cx), "Hello 👋 😄👋:1: Test:wave".unindent());

        editor.handle_input(":", window, cx);
        assert_eq!(editor.text(cx), "Hello 👋 😄👋:1: Test:wave:".unindent());

        editor.set_auto_replace_emoji_shortcode(false);

        // Ensure shortcode does not get replaced when auto replace is off
        editor.handle_input(" :wave", window, cx);
        assert_eq!(
            editor.text(cx),
            "Hello 👋 😄👋:1: Test:wave: :wave".unindent()
        );

        editor.handle_input(":", window, cx);
        assert_eq!(
            editor.text(cx),
            "Hello 👋 😄👋:1: Test:wave: :wave:".unindent()
        );
    });
}
