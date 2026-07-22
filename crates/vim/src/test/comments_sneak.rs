use super::*;

#[cfg(target_os = "macos")]
#[perf]
#[gpui::test]
async fn test_dw_eol(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_wrap(12).await;
    cx.set_shared_state("twelve ˇchar twelve char\ntwelve char")
        .await;
    cx.simulate_shared_keystrokes("d w").await;
    cx.shared_state()
        .await
        .assert_eq("twelve ˇtwelve char\ntwelve char");
}

#[perf]
#[gpui::test]
async fn test_toggle_comments(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    let language = std::sync::Arc::new(language::Language::new(
        language::LanguageConfig {
            line_comments: vec!["// ".into(), "//! ".into(), "/// ".into()],
            ..Default::default()
        },
        Some(language::tree_sitter_rust::LANGUAGE.into()),
    ));
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // works in normal model
    cx.set_state(
        indoc! {"
      ˇone
      two
      three
      "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("g c c");
    cx.assert_state(
        indoc! {"
          // ˇone
          two
          three
          "},
        Mode::Normal,
    );

    // works in visual mode
    cx.simulate_keystrokes("v j g c");
    cx.assert_state(
        indoc! {"
          // // ˇone
          // two
          three
          "},
        Mode::Normal,
    );

    // works in visual line mode
    cx.simulate_keystrokes("shift-v j g c");
    cx.assert_state(
        indoc! {"
          // ˇone
          two
          three
          "},
        Mode::Normal,
    );

    // works with count
    cx.simulate_keystrokes("g c 2 j");
    cx.assert_state(
        indoc! {"
            // // ˇone
            // two
            // three
            "},
        Mode::Normal,
    );

    // works with motion object
    cx.simulate_keystrokes("shift-g");
    cx.simulate_keystrokes("g c g g");
    cx.assert_state(
        indoc! {"
            // one
            two
            three
            ˇ"},
        Mode::Normal,
    );
}

#[perf]
#[gpui::test]
async fn test_toggle_block_comments(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    let language = std::sync::Arc::new(language::Language::new(
        language::LanguageConfig {
            block_comment: Some(language::BlockCommentConfig {
                start: "/* ".into(),
                prefix: "".into(),
                end: " */".into(),
                tab_size: 1,
            }),
            ..Default::default()
        },
        Some(language::tree_sitter_rust::LANGUAGE.into()),
    ));
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // works in normal mode with current-line shorthand
    cx.set_state(
        indoc! {"
        ˇone
        two
        three
        "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("g b c");
    cx.assert_state(
        indoc! {"
        /* ˇone */
        two
        three
        "},
        Mode::Normal,
    );

    // toggle off with cursor inside the comment
    cx.simulate_keystrokes("g b c");
    cx.assert_state(
        indoc! {"
        ˇone
        two
        three
        "},
        Mode::Normal,
    );

    // works in visual line mode (wraps full lines)
    cx.simulate_keystrokes("shift-v j g b");
    cx.assert_state(
        indoc! {"
        /* ˇone
        two */
        three
        "},
        Mode::Normal,
    );

    // works in visual mode and restores the cursor to the selection start
    cx.set_state(
        indoc! {"
        «oneˇ»
        two
        three
        "},
        Mode::Visual,
    );
    cx.simulate_keystrokes("g b");
    cx.assert_state(
        indoc! {"
        /* ˇone */
        two
        three
        "},
        Mode::Normal,
    );

    // works with multiple visual selections and restores each cursor
    cx.set_state(
        indoc! {"
        «oneˇ» «twoˇ»
        three
        "},
        Mode::Visual,
    );
    cx.simulate_keystrokes("g b");
    cx.assert_state(
        indoc! {"
        /* ˇone */ /* ˇtwo */
        three
        "},
        Mode::Normal,
    );

    // works with count
    cx.set_state(
        indoc! {"
        ˇone
        two
        three
        "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("g b 2 j");
    cx.assert_state(
        indoc! {"
        /* ˇone
        two
        three */
        "},
        Mode::Normal,
    );

    // works with motion object
    cx.simulate_keystrokes("shift-g");
    cx.simulate_keystrokes("g b g g");
    cx.assert_state(
        indoc! {"
        one
        two
        three
        ˇ"},
        Mode::Normal,
    );
}

#[perf]
#[gpui::test]
async fn test_find_multibyte(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(r#"<label for="guests">ˇPočet hostů</label>"#)
        .await;

    cx.simulate_shared_keystrokes("c t < o escape").await;
    cx.shared_state()
        .await
        .assert_eq(r#"<label for="guests">ˇo</label>"#);
}

#[perf]
#[gpui::test]
async fn test_sneak(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.update(|_window, cx| {
        cx.bind_keys([
            KeyBinding::new(
                "s",
                PushSneak { first_char: None },
                Some("vim_mode == normal"),
            ),
            KeyBinding::new(
                "shift-s",
                PushSneakBackward { first_char: None },
                Some("vim_mode == normal"),
            ),
            KeyBinding::new(
                "shift-s",
                PushSneakBackward { first_char: None },
                Some("vim_mode == visual"),
            ),
        ])
    });

    // Sneak forwards multibyte & multiline
    cx.set_state(
        indoc! {
            r#"<labelˇ for="guests">
                    Počet hostů
                </label>"#
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("s t ů");
    cx.assert_state(
        indoc! {
            r#"<label for="guests">
                Počet hosˇtů
            </label>"#
        },
        Mode::Normal,
    );

    // Visual sneak backwards multibyte & multiline
    cx.simulate_keystrokes("v S < l");
    cx.assert_state(
        indoc! {
            r#"«ˇ<label for="guests">
                Počet host»ů
            </label>"#
        },
        Mode::Visual,
    );

    // Sneak backwards repeated
    cx.set_state(r#"11 12 13 ˇ14"#, Mode::Normal);
    cx.simulate_keystrokes("S space 1");
    cx.assert_state(r#"11 12ˇ 13 14"#, Mode::Normal);
    cx.simulate_keystrokes(";");
    cx.assert_state(r#"11ˇ 12 13 14"#, Mode::Normal);
}

#[perf]
#[gpui::test]
async fn test_plus_minus(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "one
           two
        thrˇee
    "})
        .await;

    cx.simulate_shared_keystrokes("-").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("-").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("+").await;
    cx.shared_state().await.assert_matches();
}
