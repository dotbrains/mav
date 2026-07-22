use super::*;

#[gpui::test]
async fn test_delete_surrounding_character_objects(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    for (start, end) in SURROUNDING_OBJECTS {
        let marked_string = SURROUNDING_MARKER_STRING
            .replace('`', &start.to_string())
            .replace('\'', &end.to_string());

        cx.simulate_at_each_offset(&format!("d i {start}"), &marked_string)
            .await
            .assert_matches();
        cx.simulate_at_each_offset(&format!("d i {end}"), &marked_string)
            .await
            .assert_matches();
        cx.simulate_at_each_offset(&format!("d a {start}"), &marked_string)
            .await
            .assert_matches();
        cx.simulate_at_each_offset(&format!("d a {end}"), &marked_string)
            .await
            .assert_matches();
    }
}

#[gpui::test]
async fn test_anyquotes_object(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "q",
            AnyQuotes,
            Some("vim_operator == a || vim_operator == i || vim_operator == cs"),
        )]);
    });

    const TEST_CASES: &[(&str, &str, &str, Mode)] = &[
        // the false string in the middle should be considered
        (
            "c i q",
            "'first' false ˇstring 'second'",
            "'first'ˇ'second'",
            Mode::Insert,
        ),
        // Single quotes
        (
            "c i q",
            "Thisˇ is a 'quote' example.",
            "This is a 'ˇ' example.",
            Mode::Insert,
        ),
        (
            "c a q",
            "Thisˇ is a 'quote' example.",
            "This is a ˇexample.",
            Mode::Insert,
        ),
        (
            "c i q",
            "This is a \"simple 'qˇuote'\" example.",
            "This is a \"simple 'ˇ'\" example.",
            Mode::Insert,
        ),
        (
            "c a q",
            "This is a \"simple 'qˇuote'\" example.",
            "This is a \"simpleˇ\" example.",
            Mode::Insert,
        ),
        (
            "c i q",
            "This is a 'qˇuote' example.",
            "This is a 'ˇ' example.",
            Mode::Insert,
        ),
        (
            "c a q",
            "This is a 'qˇuote' example.",
            "This is a ˇexample.",
            Mode::Insert,
        ),
        (
            "d i q",
            "This is a 'qˇuote' example.",
            "This is a 'ˇ' example.",
            Mode::Normal,
        ),
        (
            "d a q",
            "This is a 'qˇuote' example.",
            "This is a ˇexample.",
            Mode::Normal,
        ),
        // Double quotes
        (
            "c i q",
            "This is a \"qˇuote\" example.",
            "This is a \"ˇ\" example.",
            Mode::Insert,
        ),
        (
            "c a q",
            "This is a \"qˇuote\" example.",
            "This is a ˇexample.",
            Mode::Insert,
        ),
        (
            "d i q",
            "This is a \"qˇuote\" example.",
            "This is a \"ˇ\" example.",
            Mode::Normal,
        ),
        (
            "d a q",
            "This is a \"qˇuote\" example.",
            "This is a ˇexample.",
            Mode::Normal,
        ),
        // Back quotes
        (
            "c i q",
            "This is a `qˇuote` example.",
            "This is a `ˇ` example.",
            Mode::Insert,
        ),
        (
            "c a q",
            "This is a `qˇuote` example.",
            "This is a ˇexample.",
            Mode::Insert,
        ),
        (
            "d i q",
            "This is a `qˇuote` example.",
            "This is a `ˇ` example.",
            Mode::Normal,
        ),
        (
            "d a q",
            "This is a `qˇuote` example.",
            "This is a ˇexample.",
            Mode::Normal,
        ),
    ];

    for (keystrokes, initial_state, expected_state, expected_mode) in TEST_CASES {
        cx.set_state(initial_state, Mode::Normal);

        cx.simulate_keystrokes(keystrokes);

        cx.assert_state(expected_state, *expected_mode);
    }

    const INVALID_CASES: &[(&str, &str, Mode)] = &[
        ("c i q", "this is a 'qˇuote example.", Mode::Normal), // Missing closing simple quote
        ("c a q", "this is a 'qˇuote example.", Mode::Normal), // Missing closing simple quote
        ("d i q", "this is a 'qˇuote example.", Mode::Normal), // Missing closing simple quote
        ("d a q", "this is a 'qˇuote example.", Mode::Normal), // Missing closing simple quote
        ("c i q", "this is a \"qˇuote example.", Mode::Normal), // Missing closing double quote
        ("c a q", "this is a \"qˇuote example.", Mode::Normal), // Missing closing double quote
        ("d i q", "this is a \"qˇuote example.", Mode::Normal), // Missing closing double quote
        ("d a q", "this is a \"qˇuote example.", Mode::Normal), // Missing closing back quote
        ("c i q", "this is a `qˇuote example.", Mode::Normal), // Missing closing back quote
        ("c a q", "this is a `qˇuote example.", Mode::Normal), // Missing closing back quote
        ("d i q", "this is a `qˇuote example.", Mode::Normal), // Missing closing back quote
        ("d a q", "this is a `qˇuote example.", Mode::Normal), // Missing closing back quote
    ];

    for (keystrokes, initial_state, mode) in INVALID_CASES {
        cx.set_state(initial_state, Mode::Normal);

        cx.simulate_keystrokes(keystrokes);

        cx.assert_state(initial_state, *mode);
    }
}

