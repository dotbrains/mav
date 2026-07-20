use super::*;

#[gpui::test]
async fn test_rewrap(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.languages.0.extend([
            (
                "Markdown".into(),
                LanguageSettingsContent {
                    allow_rewrap: Some(language_settings::RewrapBehavior::Anywhere),
                    preferred_line_length: Some(40),
                    ..Default::default()
                },
            ),
            (
                "Plain Text".into(),
                LanguageSettingsContent {
                    allow_rewrap: Some(language_settings::RewrapBehavior::Anywhere),
                    preferred_line_length: Some(40),
                    ..Default::default()
                },
            ),
            (
                "C++".into(),
                LanguageSettingsContent {
                    allow_rewrap: Some(language_settings::RewrapBehavior::InComments),
                    preferred_line_length: Some(40),
                    ..Default::default()
                },
            ),
            (
                "Python".into(),
                LanguageSettingsContent {
                    allow_rewrap: Some(language_settings::RewrapBehavior::InComments),
                    preferred_line_length: Some(40),
                    ..Default::default()
                },
            ),
            (
                "Rust".into(),
                LanguageSettingsContent {
                    allow_rewrap: Some(language_settings::RewrapBehavior::InComments),
                    preferred_line_length: Some(40),
                    ..Default::default()
                },
            ),
        ])
    });

    let mut cx = EditorTestContext::new(cx).await;

    let cpp_language = Arc::new(Language::new(
        LanguageConfig {
            name: "C++".into(),
            line_comments: vec!["// ".into()],
            ..LanguageConfig::default()
        },
        None,
    ));
    let python_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Python".into(),
            line_comments: vec!["# ".into()],
            ..LanguageConfig::default()
        },
        None,
    ));
    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            rewrap_prefixes: vec![
                regex::Regex::new("\\d+\\.\\s+").unwrap(),
                regex::Regex::new("[-*+]\\s+").unwrap(),
            ],
            ..LanguageConfig::default()
        },
        None,
    ));
    let rust_language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "Rust".into(),
                line_comments: vec!["// ".into(), "/// ".into()],
                ..LanguageConfig::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_override_query("[(line_comment)(block_comment)] @comment.inclusive")
        .unwrap(),
    );

    let plaintext_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Plain Text".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    // Test basic rewrapping of a long line with a cursor
    assert_rewrap(
        indoc! {"
            // ˇThis is a long comment that needs to be wrapped.
        "},
        indoc! {"
            // ˇThis is a long comment that needs to
            // be wrapped.
        "},
        cpp_language.clone(),
        &mut cx,
    );

    // Test rewrapping a full selection
    assert_rewrap(
        indoc! {"
            «// This selected long comment needs to be wrapped.ˇ»"
        },
        indoc! {"
            «// This selected long comment needs to
            // be wrapped.ˇ»"
        },
        cpp_language.clone(),
        &mut cx,
    );

    // Test multiple cursors on different lines within the same paragraph are preserved after rewrapping
    assert_rewrap(
        indoc! {"
            // ˇThis is the first line.
            // Thisˇ is the second line.
            // This is the thirdˇ line, all part of one paragraph.
         "},
        indoc! {"
            // ˇThis is the first line. Thisˇ is the
            // second line. This is the thirdˇ line,
            // all part of one paragraph.
         "},
        cpp_language.clone(),
        &mut cx,
    );

    // Test multiple cursors in different paragraphs trigger separate rewraps
    assert_rewrap(
        indoc! {"
            // ˇThis is the first paragraph, first line.
            // ˇThis is the first paragraph, second line.

            // ˇThis is the second paragraph, first line.
            // ˇThis is the second paragraph, second line.
        "},
        indoc! {"
            // ˇThis is the first paragraph, first
            // line. ˇThis is the first paragraph,
            // second line.

            // ˇThis is the second paragraph, first
            // line. ˇThis is the second paragraph,
            // second line.
        "},
        cpp_language.clone(),
        &mut cx,
    );

    // Test that change in comment prefix (e.g., `//` to `///`) trigger separate rewraps
    assert_rewrap(
        indoc! {"
            «// A regular long long comment to be wrapped.
            /// A documentation long comment to be wrapped.ˇ»
          "},
        indoc! {"
            «// A regular long long comment to be
            // wrapped.
            /// A documentation long comment to be
            /// wrapped.ˇ»
          "},
        rust_language.clone(),
        &mut cx,
    );

    // Test that change in indentation level trigger separate rewraps
    assert_rewrap(
        indoc! {"
            fn foo() {
                «// This is a long comment at the base indent.
                    // This is a long comment at the next indent.ˇ»
            }
        "},
        indoc! {"
            fn foo() {
                «// This is a long comment at the
                // base indent.
                    // This is a long comment at the
                    // next indent.ˇ»
            }
        "},
        rust_language.clone(),
        &mut cx,
    );

    // Test that different comment prefix characters (e.g., '#') are handled correctly
    assert_rewrap(
        indoc! {"
            # ˇThis is a long comment using a pound sign.
        "},
        indoc! {"
            # ˇThis is a long comment using a pound
            # sign.
        "},
        python_language,
        &mut cx,
    );

    // Test rewrapping only affects comments, not code even when selected
    assert_rewrap(
        indoc! {"
            «/// This doc comment is long and should be wrapped.
            fn my_func(a: u32, b: u32, c: u32, d: u32, e: u32, f: u32) {}ˇ»
        "},
        indoc! {"
            «/// This doc comment is long and should
            /// be wrapped.
            fn my_func(a: u32, b: u32, c: u32, d: u32, e: u32, f: u32) {}ˇ»
        "},
        rust_language.clone(),
        &mut cx,
    );

    // Test that rewrapping works in Markdown documents where `allow_rewrap` is `Anywhere`
    assert_rewrap(
        indoc! {"
            # Header

            A long long long line of markdown text to wrap.ˇ
         "},
        indoc! {"
            # Header

            A long long long line of markdown text
            to wrap.ˇ
         "},
        markdown_language.clone(),
        &mut cx,
    );

    // Test that rewrapping boundary works and preserves relative indent for Markdown documents
    assert_rewrap(
        indoc! {"
            «1. This is a numbered list item that is very long and needs to be wrapped properly.
            2. This is a numbered list item that is very long and needs to be wrapped properly.
            - This is an unordered list item that is also very long and should not merge with the numbered item.ˇ»
        "},
        indoc! {"
            «1. This is a numbered list item that is
               very long and needs to be wrapped
               properly.
            2. This is a numbered list item that is
               very long and needs to be wrapped
               properly.
            - This is an unordered list item that is
              also very long and should not merge
              with the numbered item.ˇ»
        "},
        markdown_language.clone(),
        &mut cx,
    );

    // Test that rewrapping add indents for rewrapping boundary if not exists already.
    assert_rewrap(
        indoc! {"
            «1. This is a numbered list item that is
            very long and needs to be wrapped
            properly.
            2. This is a numbered list item that is
            very long and needs to be wrapped
            properly.
            - This is an unordered list item that is
            also very long and should not merge with
            the numbered item.ˇ»
        "},
        indoc! {"
            «1. This is a numbered list item that is
               very long and needs to be wrapped
               properly.
            2. This is a numbered list item that is
               very long and needs to be wrapped
               properly.
            - This is an unordered list item that is
              also very long and should not merge
              with the numbered item.ˇ»
        "},
        markdown_language.clone(),
        &mut cx,
    );

    // Test that rewrapping maintain indents even when they already exists.
    assert_rewrap(
        indoc! {"
            «1. This is a numbered list
               item that is very long and needs to be wrapped properly.
            2. This is a numbered list
               item that is very long and needs to be wrapped properly.
            - This is an unordered list item that is also very long and
              should not merge with the numbered item.ˇ»
        "},
        indoc! {"
            «1. This is a numbered list item that is
               very long and needs to be wrapped
               properly.
            2. This is a numbered list item that is
               very long and needs to be wrapped
               properly.
            - This is an unordered list item that is
              also very long and should not merge
              with the numbered item.ˇ»
        "},
        markdown_language.clone(),
        &mut cx,
    );

    // Test that empty selection rewrap on a numbered list item does not merge adjacent items
    assert_rewrap(
        indoc! {"
            1. This is the first numbered list item that is very long and needs to be wrapped properly.
            2. ˇThis is the second numbered list item that is also very long and needs to be wrapped.
            3. This is the third numbered list item, shorter.
        "},
        indoc! {"
            1. This is the first numbered list item
               that is very long and needs to be
               wrapped properly.
            2. ˇThis is the second numbered list item
               that is also very long and needs to
               be wrapped.
            3. This is the third numbered list item,
               shorter.
        "},
        markdown_language.clone(),
        &mut cx,
    );

    // Test that empty selection rewrap on a bullet list item does not merge adjacent items
    assert_rewrap(
        indoc! {"
            - This is the first bullet item that is very long and needs wrapping properly here.
            - ˇThis is the second bullet item that is also very long and needs to be wrapped.
            - This is the third bullet item, shorter.
        "},
        indoc! {"
            - This is the first bullet item that is
              very long and needs wrapping properly
              here.
            - ˇThis is the second bullet item that is
              also very long and needs to be
              wrapped.
            - This is the third bullet item,
              shorter.
        "},
        markdown_language,
        &mut cx,
    );

    // Test that rewrapping works in plain text where `allow_rewrap` is `Anywhere`
    assert_rewrap(
        indoc! {"
            ˇThis is a very long line of plain text that will be wrapped.
        "},
        indoc! {"
            ˇThis is a very long line of plain text
            that will be wrapped.
        "},
        plaintext_language.clone(),
        &mut cx,
    );

    // Test that non-commented code acts as a paragraph boundary within a selection
    assert_rewrap(
        indoc! {"
               «// This is the first long comment block to be wrapped.
               fn my_func(a: u32);
               // This is the second long comment block to be wrapped.ˇ»
           "},
        indoc! {"
               «// This is the first long comment block
               // to be wrapped.
               fn my_func(a: u32);
               // This is the second long comment block
               // to be wrapped.ˇ»
           "},
        rust_language,
        &mut cx,
    );

    // Test rewrapping multiple selections, including ones with blank lines or tabs
    assert_rewrap(
        indoc! {"
            «ˇThis is a very long line that will be wrapped.

            This is another paragraph in the same selection.»

            «\tThis is a very long indented line that will be wrapped.ˇ»
         "},
        indoc! {"
            «ˇThis is a very long line that will be
            wrapped.

            This is another paragraph in the same
            selection.»

            «\tThis is a very long indented line
            \tthat will be wrapped.ˇ»
         "},
        plaintext_language,
        &mut cx,
    );

    // Test that an empty comment line acts as a paragraph boundary
    assert_rewrap(
        indoc! {"
            // ˇThis is a long comment that will be wrapped.
            //
            // And this is another long comment that will also be wrapped.ˇ
         "},
        indoc! {"
            // ˇThis is a long comment that will be
            // wrapped.
            //
            // And this is another long comment that
            // will also be wrapped.ˇ
         "},
        cpp_language,
        &mut cx,
    );

    #[track_caller]
    fn assert_rewrap(
        unwrapped_text: &str,
        wrapped_text: &str,
        language: Arc<Language>,
        cx: &mut EditorTestContext,
    ) {
        cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
        cx.set_state(unwrapped_text);
        cx.update_editor(|e, _, cx| e.rewrap(RewrapOptions::default(), cx));
        cx.assert_editor_state(wrapped_text);
    }
}
