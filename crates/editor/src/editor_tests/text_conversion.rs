use super::*;

#[gpui::test]
async fn test_convert_indentation_to_spaces(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(3)
    });

    let mut cx = EditorTestContext::new(cx).await;

    // MULTI SELECTION
    // Ln.1 "«" tests empty lines
    // Ln.9 tests just leading whitespace
    cx.set_state(indoc! {"
        «
        abc                 // No indentationˇ»
        «\tabc              // 1 tabˇ»
        \t\tabc «      ˇ»   // 2 tabs
        \t ab«c             // Tab followed by space
         \tabc              // Space followed by tab (3 spaces should be the result)
        \t \t  \t   \tabc   // Mixed indentation (tab conversion depends on the column)
           abˇ»ˇc   ˇ    ˇ  // Already space indented«
        \t
        \tabc\tdef          // Only the leading tab is manipulatedˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.convert_indentation_to_spaces(&ConvertIndentationToSpaces, window, cx);
    });
    cx.assert_editor_state(
        indoc! {"
            «
            abc                 // No indentation
               abc              // 1 tab
                  abc          // 2 tabs
                abc             // Tab followed by space
               abc              // Space followed by tab (3 spaces should be the result)
                           abc   // Mixed indentation (tab conversion depends on the column)
               abc         // Already space indented
               ·
               abc\tdef          // Only the leading tab is manipulatedˇ»
        "}
        .replace("·", "")
        .as_str(), // · used as placeholder to prevent format-on-save from removing whitespace
    );

    // Test on just a few lines, the others should remain unchanged
    // Only lines (3, 5, 10, 11) should change
    cx.set_state(
        indoc! {"
            ·
            abc                 // No indentation
            \tabcˇ               // 1 tab
            \t\tabc             // 2 tabs
            \t abcˇ              // Tab followed by space
             \tabc              // Space followed by tab (3 spaces should be the result)
            \t \t  \t   \tabc   // Mixed indentation (tab conversion depends on the column)
               abc              // Already space indented
            «\t
            \tabc\tdef          // Only the leading tab is manipulatedˇ»
        "}
        .replace("·", "")
        .as_str(), // · used as placeholder to prevent format-on-save from removing whitespace
    );
    cx.update_editor(|e, window, cx| {
        e.convert_indentation_to_spaces(&ConvertIndentationToSpaces, window, cx);
    });
    cx.assert_editor_state(
        indoc! {"
            ·
            abc                 // No indentation
            «   abc               // 1 tabˇ»
            \t\tabc             // 2 tabs
            «    abc              // Tab followed by spaceˇ»
             \tabc              // Space followed by tab (3 spaces should be the result)
            \t \t  \t   \tabc   // Mixed indentation (tab conversion depends on the column)
               abc              // Already space indented
            «   ·
               abc\tdef          // Only the leading tab is manipulatedˇ»
        "}
        .replace("·", "")
        .as_str(), // · used as placeholder to prevent format-on-save from removing whitespace
    );

    // SINGLE SELECTION
    // Ln.1 "«" tests empty lines
    // Ln.9 tests just leading whitespace
    cx.set_state(indoc! {"
        «
        abc                 // No indentation
        \tabc               // 1 tab
        \t\tabc             // 2 tabs
        \t abc              // Tab followed by space
         \tabc              // Space followed by tab (3 spaces should be the result)
        \t \t  \t   \tabc   // Mixed indentation (tab conversion depends on the column)
           abc              // Already space indented
        \t
        \tabc\tdef          // Only the leading tab is manipulatedˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.convert_indentation_to_spaces(&ConvertIndentationToSpaces, window, cx);
    });
    cx.assert_editor_state(
        indoc! {"
            «
            abc                 // No indentation
               abc               // 1 tab
                  abc             // 2 tabs
                abc              // Tab followed by space
               abc              // Space followed by tab (3 spaces should be the result)
                           abc   // Mixed indentation (tab conversion depends on the column)
               abc              // Already space indented
               ·
               abc\tdef          // Only the leading tab is manipulatedˇ»
        "}
        .replace("·", "")
        .as_str(), // · used as placeholder to prevent format-on-save from removing whitespace
    );
}

#[gpui::test]
async fn test_convert_indentation_to_tabs(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(3)
    });

    let mut cx = EditorTestContext::new(cx).await;

    // MULTI SELECTION
    // Ln.1 "«" tests empty lines
    // Ln.11 tests just leading whitespace
    cx.set_state(indoc! {"
        «
        abˇ»ˇc                 // No indentation
         abc    ˇ        ˇ    // 1 space (< 3 so dont convert)
          abc  «             // 2 spaces (< 3 so dont convert)
           abc              // 3 spaces (convert)
             abc ˇ»           // 5 spaces (1 tab + 2 spaces)
        «\tˇ»\t«\tˇ»abc           // Already tab indented
        «\t abc              // Tab followed by space
         \tabc              // Space followed by tab (should be consumed due to tab)
        \t \t  \t   \tabc   // Mixed indentation (first 3 spaces are consumed, the others are converted)
           \tˇ»  «\t
           abcˇ»   \t ˇˇˇ        // Only the leading spaces should be converted
    "});
    cx.update_editor(|e, window, cx| {
        e.convert_indentation_to_tabs(&ConvertIndentationToTabs, window, cx);
    });
    cx.assert_editor_state(indoc! {"
        «
        abc                 // No indentation
         abc                // 1 space (< 3 so dont convert)
          abc               // 2 spaces (< 3 so dont convert)
        \tabc              // 3 spaces (convert)
        \t  abc            // 5 spaces (1 tab + 2 spaces)
        \t\t\tabc           // Already tab indented
        \t abc              // Tab followed by space
        \tabc              // Space followed by tab (should be consumed due to tab)
        \t\t\t\t\tabc   // Mixed indentation (first 3 spaces are consumed, the others are converted)
        \t\t\t
        \tabc   \t         // Only the leading spaces should be convertedˇ»
    "});

    // Test on just a few lines, the other should remain unchanged
    // Only lines (4, 8, 11, 12) should change
    cx.set_state(
        indoc! {"
            ·
            abc                 // No indentation
             abc                // 1 space (< 3 so dont convert)
              abc               // 2 spaces (< 3 so dont convert)
            «   abc              // 3 spaces (convert)ˇ»
                 abc            // 5 spaces (1 tab + 2 spaces)
            \t\t\tabc           // Already tab indented
            \t abc              // Tab followed by space
             \tabc      ˇ        // Space followed by tab (should be consumed due to tab)
               \t\t  \tabc      // Mixed indentation
            \t \t  \t   \tabc   // Mixed indentation
               \t  \tˇ
            «   abc   \t         // Only the leading spaces should be convertedˇ»
        "}
        .replace("·", "")
        .as_str(), // · used as placeholder to prevent format-on-save from removing whitespace
    );
    cx.update_editor(|e, window, cx| {
        e.convert_indentation_to_tabs(&ConvertIndentationToTabs, window, cx);
    });
    cx.assert_editor_state(
        indoc! {"
            ·
            abc                 // No indentation
             abc                // 1 space (< 3 so dont convert)
              abc               // 2 spaces (< 3 so dont convert)
            «\tabc              // 3 spaces (convert)ˇ»
                 abc            // 5 spaces (1 tab + 2 spaces)
            \t\t\tabc           // Already tab indented
            \t abc              // Tab followed by space
            «\tabc              // Space followed by tab (should be consumed due to tab)ˇ»
               \t\t  \tabc      // Mixed indentation
            \t \t  \t   \tabc   // Mixed indentation
            «\t\t\t
            \tabc   \t         // Only the leading spaces should be convertedˇ»
        "}
        .replace("·", "")
        .as_str(), // · used as placeholder to prevent format-on-save from removing whitespace
    );

    // SINGLE SELECTION
    // Ln.1 "«" tests empty lines
    // Ln.11 tests just leading whitespace
    cx.set_state(indoc! {"
        «
        abc                 // No indentation
         abc                // 1 space (< 3 so dont convert)
          abc               // 2 spaces (< 3 so dont convert)
           abc              // 3 spaces (convert)
             abc            // 5 spaces (1 tab + 2 spaces)
        \t\t\tabc           // Already tab indented
        \t abc              // Tab followed by space
         \tabc              // Space followed by tab (should be consumed due to tab)
        \t \t  \t   \tabc   // Mixed indentation (first 3 spaces are consumed, the others are converted)
           \t  \t
           abc   \t         // Only the leading spaces should be convertedˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.convert_indentation_to_tabs(&ConvertIndentationToTabs, window, cx);
    });
    cx.assert_editor_state(indoc! {"
        «
        abc                 // No indentation
         abc                // 1 space (< 3 so dont convert)
          abc               // 2 spaces (< 3 so dont convert)
        \tabc              // 3 spaces (convert)
        \t  abc            // 5 spaces (1 tab + 2 spaces)
        \t\t\tabc           // Already tab indented
        \t abc              // Tab followed by space
        \tabc              // Space followed by tab (should be consumed due to tab)
        \t\t\t\t\tabc   // Mixed indentation (first 3 spaces are consumed, the others are converted)
        \t\t\t
        \tabc   \t         // Only the leading spaces should be convertedˇ»
    "});
}

#[gpui::test]
async fn test_toggle_case(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    // If all lower case -> upper case
    cx.set_state(indoc! {"
        «hello worldˇ»
    "});
    cx.update_editor(|e, window, cx| e.toggle_case(&ToggleCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «HELLO WORLDˇ»
    "});

    // If all upper case -> lower case
    cx.set_state(indoc! {"
        «HELLO WORLDˇ»
    "});
    cx.update_editor(|e, window, cx| e.toggle_case(&ToggleCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «hello worldˇ»
    "});

    // If any upper case characters are identified -> lower case
    // This matches JetBrains IDEs
    cx.set_state(indoc! {"
        «hEllo worldˇ»
    "});
    cx.update_editor(|e, window, cx| e.toggle_case(&ToggleCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «hello worldˇ»
    "});
}

#[gpui::test]
async fn test_convert_to_sentence_case(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        «implement-windows-supportˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.convert_to_sentence_case(&ConvertToSentenceCase, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «Implement windows supportˇ»
    "});
}

#[gpui::test]
async fn test_convert_to_base64(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    // Encode a plain text selection
    cx.set_state(indoc! {"
        «helloˇ»
    "});
    cx.update_editor(|e, window, cx| e.convert_to_base64(&ConvertToBase64, window, cx));
    cx.assert_editor_state(indoc! {"
        «aGVsbG8=ˇ»
    "});

    // Decode a valid base64 selection
    cx.set_state(indoc! {"
        «aGVsbG8=ˇ»
    "});
    cx.update_editor(|e, window, cx| e.convert_from_base64(&ConvertFromBase64, window, cx));
    cx.assert_editor_state(indoc! {"
        «helloˇ»
    "});

    // Decode invalid base64 — should leave text unchanged
    cx.set_state(indoc! {"
        «not!!!ˇ»
    "});
    cx.update_editor(|e, window, cx| e.convert_from_base64(&ConvertFromBase64, window, cx));
    cx.assert_editor_state(indoc! {"
        «not!!!ˇ»
    "});
}

#[gpui::test]
fn test_manipulate_text_handles_cross_excerpt_edit_that_applies_differently(
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let buffer_1 = cx.new(|cx| {
        let mut buffer = Buffer::local("ab", cx);
        // The selected multibuffer range starts in this excerpt, but edits to
        // it are skipped because the underlying buffer is read-only.
        buffer.set_capability(language::Capability::ReadOnly, cx);
        buffer
    });
    let buffer_2 = cx.new(|cx| Buffer::local("cd", cx));
    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(0, 2)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(0, 2)],
            0,
            cx,
        );
        multibuffer
    });

    cx.add_window(|window, cx| {
        let mut editor = build_editor(multibuffer, window, cx);
        let len = editor.buffer().read(cx).len(cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_ranges([MultiBufferOffset(0)..len])
        });

        // No-op transformations should not be sent through `MultiBuffer::edit`.
        editor.manipulate_text(window, cx, |text| text.to_string());
        assert_eq!(buffer_1.read(cx).text(), "ab");
        assert_eq!(buffer_2.read(cx).text(), "cd");

        // A real replacement can apply differently than requested; selection
        // remapping should follow the actual edit instead of predicted offsets.
        editor.manipulate_text(window, cx, |_| "replacement".to_string());
        assert_eq!(buffer_1.read(cx).text(), "ab");
        assert_eq!(buffer_2.read(cx).text(), "");

        editor
    });
}
