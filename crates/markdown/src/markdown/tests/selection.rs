use super::*;

#[gpui::test]
fn test_surrounding_word_range(cx: &mut TestAppContext) {
    let rendered = render_markdown("Hello world tesεζ", cx);

    // Test word selection for "Hello"
    let word_range = rendered.surrounding_word_range(2); // Simulate click on 'l' in "Hello"
    let selected_text = rendered.text_for_range(word_range);
    assert_eq!(selected_text, "Hello");

    // Test word selection for "world"
    let word_range = rendered.surrounding_word_range(7); // Simulate click on 'o' in "world"
    let selected_text = rendered.text_for_range(word_range);
    assert_eq!(selected_text, "world");

    // Test word selection for "tesεζ"
    let word_range = rendered.surrounding_word_range(14); // Simulate click on 's' in "tesεζ"
    let selected_text = rendered.text_for_range(word_range);
    assert_eq!(selected_text, "tesεζ");

    // Test word selection at word boundary (space)
    let word_range = rendered.surrounding_word_range(5); // Simulate click on space between "Hello" and "world", expect highlighting word to the left
    let selected_text = rendered.text_for_range(word_range);
    assert_eq!(selected_text, "Hello");
}

#[gpui::test]
fn test_surrounding_line_range(cx: &mut TestAppContext) {
    let rendered = render_markdown("First line\n\nSecond line\n\nThird lineεζ", cx);

    // Test getting line range for first line
    let line_range = rendered.surrounding_line_range(5); // Simulate click somewhere in first line
    let selected_text = rendered.text_for_range(line_range);
    assert_eq!(selected_text, "First line");

    // Test getting line range for second line
    let line_range = rendered.surrounding_line_range(13); // Simulate click at beginning in second line
    let selected_text = rendered.text_for_range(line_range);
    assert_eq!(selected_text, "Second line");

    // Test getting line range for third line
    let line_range = rendered.surrounding_line_range(37); // Simulate click at end of third line with multi-byte chars
    let selected_text = rendered.text_for_range(line_range);
    assert_eq!(selected_text, "Third lineεζ");
}

#[gpui::test]
fn test_selection_head_movement(cx: &mut TestAppContext) {
    let rendered = render_markdown("Hello world test", cx);

    let mut selection = Selection {
        start: 5,
        end: 5,
        reversed: false,
        pending: false,
        mode: SelectMode::Character,
    };

    // Test forward selection
    selection.set_head(10, &rendered);
    assert_eq!(selection.start, 5);
    assert_eq!(selection.end, 10);
    assert!(!selection.reversed);
    assert_eq!(selection.tail(), 5);

    // Test backward selection
    selection.set_head(2, &rendered);
    assert_eq!(selection.start, 2);
    assert_eq!(selection.end, 5);
    assert!(selection.reversed);
    assert_eq!(selection.tail(), 5);

    // Test forward selection again from reversed state
    selection.set_head(15, &rendered);
    assert_eq!(selection.start, 5);
    assert_eq!(selection.end, 15);
    assert!(!selection.reversed);
    assert_eq!(selection.tail(), 5);
}

#[gpui::test]
fn test_word_selection_drag(cx: &mut TestAppContext) {
    let rendered = render_markdown("Hello world test", cx);

    // Start with a simulated double-click on "world" (index 6-10)
    let word_range = rendered.surrounding_word_range(7); // Click on 'o' in "world"
    let mut selection = Selection {
        start: word_range.start,
        end: word_range.end,
        reversed: false,
        pending: true,
        mode: SelectMode::Word(word_range),
    };

    // Drag forward to "test" - should expand selection to include "test"
    selection.set_head(13, &rendered); // Index in "test"
    assert_eq!(selection.start, 6); // Start of "world"
    assert_eq!(selection.end, 16); // End of "test"
    assert!(!selection.reversed);
    let selected_text = rendered.text_for_range(selection.start..selection.end);
    assert_eq!(selected_text, "world test");

    // Drag backward to "Hello" - should expand selection to include "Hello"
    selection.set_head(2, &rendered); // Index in "Hello"
    assert_eq!(selection.start, 0); // Start of "Hello"
    assert_eq!(selection.end, 11); // End of "world" (original selection)
    assert!(selection.reversed);
    let selected_text = rendered.text_for_range(selection.start..selection.end);
    assert_eq!(selected_text, "Hello world");

    // Drag back within original word - should revert to original selection
    selection.set_head(8, &rendered); // Back within "world"
    assert_eq!(selection.start, 6); // Start of "world"
    assert_eq!(selection.end, 11); // End of "world"
    assert!(!selection.reversed);
    let selected_text = rendered.text_for_range(selection.start..selection.end);
    assert_eq!(selected_text, "world");
}

