use super::*;

#[gpui::test]
async fn test_newline_task_list_continuation(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = Some(2.try_into().unwrap());
    });

    let markdown_language = languages::language("markdown", tree_sitter_md::LANGUAGE.into());
    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));

    // Case 1: Adding newline after (whitespace + prefix + any non-whitespace) adds marker
    cx.set_state(indoc! {"
        - [ ] taskˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [ ] task
        - [ ] ˇ
    "});

    // Case 2: Works with checked task items too
    cx.set_state(indoc! {"
        - [x] completed taskˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [x] completed task
        - [ ] ˇ
    "});

    // Case 2.1: Works with uppercase checked marker too
    cx.set_state(indoc! {"
        - [X] completed taskˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [X] completed task
        - [ ] ˇ
    "});

    // Case 3: Cursor position doesn't matter - content after marker is what counts
    cx.set_state(indoc! {"
        - [ ] taˇsk
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [ ] ta
        - [ ] ˇsk
    "});

    // Case 4: Adding newline after (whitespace + prefix + some whitespace) does NOT add marker
    cx.set_state(indoc! {"
        - [ ]  ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(
        indoc! {"
        - [ ]$$
        ˇ
    "}
        .replace("$", " ")
        .as_str(),
    );

    // Case 5: Adding newline with content adds marker preserving indentation
    cx.set_state(indoc! {"
        - [ ] task
          - [ ] indentedˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [ ] task
          - [ ] indented
          - [ ] ˇ
    "});

    // Case 6: Adding newline with cursor right after prefix, unindents
    cx.set_state(indoc! {"
        - [ ] task
          - [ ] sub task
            - [ ] ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [ ] task
          - [ ] sub task
          - [ ] ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;

    // Case 7: Adding newline with cursor right after prefix, removes marker
    cx.assert_editor_state(indoc! {"
        - [ ] task
          - [ ] sub task
        - [ ] ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [ ] task
          - [ ] sub task
        ˇ
    "});

    // Case 8: Cursor before or inside prefix does not add marker
    cx.set_state(indoc! {"
        ˇ- [ ] task
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"

        ˇ- [ ] task
    "});

    cx.set_state(indoc! {"
        - [ˇ ] task
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [
        ˇ
        ] task
    "});
}

#[gpui::test]
async fn test_newline_unordered_list_continuation(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = Some(2.try_into().unwrap());
    });

    let markdown_language = languages::language("markdown", tree_sitter_md::LANGUAGE.into());
    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));

    // Case 1: Adding newline after (whitespace + marker + any non-whitespace) adds marker
    cx.set_state(indoc! {"
        - itemˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - item
        - ˇ
    "});

    // Case 2: Works with different markers
    cx.set_state(indoc! {"
        * starred itemˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        * starred item
        * ˇ
    "});

    cx.set_state(indoc! {"
        + plus itemˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        + plus item
        + ˇ
    "});

    // Case 3: Cursor position doesn't matter - content after marker is what counts
    cx.set_state(indoc! {"
        - itˇem
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - it
        - ˇem
    "});

    // Case 4: Adding newline after (whitespace + marker + some whitespace) does NOT add marker
    cx.set_state(indoc! {"
        -  ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(
        indoc! {"
        - $
        ˇ
    "}
        .replace("$", " ")
        .as_str(),
    );

    // Case 5: Adding newline with content adds marker preserving indentation
    cx.set_state(indoc! {"
        - item
          - indentedˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - item
          - indented
          - ˇ
    "});

    // Case 6: Adding newline with cursor right after marker, unindents
    cx.set_state(indoc! {"
        - item
          - sub item
            - ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - item
          - sub item
          - ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;

    // Case 7: Adding newline with cursor right after marker, removes marker
    cx.assert_editor_state(indoc! {"
        - item
          - sub item
        - ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - item
          - sub item
        ˇ
    "});

    // Case 8: Cursor before or inside prefix does not add marker
    cx.set_state(indoc! {"
        ˇ- item
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"

        ˇ- item
    "});

    cx.set_state(indoc! {"
        -ˇ item
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        -
        ˇitem
    "});

    update_test_language_settings(&mut cx, &|settings| {
        settings.defaults.tab_size = Some(4.try_into().unwrap());
    });

    // Case 9: Empty list item unindent works when tab size is larger than list indentation
    cx.set_state(indoc! {"
        - item
          - sub item
          - ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - item
          - sub item
        - ˇ
    "});

    // Case 10: Empty list item unindent moves to the previous tab stop
    cx.set_state(
        indoc! {"
        $$$$$$- ˇ
    "}
        .replace("$", " ")
        .as_str(),
    );
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(
        indoc! {"
        $$$$- ˇ
    "}
        .replace("$", " ")
        .as_str(),
    );
}

#[gpui::test]
async fn test_newline_ordered_list_continuation(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = Some(2.try_into().unwrap());
    });

    let markdown_language = languages::language("markdown", tree_sitter_md::LANGUAGE.into());
    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));

    // Case 1: Adding newline after (whitespace + marker + any non-whitespace) increments number
    cx.set_state(indoc! {"
        1. first itemˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        1. first item
        2. ˇ
    "});

    // Case 2: Works with larger numbers
    cx.set_state(indoc! {"
        10. tenth itemˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        10. tenth item
        11. ˇ
    "});

    // Case 3: Cursor position doesn't matter - content after marker is what counts
    cx.set_state(indoc! {"
        1. itˇem
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        1. it
        2. ˇem
    "});

    // Case 4: Adding newline after (whitespace + marker + some whitespace) does NOT add marker
    cx.set_state(indoc! {"
        1.  ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(
        indoc! {"
        1. $
        ˇ
    "}
        .replace("$", " ")
        .as_str(),
    );

    // Case 5: Adding newline with content adds marker preserving indentation
    cx.set_state(indoc! {"
        1. item
          2. indentedˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        1. item
          2. indented
          3. ˇ
    "});

    // Case 6: Adding newline with cursor right after marker, unindents
    cx.set_state(indoc! {"
        1. item
          2. sub item
            3. ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        1. item
          2. sub item
          1. ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;

    // Case 7: Adding newline with cursor right after marker, removes marker
    cx.assert_editor_state(indoc! {"
        1. item
          2. sub item
        1. ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        1. item
          2. sub item
        ˇ
    "});

    // Case 8: Cursor before or inside prefix does not add marker
    cx.set_state(indoc! {"
        ˇ1. item
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"

        ˇ1. item
    "});

    cx.set_state(indoc! {"
        1ˇ. item
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        1
        ˇ. item
    "});
}
