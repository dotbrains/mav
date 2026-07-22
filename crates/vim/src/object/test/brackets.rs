use super::*;

#[gpui::test]
async fn test_anybrackets_object(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "b",
            AnyBrackets,
            Some("vim_operator == a || vim_operator == i || vim_operator == cs"),
        )]);
    });

    const TEST_CASES: &[(&str, &str, &str, Mode)] = &[
        (
            "c i b",
            indoc! {"
                    {
                        {
                            ˇprint('hello')
                        }
                    }
                "},
            indoc! {"
                    {
                        {
                            ˇ
                        }
                    }
                "},
            Mode::Insert,
        ),
        // Bracket (Parentheses)
        (
            "c i b",
            "Thisˇ is a (simple [quote]) example.",
            "This is a (ˇ) example.",
            Mode::Insert,
        ),
        (
            "c i b",
            "This is a [simple (qˇuote)] example.",
            "This is a [simple (ˇ)] example.",
            Mode::Insert,
        ),
        (
            "c a b",
            "This is a [simple (qˇuote)] example.",
            "This is a [simple ˇ] example.",
            Mode::Insert,
        ),
        (
            "c a b",
            "Thisˇ is a (simple [quote]) example.",
            "This is a ˇ example.",
            Mode::Insert,
        ),
        (
            "c i b",
            "This is a (qˇuote) example.",
            "This is a (ˇ) example.",
            Mode::Insert,
        ),
        (
            "c a b",
            "This is a (qˇuote) example.",
            "This is a ˇ example.",
            Mode::Insert,
        ),
        (
            "d i b",
            "This is a (qˇuote) example.",
            "This is a (ˇ) example.",
            Mode::Normal,
        ),
        (
            "d a b",
            "This is a (qˇuote) example.",
            "This is a ˇ example.",
            Mode::Normal,
        ),
        // Square brackets
        (
            "c i b",
            "This is a [qˇuote] example.",
            "This is a [ˇ] example.",
            Mode::Insert,
        ),
        (
            "c a b",
            "This is a [qˇuote] example.",
            "This is a ˇ example.",
            Mode::Insert,
        ),
        (
            "d i b",
            "This is a [qˇuote] example.",
            "This is a [ˇ] example.",
            Mode::Normal,
        ),
        (
            "d a b",
            "This is a [qˇuote] example.",
            "This is a ˇ example.",
            Mode::Normal,
        ),
        // Curly brackets
        (
            "c i b",
            "This is a {qˇuote} example.",
            "This is a {ˇ} example.",
            Mode::Insert,
        ),
        (
            "c a b",
            "This is a {qˇuote} example.",
            "This is a ˇ example.",
            Mode::Insert,
        ),
        (
            "d i b",
            "This is a {qˇuote} example.",
            "This is a {ˇ} example.",
            Mode::Normal,
        ),
        (
            "d a b",
            "This is a {qˇuote} example.",
            "This is a ˇ example.",
            Mode::Normal,
        ),
    ];

    for (keystrokes, initial_state, expected_state, expected_mode) in TEST_CASES {
        cx.set_state(initial_state, Mode::Normal);

        cx.simulate_keystrokes(keystrokes);

        cx.assert_state(expected_state, *expected_mode);
    }

    const INVALID_CASES: &[(&str, &str, Mode)] = &[
        ("c i b", "this is a (qˇuote example.", Mode::Normal), // Missing closing bracket
        ("c a b", "this is a (qˇuote example.", Mode::Normal), // Missing closing bracket
        ("d i b", "this is a (qˇuote example.", Mode::Normal), // Missing closing bracket
        ("d a b", "this is a (qˇuote example.", Mode::Normal), // Missing closing bracket
        ("c i b", "this is a [qˇuote example.", Mode::Normal), // Missing closing square bracket
        ("c a b", "this is a [qˇuote example.", Mode::Normal), // Missing closing square bracket
        ("d i b", "this is a [qˇuote example.", Mode::Normal), // Missing closing square bracket
        ("d a b", "this is a [qˇuote example.", Mode::Normal), // Missing closing square bracket
        ("c i b", "this is a {qˇuote example.", Mode::Normal), // Missing closing curly bracket
        ("c a b", "this is a {qˇuote example.", Mode::Normal), // Missing closing curly bracket
        ("d i b", "this is a {qˇuote example.", Mode::Normal), // Missing closing curly bracket
        ("d a b", "this is a {qˇuote example.", Mode::Normal), // Missing closing curly bracket
    ];

    for (keystrokes, initial_state, mode) in INVALID_CASES {
        cx.set_state(initial_state, Mode::Normal);

        cx.simulate_keystrokes(keystrokes);

        cx.assert_state(initial_state, *mode);
    }
}

#[gpui::test]
async fn test_minibrackets_object(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "b",
            MiniBrackets,
            Some("vim_operator == a || vim_operator == i || vim_operator == cs"),
        )]);
    });

    const TEST_CASES: &[(&str, &str, &str, Mode)] = &[
        // Special cases from mini.ai plugin
        // Current line has more priority for the cover or next algorithm, to avoid changing curly brackets which is supper anoying
        // Same behavior as mini.ai plugin
        (
            "c i b",
            indoc! {"
                    {
                        {
                            ˇprint('hello')
                        }
                    }
                "},
            indoc! {"
                    {
                        {
                            print(ˇ)
                        }
                    }
                "},
            Mode::Insert,
        ),
        // If the current line doesn't have brackets then it should consider if the caret is inside an external bracket
        // Same behavior as mini.ai plugin
        (
            "c i b",
            indoc! {"
                    {
                        {
                            ˇ
                            print('hello')
                        }
                    }
                "},
            indoc! {"
                    {
                        {ˇ}
                    }
                "},
            Mode::Insert,
        ),
        // If you are in the open bracket then it has higher priority
        (
            "c i b",
            indoc! {"
                    «{ˇ»
                        {
                            print('hello')
                        }
                    }
                "},
            indoc! {"
                    {ˇ}
                "},
            Mode::Insert,
        ),
        // If you are in the close bracket then it has higher priority
        (
            "c i b",
            indoc! {"
                    {
                        {
                            print('hello')
                        }
                    «}ˇ»
                "},
            indoc! {"
                    {ˇ}
                "},
            Mode::Insert,
        ),
        // Bracket (Parentheses)
        (
            "c i b",
            "Thisˇ is a (simple [quote]) example.",
            "This is a (ˇ) example.",
            Mode::Insert,
        ),
        (
            "c i b",
            "This is a [simple (qˇuote)] example.",
            "This is a [simple (ˇ)] example.",
            Mode::Insert,
        ),
        (
            "c a b",
            "This is a [simple (qˇuote)] example.",
            "This is a [simple ˇ] example.",
            Mode::Insert,
        ),
        (
            "c a b",
            "Thisˇ is a (simple [quote]) example.",
            "This is a ˇ example.",
            Mode::Insert,
        ),
        (
            "c i b",
            "This is a (qˇuote) example.",
            "This is a (ˇ) example.",
            Mode::Insert,
        ),
        (
            "c a b",
            "This is a (qˇuote) example.",
            "This is a ˇ example.",
            Mode::Insert,
        ),
        (
            "d i b",
            "This is a (qˇuote) example.",
            "This is a (ˇ) example.",
            Mode::Normal,
        ),
        (
            "d a b",
            "This is a (qˇuote) example.",
            "This is a ˇ example.",
            Mode::Normal,
        ),
        // Square brackets
        (
            "c i b",
            "This is a [qˇuote] example.",
            "This is a [ˇ] example.",
            Mode::Insert,
        ),
        (
            "c a b",
            "This is a [qˇuote] example.",
            "This is a ˇ example.",
            Mode::Insert,
        ),
        (
            "d i b",
            "This is a [qˇuote] example.",
            "This is a [ˇ] example.",
            Mode::Normal,
        ),
        (
            "d a b",
            "This is a [qˇuote] example.",
            "This is a ˇ example.",
            Mode::Normal,
        ),
        // Curly brackets
        (
            "c i b",
            "This is a {qˇuote} example.",
            "This is a {ˇ} example.",
            Mode::Insert,
        ),
        (
            "c a b",
            "This is a {qˇuote} example.",
            "This is a ˇ example.",
            Mode::Insert,
        ),
        (
            "d i b",
            "This is a {qˇuote} example.",
            "This is a {ˇ} example.",
            Mode::Normal,
        ),
        (
            "d a b",
            "This is a {qˇuote} example.",
            "This is a ˇ example.",
            Mode::Normal,
        ),
    ];

    for (keystrokes, initial_state, expected_state, expected_mode) in TEST_CASES {
        cx.set_state(initial_state, Mode::Normal);
        cx.buffer(|buffer, _| buffer.parsing_idle()).await;
        cx.simulate_keystrokes(keystrokes);
        cx.assert_state(expected_state, *expected_mode);
    }

    const INVALID_CASES: &[(&str, &str, Mode)] = &[
        ("c i b", "this is a (qˇuote example.", Mode::Normal), // Missing closing bracket
        ("c a b", "this is a (qˇuote example.", Mode::Normal), // Missing closing bracket
        ("d i b", "this is a (qˇuote example.", Mode::Normal), // Missing closing bracket
        ("d a b", "this is a (qˇuote example.", Mode::Normal), // Missing closing bracket
        ("c i b", "this is a [qˇuote example.", Mode::Normal), // Missing closing square bracket
        ("c a b", "this is a [qˇuote example.", Mode::Normal), // Missing closing square bracket
        ("d i b", "this is a [qˇuote example.", Mode::Normal), // Missing closing square bracket
        ("d a b", "this is a [qˇuote example.", Mode::Normal), // Missing closing square bracket
        ("c i b", "this is a {qˇuote example.", Mode::Normal), // Missing closing curly bracket
        ("c a b", "this is a {qˇuote example.", Mode::Normal), // Missing closing curly bracket
        ("d i b", "this is a {qˇuote example.", Mode::Normal), // Missing closing curly bracket
        ("d a b", "this is a {qˇuote example.", Mode::Normal), // Missing closing curly bracket
    ];

    for (keystrokes, initial_state, mode) in INVALID_CASES {
        cx.set_state(initial_state, Mode::Normal);
        cx.buffer(|buffer, _| buffer.parsing_idle()).await;
        cx.simulate_keystrokes(keystrokes);
        cx.assert_state(initial_state, *mode);
    }
}

#[gpui::test]
async fn test_minibrackets_multibuffer(cx: &mut gpui::TestAppContext) {
    // Initialize test context with the TypeScript language loaded, so we
    // can actually get brackets definition.
    let mut cx = VimTestContext::new(cx, true).await;

    // Update `b` to `MiniBrackets` so we can later use it when simulating
    // keystrokes.
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new("b", MiniBrackets, None)]);
    });

    let (editor, cx) = cx.add_window_view(|window, cx| {
        let multi_buffer = MultiBuffer::build_multi(
            [
                ("111\n222\n333\n444\n", vec![Point::row_range(0..2)]),
                ("111\na {bracket} example\n", vec![Point::row_range(0..2)]),
            ],
            cx,
        );

        // In order for the brackets to actually be found, we need to update
        // the language used for the second buffer. This is something that
        // is handled automatically when simply using `VimTestContext::new`
        // but, since this is being set manually, the language isn't
        // automatically set.
        let editor = Editor::new(EditorMode::full(), multi_buffer.clone(), None, window, cx);
        let buffer_ids = multi_buffer
            .read(cx)
            .snapshot(cx)
            .excerpts()
            .map(|excerpt| excerpt.context.start.buffer_id)
            .collect::<Vec<_>>();
        if let Some(buffer) = multi_buffer.read(cx).buffer(buffer_ids[1]) {
            buffer.update(cx, |buffer, cx| {
                buffer.set_language(Some(language::rust_lang()), cx);
            })
        };

        editor
    });

    let mut cx = EditorTestContext::for_editor_in(editor.clone(), cx).await;

    cx.assert_excerpts_with_selections(indoc! {"
            [EXCERPT]
            ˇ111
            222
            [EXCERPT]
            111
            a {bracket} example
            "
    });

    cx.simulate_keystrokes("j j j j f r");
    cx.assert_excerpts_with_selections(indoc! {"
            [EXCERPT]
            111
            222
            [EXCERPT]
            111
            a {bˇracket} example
            "
    });

    cx.simulate_keystrokes("d i b");
    cx.assert_excerpts_with_selections(indoc! {"
            [EXCERPT]
            111
            222
            [EXCERPT]
            111
            a {ˇ} example
            "
    });
}

#[gpui::test]
async fn test_minibrackets_trailing_space(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state("(trailingˇ whitespace          )")
        .await;
    cx.simulate_shared_keystrokes("v i b").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("escape y i b").await;
    cx.shared_clipboard()
        .await
        .assert_eq("trailing whitespace          ");
}