#[gpui::test]
fn test_selection_with_markdown_formatting(cx: &mut TestAppContext) {
    let rendered = render_markdown(
        "This is **bold** text, this is *italic* text, use `code` here",
        cx,
    );
    let word_range = rendered.surrounding_word_range(10); // Inside "bold"
    let selected_text = rendered.text_for_range(word_range);
    assert_eq!(selected_text, "bold");

    let word_range = rendered.surrounding_word_range(32); // Inside "italic"
    let selected_text = rendered.text_for_range(word_range);
    assert_eq!(selected_text, "italic");

    let word_range = rendered.surrounding_word_range(51); // Inside "code"
    let selected_text = rendered.text_for_range(word_range);
    assert_eq!(selected_text, "code");
}

#[test]
fn test_source_range_for_rendered_handles_split_chunks() {
    let mappings = vec![
        SourceMapping {
            rendered_index: 0,
            source_index: 20,
        },
        SourceMapping {
            rendered_index: 1,
            source_index: 21,
        },
        SourceMapping {
            rendered_index: 2,
            source_index: 22,
        },
    ];

    let range = source_range_for_rendered(&mappings, &(0..3)).unwrap();
    assert_eq!(range, 20..23);

    let range = source_range_for_rendered(&mappings, &(1..2)).unwrap();
    assert_eq!(range, 21..22);

    assert_eq!(source_range_for_rendered(&mappings, &(2..2)), None);
}

#[gpui::test]
fn test_inline_code_word_selection_excludes_backticks(cx: &mut TestAppContext) {
    // Test that double-clicking on inline code selects just the code content,
    // not the backticks. This verifies the fix for the bug where selecting
    // inline code would include the trailing backtick.
    let rendered = render_markdown("use `blah` here", cx);

    // Source layout: "use `blah` here"
    //                 0123456789...
    // The inline code "blah" is at source positions 5-8 (content range 5..9)

    // Click inside "blah" - should select just "blah", not "blah`"
    let word_range = rendered.surrounding_word_range(6); // 'l' in "blah"

    // text_for_range extracts from the rendered text (without backticks), so it
    // would return "blah" even with a wrong source range. We check it anyway.
    let selected_text = rendered.text_for_range(word_range.clone());
    assert_eq!(selected_text, "blah");

    // The source range is what matters for copy_as_markdown and selected_text,
    // which extract directly from the source. With the bug, this would be 5..10
    // which includes the closing backtick at position 9.
    assert_eq!(word_range, 5..9);
}

#[gpui::test]
fn test_surrounding_word_range_respects_word_characters(cx: &mut TestAppContext) {
    let rendered = render_markdown("foo.bar() baz", cx);

    // Double clicking on 'f' in "foo" - should select just "foo"
    let word_range = rendered.surrounding_word_range(0);
    let selected_text = rendered.text_for_range(word_range);
    assert_eq!(selected_text, "foo");

    // Double clicking on 'b' in "bar" - should select just "bar"
    let word_range = rendered.surrounding_word_range(4);
    let selected_text = rendered.text_for_range(word_range);
    assert_eq!(selected_text, "bar");

    // Double clicking on 'b' in "baz" - should select "baz"
    let word_range = rendered.surrounding_word_range(10);
    let selected_text = rendered.text_for_range(word_range);
    assert_eq!(selected_text, "baz");

    // Double clicking selects word characters in code blocks
    let javascript_language = Arc::new(Language::new(
        LanguageConfig {
            name: "JavaScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["js".to_string()],
                ..Default::default()
            },
            word_characters: ['$', '#'].into_iter().collect(),
            ..Default::default()
        },
        None,
    ));

    let language_registry = Arc::new(LanguageRegistry::test(cx.executor()));
    language_registry.add(javascript_language);

    let rendered = render_markdown_with_language_registry(
        "```javascript\n$foo #bar\n```",
        Some(language_registry),
        cx,
    );

    let word_range = rendered.surrounding_word_range(14);
    let selected_text = rendered.text_for_range(word_range);
    assert_eq!(selected_text, "$foo");

    let word_range = rendered.surrounding_word_range(19);
    let selected_text = rendered.text_for_range(word_range);
    assert_eq!(selected_text, "#bar");
}

#[gpui::test]
fn test_all_selection(cx: &mut TestAppContext) {
    let rendered = render_markdown("Hello world\n\nThis is a test\n\nwith multiple lines", cx);

    let total_length = rendered
        .lines
        .last()
        .map(|line| line.source_end)
        .unwrap_or(0);

    let mut selection = Selection {
        start: 0,
        end: total_length,
        reversed: false,
        pending: true,
        mode: SelectMode::All,
    };

    selection.set_head(5, &rendered); // Try to set head in middle
    assert_eq!(selection.start, 0);
    assert_eq!(selection.end, total_length);
    assert!(!selection.reversed);

    selection.set_head(25, &rendered); // Try to set head near end
    assert_eq!(selection.start, 0);
    assert_eq!(selection.end, total_length);
    assert!(!selection.reversed);

    let selected_text = rendered.text_for_range(selection.start..selection.end);
    assert_eq!(
        selected_text,
        "Hello world\nThis is a test\nwith multiple lines"
    );
}
