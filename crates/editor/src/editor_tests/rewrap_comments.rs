use super::*;

#[gpui::test]
async fn test_rewrap_line_comment_in_go(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.languages.0.extend([(
            "Go".into(),
            LanguageSettingsContent {
                allow_rewrap: Some(language_settings::RewrapBehavior::InComments),
                preferred_line_length: Some(40),
                ..Default::default()
            },
        )])
    });

    let mut cx = EditorTestContext::new(cx).await;

    let go_lang = languages::language("go", tree_sitter_go::LANGUAGE.into());

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(go_lang), cx));
    cx.set_state(indoc! {"
        // Lorem ipsum dolor sit amet, consectetur adipiscing elit.ˇ
    "});
    cx.update_editor(|e, _, cx| e.rewrap(RewrapOptions::default(), cx));
    cx.assert_editor_state(indoc! {"
        // Lorem ipsum dolor sit amet,
        // consectetur adipiscing elit.ˇ
    "});
}

#[gpui::test]
async fn test_rewrap_line_comment_in_c(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.languages.0.extend([(
            "C".into(),
            LanguageSettingsContent {
                allow_rewrap: Some(language_settings::RewrapBehavior::InComments),
                preferred_line_length: Some(40),
                ..Default::default()
            },
        )])
    });

    let mut cx = EditorTestContext::new(cx).await;

    let c_lang = languages::language("c", tree_sitter_c::LANGUAGE.into());

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(c_lang), cx));
    cx.set_state(indoc! {"
        // Lorem ipsum dolor sit amet, consectetur adipiscing elit.ˇ
    "});
    cx.update_editor(|e, _, cx| e.rewrap(RewrapOptions::default(), cx));
    cx.assert_editor_state(indoc! {"
        // Lorem ipsum dolor sit amet,
        // consectetur adipiscing elit.ˇ
    "});
}

#[gpui::test]
async fn test_hard_wrap(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(git_commit_lang()), cx));
    cx.update_editor(|editor, _, cx| {
        editor.set_hard_wrap(Some(14), cx);
    });

    cx.set_state(indoc!(
        "
        one two three ˇ
        "
    ));
    cx.simulate_input("four");
    cx.run_until_parked();

    cx.assert_editor_state(indoc!(
        "
        one two three
        fourˇ
        "
    ));

    cx.update_editor(|editor, window, cx| {
        editor.newline(&Default::default(), window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc!(
        "
        one two three
        four
        ˇ
        "
    ));

    cx.simulate_input("five");
    cx.run_until_parked();
    cx.assert_editor_state(indoc!(
        "
        one two three
        four
        fiveˇ
        "
    ));

    cx.update_editor(|editor, window, cx| {
        editor.newline(&Default::default(), window, cx);
    });
    cx.run_until_parked();
    cx.simulate_input("# ");
    cx.run_until_parked();
    cx.assert_editor_state(indoc!(
        "
        one two three
        four
        five
        # ˇ
        "
    ));

    cx.update_editor(|editor, window, cx| {
        editor.newline(&Default::default(), window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc!(
        "
        one two three
        four
        five
        #\x20
        #ˇ
        "
    ));

    cx.simulate_input(" 6");
    cx.run_until_parked();
    cx.assert_editor_state(indoc!(
        "
        one two three
        four
        five
        #
        # 6ˇ
        "
    ));
}

#[gpui::test]
async fn test_cut_line_ends(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"The quick brownˇ"});
    cx.update_editor(|e, window, cx| e.cut_to_end_of_line(&CutToEndOfLine::default(), window, cx));
    cx.assert_editor_state(indoc! {"The quick brownˇ"});

    cx.set_state(indoc! {"The emacs foxˇ"});
    cx.update_editor(|e, window, cx| e.kill_ring_cut(&KillRingCut, window, cx));
    cx.assert_editor_state(indoc! {"The emacs foxˇ"});

    cx.set_state(indoc! {"
        The quick« brownˇ»
        fox jumps overˇ
        the lazy dog"});
    cx.update_editor(|e, window, cx| e.cut(&Cut, window, cx));
    cx.assert_editor_state(indoc! {"
        The quickˇ
        ˇthe lazy dog"});

    cx.set_state(indoc! {"
        The quick« brownˇ»
        fox jumps overˇ
        the lazy dog"});
    cx.update_editor(|e, window, cx| e.cut_to_end_of_line(&CutToEndOfLine::default(), window, cx));
    cx.assert_editor_state(indoc! {"
        The quickˇ
        fox jumps overˇthe lazy dog"});

    cx.set_state(indoc! {"
        The quick« brownˇ»
        fox jumps overˇ
        the lazy dog"});
    cx.update_editor(|e, window, cx| {
        e.cut_to_end_of_line(
            &CutToEndOfLine {
                stop_at_newlines: true,
            },
            window,
            cx,
        )
    });
    cx.assert_editor_state(indoc! {"
        The quickˇ
        fox jumps overˇ
        the lazy dog"});

    cx.set_state(indoc! {"
        The quick« brownˇ»
        fox jumps overˇ
        the lazy dog"});
    cx.update_editor(|e, window, cx| e.kill_ring_cut(&KillRingCut, window, cx));
    cx.assert_editor_state(indoc! {"
        The quickˇ
        fox jumps overˇthe lazy dog"});

    for selection in ["The quick «brownˇ» fox", "The quick «ˇbrown» fox"] {
        cx.set_state(selection);
        cx.update_editor(|e, window, cx| {
            e.cut_to_end_of_line(&CutToEndOfLine::default(), window, cx)
        });
        cx.assert_editor_state("The quick ˇ");
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text().as_deref().map(str::to_string)),
            Some("brown fox".to_string())
        );
    }
}
