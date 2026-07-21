use super::*;

#[gpui::test]
async fn test_start_end_of_paragraph(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    let initial_state = indoc! {r"ˇabc
            def

            paragraph
            the second



            third and
            final"};

    // goes down once
    cx.set_shared_state(initial_state).await;
    cx.simulate_shared_keystrokes("}").await;
    cx.shared_state().await.assert_eq(indoc! {r"abc
            def
            ˇ
            paragraph
            the second



            third and
            final"});

    // goes up once
    cx.simulate_shared_keystrokes("{").await;
    cx.shared_state().await.assert_eq(initial_state);

    // goes down twice
    cx.simulate_shared_keystrokes("2 }").await;
    cx.shared_state().await.assert_eq(indoc! {r"abc
            def

            paragraph
            the second
            ˇ


            third and
            final"});

    // goes down over multiple blanks
    cx.simulate_shared_keystrokes("}").await;
    cx.shared_state().await.assert_eq(indoc! {r"abc
                def

                paragraph
                the second



                third and
                finaˇl"});

    // goes up twice
    cx.simulate_shared_keystrokes("2 {").await;
    cx.shared_state().await.assert_eq(indoc! {r"abc
                def
                ˇ
                paragraph
                the second



                third and
                final"});
}

#[gpui::test]
async fn test_paragraph_motion_with_whitespace_lines(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // Test that whitespace-only lines are NOT treated as paragraph boundaries
    // Per vim's :help paragraph - only truly empty lines are boundaries
    // Line 2 has 4 spaces (whitespace-only), line 4 is truly empty
    cx.set_shared_state("ˇfirst\n    \nstill first\n\nsecond")
        .await;
    cx.simulate_shared_keystrokes("}").await;

    // Should skip whitespace-only line and stop at truly empty line
    let mut shared_state = cx.shared_state().await;
    shared_state.assert_eq("first\n    \nstill first\nˇ\nsecond");
    shared_state.assert_matches();

    // Should go back to original position
    cx.simulate_shared_keystrokes("{").await;
    let mut shared_state = cx.shared_state().await;
    shared_state.assert_eq("ˇfirst\n    \nstill first\n\nsecond");
    shared_state.assert_matches();
}

#[gpui::test]
async fn test_matching(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {r"func ˇ(a string) {
                do(something(with<Types>.and_arrays[0, 2]))
            }"})
        .await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"func (a stringˇ) {
                do(something(with<Types>.and_arrays[0, 2]))
            }"});

    // test it works on the last character of the line
    cx.set_shared_state(indoc! {r"func (a string) ˇ{
            do(something(with<Types>.and_arrays[0, 2]))
            }"})
        .await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"func (a string) {
            do(something(with<Types>.and_arrays[0, 2]))
            ˇ}"});

    // test it works on immediate nesting
    cx.set_shared_state("ˇ{()}").await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq("{()ˇ}");
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq("ˇ{()}");

    // test it works on immediate nesting inside braces
    cx.set_shared_state("{\n    ˇ{()}\n}").await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq("{\n    {()ˇ}\n}");

    // test it jumps to the next paren on a line
    cx.set_shared_state("func ˇboop() {\n}").await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq("func boop(ˇ) {\n}");
}

#[gpui::test]
async fn test_matching_in_multibuffer(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    let (editor, cx) = cx.add_window_view(|window, cx| {
        let multi_buffer = MultiBuffer::build_multi(
            [
                (
                    "fn a() {\n    let x = 1;\n}\n",
                    vec![Point::row_range(0..3)],
                ),
                (
                    "fn b() {\n    let y = 2;\n}\n",
                    vec![Point::row_range(0..3)],
                ),
            ],
            cx,
        );

        let buffer_ids = multi_buffer
            .read(cx)
            .snapshot(cx)
            .excerpts()
            .map(|excerpt| excerpt.context.start.buffer_id)
            .collect::<Vec<_>>();

        for buffer_id in buffer_ids {
            if let Some(buffer) = multi_buffer.read(cx).buffer(buffer_id) {
                buffer.update(cx, |buffer, cx| {
                    buffer.set_language(Some(language::rust_lang()), cx);
                });
            }
        }

        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    let mut cx = EditorTestContext::for_editor_in(editor.clone(), cx).await;

    cx.simulate_keystrokes("j j j j f {");
    cx.assert_excerpts_with_selections(indoc! {"
            [EXCERPT]
            fn a() {
                let x = 1;
            }
            [EXCERPT]
            fn b() ˇ{
                let y = 2;
            }
            "
    });

    cx.simulate_keystrokes("%");
    cx.assert_excerpts_with_selections(indoc! {"
            [EXCERPT]
            fn a() {
                let x = 1;
            }
            [EXCERPT]
            fn b() {
                let y = 2;
            ˇ}
            "
    });

    cx.simulate_keystrokes("%");
    cx.assert_excerpts_with_selections(indoc! {"
            [EXCERPT]
            fn a() {
                let x = 1;
            }
            [EXCERPT]
            fn b() ˇ{
                let y = 2;
            }
            "
    });
}

#[gpui::test]
async fn test_matching_quotes_disabled(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // Bind % to Matching with match_quotes: false to match Neovim's behavior
    // (Neovim's % doesn't match quotes by default)
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "%",
            Matching {
                match_quotes: false,
            },
            None,
        )]);
    });

    cx.set_shared_state("one {two 'thˇree' four}").await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq("one ˇ{two 'three' four}");

    cx.set_shared_state("'hello wˇorld'").await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq("'hello wˇorld'");

    cx.set_shared_state(r#"func ("teˇst") {}"#).await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(r#"func ˇ("test") {}"#);

    cx.set_shared_state("ˇ'hello'").await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq("ˇ'hello'");

    cx.set_shared_state("'helloˇ'").await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq("'helloˇ'");

    cx.set_shared_state(indoc! {r"func (a string) {
                do('somethiˇng'))
            }"})
        .await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {r"func (a string) {
                doˇ('something'))
            }"});
}

#[gpui::test]
async fn test_matching_quotes_enabled(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_markdown_with_rust(cx).await;

    // Test default behavior (match_quotes: true as configured in keymap/vim.json)
    cx.set_state("one {two 'thˇree' four}", Mode::Normal);
    cx.simulate_keystrokes("%");
    cx.assert_state("one {two ˇ'three' four}", Mode::Normal);

    cx.set_state("'hello wˇorld'", Mode::Normal);
    cx.simulate_keystrokes("%");
    cx.assert_state("ˇ'hello world'", Mode::Normal);

    cx.set_state(r#"func ('teˇst') {}"#, Mode::Normal);
    cx.simulate_keystrokes("%");
    cx.assert_state(r#"func (ˇ'test') {}"#, Mode::Normal);

    cx.set_state("ˇ'hello'", Mode::Normal);
    cx.simulate_keystrokes("%");
    cx.assert_state("'helloˇ'", Mode::Normal);

    cx.set_state("'helloˇ'", Mode::Normal);
    cx.simulate_keystrokes("%");
    cx.assert_state("ˇ'hello'", Mode::Normal);

    cx.set_state(
        indoc! {r"func (a string) {
                do('somethiˇng'))
            }"},
        Mode::Normal,
    );
    cx.simulate_keystrokes("%");
    cx.assert_state(
        indoc! {r"func (a string) {
                do(ˇ'something'))
            }"},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_matching_comments(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {r"ˇ/*
          this is a comment
        */"})
        .await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(indoc! {r"/*
          this is a comment
        ˇ*/"});
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(indoc! {r"ˇ/*
          this is a comment
        */"});
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(indoc! {r"/*
          this is a comment
        ˇ*/"});
    cx.simulate_shared_keystrokes("k %").await;
    cx.shared_state().await.assert_eq(indoc! {r"/*
        ˇ  this is a comment
        */"});

    cx.set_shared_state("ˇ// comment").await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq("ˇ// comment");
}

#[gpui::test]
async fn test_matching_preprocessor_directives(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {r"
          #ˇif

          #else

          #endif
        "})
        .await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(indoc! {r"
          #if

          ˇ#else

          #endif
        "});

    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(indoc! {r"
          #if

          #else

          ˇ#endif
        "});

    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(indoc! {r"
          ˇ#if

          #else

          #endif
        "});

    cx.set_shared_state(indoc! {r"
          #ˇif
            #if

            #else

            #endif

          #else

          #endif
        "})
        .await;

    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(indoc! {r"
            #if
              #if

              #else

              #endif

            ˇ#else

            #endif
          "});

    cx.simulate_shared_keystrokes("% %").await;
    cx.shared_state().await.assert_eq(indoc! {r"
            ˇ#if
              #if

              #else

              #endif

            #else

            #endif
          "});
    cx.simulate_shared_keystrokes("j % % %").await;
    cx.shared_state().await.assert_eq(indoc! {r"
            #if
              ˇ#if

              #else

              #endif

            #else

            #endif
          "});

    cx.set_shared_state(indoc! {r"
          #if definedˇ(something)

          #endif
        "})
        .await;
    cx.simulate_shared_keystrokes("%").await;
    cx.shared_state().await.assert_eq(indoc! {r"
          #if defined(somethingˇ)

          #endif
        "});
    cx.simulate_shared_keystrokes("0 %").await;
    cx.shared_state().await.assert_eq(indoc! {r"
          #if defined(something)

          ˇ#endif
        "});
}
