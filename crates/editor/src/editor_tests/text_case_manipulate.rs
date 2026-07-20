use super::*;

#[gpui::test]
async fn test_manipulate_text(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    // Test convert_to_upper_case()
    cx.set_state(indoc! {"
        «hello worldˇ»
    "});
    cx.update_editor(|e, window, cx| e.convert_to_upper_case(&ConvertToUpperCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «HELLO WORLDˇ»
    "});

    // Test convert_to_lower_case()
    cx.set_state(indoc! {"
        «HELLO WORLDˇ»
    "});
    cx.update_editor(|e, window, cx| e.convert_to_lower_case(&ConvertToLowerCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «hello worldˇ»
    "});

    // Test multiple line, single selection case
    cx.set_state(indoc! {"
        «The quick brown
        fox jumps over
        the lazy dogˇ»
    "});
    cx.update_editor(|e, window, cx| e.convert_to_title_case(&ConvertToTitleCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «The Quick Brown
        Fox Jumps Over
        The Lazy Dogˇ»
    "});

    // Test multiple line, single selection case
    cx.set_state(indoc! {"
        «The quick brown
        fox jumps over
        the lazy dogˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.convert_to_upper_camel_case(&ConvertToUpperCamelCase, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «TheQuickBrown
        FoxJumpsOver
        TheLazyDogˇ»
    "});

    // From here on out, test more complex cases of manipulate_text()

    // Test no selection case - should affect words cursors are in
    // Cursor at beginning, middle, and end of word
    cx.set_state(indoc! {"
        ˇhello big beauˇtiful worldˇ
    "});
    cx.update_editor(|e, window, cx| e.convert_to_upper_case(&ConvertToUpperCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «HELLOˇ» big «BEAUTIFULˇ» «WORLDˇ»
    "});

    // Test multiple selections on a single line and across multiple lines
    cx.set_state(indoc! {"
        «Theˇ» quick «brown
        foxˇ» jumps «overˇ»
        the «lazyˇ» dog
    "});
    cx.update_editor(|e, window, cx| e.convert_to_upper_case(&ConvertToUpperCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «THEˇ» quick «BROWN
        FOXˇ» jumps «OVERˇ»
        the «LAZYˇ» dog
    "});

    // Test case where text length grows
    cx.set_state(indoc! {"
        «tschüßˇ»
    "});
    cx.update_editor(|e, window, cx| e.convert_to_upper_case(&ConvertToUpperCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «TSCHÜSSˇ»
    "});

    // Test to make sure we don't crash when text shrinks
    cx.set_state(indoc! {"
        aaa_bbbˇ
    "});
    cx.update_editor(|e, window, cx| {
        e.convert_to_lower_camel_case(&ConvertToLowerCamelCase, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «aaaBbbˇ»
    "});

    // Test to make sure we all aware of the fact that each word can grow and shrink
    // Final selections should be aware of this fact
    cx.set_state(indoc! {"
        aaa_bˇbb bbˇb_ccc ˇccc_ddd
    "});
    cx.update_editor(|e, window, cx| {
        e.convert_to_lower_camel_case(&ConvertToLowerCamelCase, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «aaaBbbˇ» «bbbCccˇ» «cccDddˇ»
    "});

    cx.set_state(indoc! {"
        «hElLo, WoRld!ˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.convert_to_opposite_case(&ConvertToOppositeCase, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «HeLlO, wOrLD!ˇ»
    "});

    // Test that case conversions backed by `to_case` preserve leading/trailing whitespace.
    cx.set_state(indoc! {"
        «    hello worldˇ»
    "});
    cx.update_editor(|e, window, cx| e.convert_to_title_case(&ConvertToTitleCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «    Hello Worldˇ»
    "});

    cx.set_state(indoc! {"
        «    hello worldˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.convert_to_upper_camel_case(&ConvertToUpperCamelCase, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «    HelloWorldˇ»
    "});

    cx.set_state(indoc! {"
        «    hello worldˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.convert_to_lower_camel_case(&ConvertToLowerCamelCase, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «    helloWorldˇ»
    "});

    cx.set_state(indoc! {"
        «    hello worldˇ»
    "});
    cx.update_editor(|e, window, cx| e.convert_to_snake_case(&ConvertToSnakeCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «    hello_worldˇ»
    "});

    cx.set_state(indoc! {"
        «    hello worldˇ»
    "});
    cx.update_editor(|e, window, cx| e.convert_to_kebab_case(&ConvertToKebabCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «    hello-worldˇ»
    "});

    cx.set_state(indoc! {"
        «    hello worldˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.convert_to_sentence_case(&ConvertToSentenceCase, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «    Hello worldˇ»
    "});

    cx.set_state(indoc! {"
        «    hello world\t\tˇ»
    "});
    cx.update_editor(|e, window, cx| e.convert_to_title_case(&ConvertToTitleCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «    Hello World\t\tˇ»
    "});

    cx.set_state(indoc! {"
        «    hello world\t\tˇ»
    "});
    cx.update_editor(|e, window, cx| e.convert_to_snake_case(&ConvertToSnakeCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «    hello_world\t\tˇ»
    "});

    // Test selections with `line_mode() = true`.
    cx.update_editor(|editor, _window, _cx| editor.selections.set_line_mode(true));
    cx.set_state(indoc! {"
        «The quick brown
        fox jumps over
        tˇ»he lazy dog
    "});
    cx.update_editor(|e, window, cx| e.convert_to_upper_case(&ConvertToUpperCase, window, cx));
    cx.assert_editor_state(indoc! {"
        «THE QUICK BROWN
        FOX JUMPS OVER
        THE LAZY DOGˇ»
    "});
}