#[gpui::test]
async fn test_miniquotes_object(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_typescript(cx).await;

    const TEST_CASES: &[(&str, &str, &str, Mode)] = &[
        // Special cases from mini.ai plugin
        // the false string in the middle should not be considered
        (
            "c i q",
            "'first' false ˇstring 'second'",
            "'first' false string 'ˇ'",
            Mode::Insert,
        ),
        // Multiline support :)! Same behavior as mini.ai plugin
        (
            "c i q",
            indoc! {"
                    `
                    first
                    middle ˇstring
                    second
                    `
                "},
            indoc! {"
                    `ˇ`
                "},
            Mode::Insert,
        ),
        // If you are in the close quote and it is the only quote in the buffer, it should replace inside the quote
        // This is not working with the core motion ci' for this special edge case, so I am happy to fix it in MiniQuotes :)
        // Bug reference: https://github.com/mav-industries/mav/issues/23889
        ("c i q", "'quote«'ˇ»", "'ˇ'", Mode::Insert),
        // Single quotes
        (
            "c i q",
            "Thisˇ is a 'quote' example.",
            "This is a 'ˇ' example.",
            Mode::Insert,
        ),
        (
            "c a q",
            "Thisˇ is a 'quote' example.",
            "This is a ˇ example.", // same mini.ai plugin behavior
            Mode::Insert,
        ),
        (
            "c i q",
            "This is a \"simple 'qˇuote'\" example.",
            "This is a \"ˇ\" example.", // Not supported by Tree-sitter queries for now
            Mode::Insert,
        ),
        (
            "c a q",
            "This is a \"simple 'qˇuote'\" example.",
            "This is a ˇ example.", // Not supported by Tree-sitter queries for now
            Mode::Insert,
        ),
        (
            "c i q",
            "This is a 'qˇuote' example.",
            "This is a 'ˇ' example.",
            Mode::Insert,
        ),
        (
            "c a q",
            "This is a 'qˇuote' example.",
            "This is a ˇ example.", // same mini.ai plugin behavior
            Mode::Insert,
        ),
        (
            "d i q",
            "This is a 'qˇuote' example.",
            "This is a 'ˇ' example.",
            Mode::Normal,
        ),
        (
            "d a q",
            "This is a 'qˇuote' example.",
            "This is a ˇ example.", // same mini.ai plugin behavior
            Mode::Normal,
        ),
        // Double quotes
        (
            "c i q",
            "This is a \"qˇuote\" example.",
            "This is a \"ˇ\" example.",
            Mode::Insert,
        ),
        (
            "c a q",
            "This is a \"qˇuote\" example.",
            "This is a ˇ example.", // same mini.ai plugin behavior
            Mode::Insert,
        ),
        (
            "d i q",
            "This is a \"qˇuote\" example.",
            "This is a \"ˇ\" example.",
            Mode::Normal,
        ),
        (
            "d a q",
            "This is a \"qˇuote\" example.",
            "This is a ˇ example.", // same mini.ai plugin behavior
            Mode::Normal,
        ),
        // Back quotes
        (
            "c i q",
            "This is a `qˇuote` example.",
            "This is a `ˇ` example.",
            Mode::Insert,
        ),
        (
            "c a q",
            "This is a `qˇuote` example.",
            "This is a ˇ example.", // same mini.ai plugin behavior
            Mode::Insert,
        ),
        (
            "d i q",
            "This is a `qˇuote` example.",
            "This is a `ˇ` example.",
            Mode::Normal,
        ),
        (
            "d a q",
            "This is a `qˇuote` example.",
            "This is a ˇ example.", // same mini.ai plugin behavior
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
        ("c i q", "this is a 'qˇuote example.", Mode::Normal), // Missing closing simple quote
        ("c a q", "this is a 'qˇuote example.", Mode::Normal), // Missing closing simple quote
        ("d i q", "this is a 'qˇuote example.", Mode::Normal), // Missing closing simple quote
        ("d a q", "this is a 'qˇuote example.", Mode::Normal), // Missing closing simple quote
        ("c i q", "this is a \"qˇuote example.", Mode::Normal), // Missing closing double quote
        ("c a q", "this is a \"qˇuote example.", Mode::Normal), // Missing closing double quote
        ("d i q", "this is a \"qˇuote example.", Mode::Normal), // Missing closing double quote
        ("d a q", "this is a \"qˇuote example.", Mode::Normal), // Missing closing back quote
        ("c i q", "this is a `qˇuote example.", Mode::Normal), // Missing closing back quote
        ("c a q", "this is a `qˇuote example.", Mode::Normal), // Missing closing back quote
        ("d i q", "this is a `qˇuote example.", Mode::Normal), // Missing closing back quote
        ("d a q", "this is a `qˇuote example.", Mode::Normal), // Missing closing back quote
    ];

    for (keystrokes, initial_state, mode) in INVALID_CASES {
        cx.set_state(initial_state, Mode::Normal);
        cx.buffer(|buffer, _| buffer.parsing_idle()).await;
        cx.simulate_keystrokes(keystrokes);
        cx.assert_state(initial_state, *mode);
    }
}
