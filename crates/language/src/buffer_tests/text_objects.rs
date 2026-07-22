use super::*;

#[gpui::test]
fn test_text_objects(cx: &mut App) {
    let (text, ranges) = marked_text_ranges(
        indoc! {r#"
            impl Hello {
                fn say() -> u8 { return /* ˇhi */ 1 }
            }"#
        },
        false,
    );

    let buffer = cx.new(|cx| Buffer::local(text.clone(), cx).with_language(rust_lang(), cx));
    let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());

    let matches = snapshot
        .text_object_ranges(ranges[0].clone(), TreeSitterOptions::default())
        .map(|(range, text_object)| (&text[range], text_object))
        .collect::<Vec<_>>();

    assert_eq!(
        matches,
        &[
            ("/* hi */", TextObject::AroundComment),
            ("return /* hi */ 1", TextObject::InsideFunction),
            (
                "fn say() -> u8 { return /* hi */ 1 }",
                TextObject::AroundFunction
            ),
            (
                "fn say() -> u8 { return /* hi */ 1 }",
                TextObject::InsideClass
            ),
            (
                "impl Hello {\n    fn say() -> u8 { return /* hi */ 1 }\n}",
                TextObject::AroundClass
            ),
        ],
    )
}

#[gpui::test]
fn test_text_objects_with_has_parent_predicate(cx: &mut App) {
    use std::borrow::Cow;

    // Create a language with a custom text_objects query that uses #has-parent?
    // This query only matches closure_expression when it's inside a call_expression
    let language = Language::new(
        LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    )
    .with_queries(LanguageQueries {
        text_objects: Some(Cow::from(indoc! {r#"
            ; Only match closures that are arguments to function calls
            (closure_expression) @function.around
              (#has-parent? @function.around arguments)
        "#})),
        ..Default::default()
    })
    .expect("Could not parse queries");

    let (text, ranges) = marked_text_ranges(
        indoc! {r#"
            fn main() {
                let standalone = |x| x + 1;
                let result = foo(|y| y * ˇ2);
            }"#
        },
        false,
    );

    let buffer = cx.new(|cx| Buffer::local(text.clone(), cx).with_language(Arc::new(language), cx));
    let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());

    let matches = snapshot
        .text_object_ranges(ranges[0].clone(), TreeSitterOptions::default())
        .map(|(range, text_object)| (&text[range], text_object))
        .collect::<Vec<_>>();

    // Should only match the closure inside foo(), not the standalone closure
    assert_eq!(matches, &[("|y| y * 2", TextObject::AroundFunction),]);
}

#[gpui::test]
fn test_text_objects_with_not_has_parent_predicate(cx: &mut App) {
    use std::borrow::Cow;

    // Create a language with a custom text_objects query that uses #not-has-parent?
    // This query only matches closure_expression when it's NOT inside a call_expression
    let language = Language::new(
        LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    )
    .with_queries(LanguageQueries {
        text_objects: Some(Cow::from(indoc! {r#"
            ; Only match closures that are NOT arguments to function calls
            (closure_expression) @function.around
              (#not-has-parent? @function.around arguments)
        "#})),
        ..Default::default()
    })
    .expect("Could not parse queries");

    let (text, ranges) = marked_text_ranges(
        indoc! {r#"
            fn main() {
                let standalone = |x| x +ˇ 1;
                let result = foo(|y| y * 2);
            }"#
        },
        false,
    );

    let buffer = cx.new(|cx| Buffer::local(text.clone(), cx).with_language(Arc::new(language), cx));
    let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());

    let matches = snapshot
        .text_object_ranges(ranges[0].clone(), TreeSitterOptions::default())
        .map(|(range, text_object)| (&text[range], text_object))
        .collect::<Vec<_>>();

    // Should only match the standalone closure, not the one inside foo()
    assert_eq!(matches, &[("|x| x + 1", TextObject::AroundFunction),]);
}

#[gpui::test]
fn test_enclosing_bracket_ranges(cx: &mut App) {
    #[track_caller]
    fn assert(selection_text: &'static str, range_markers: Vec<&'static str>, cx: &mut App) {
        assert_bracket_pairs(selection_text, range_markers, rust_lang(), cx)
    }

    assert(
        indoc! {"
            mod x {
                moˇd y {

                }
            }
            let foo = 1;"},
        vec![indoc! {"
            mod x «{»
                mod y {

                }
            «}»
            let foo = 1;"}],
        cx,
    );

    assert(
        indoc! {"
            mod x {
                mod y ˇ{

                }
            }
            let foo = 1;"},
        vec![
            indoc! {"
                mod x «{»
                    mod y {

                    }
                «}»
                let foo = 1;"},
            indoc! {"
                mod x {
                    mod y «{»

                    «}»
                }
                let foo = 1;"},
        ],
        cx,
    );

    assert(
        indoc! {"
            mod x {
                mod y {

                }ˇ
            }
            let foo = 1;"},
        vec![
            indoc! {"
                mod x «{»
                    mod y {

                    }
                «}»
                let foo = 1;"},
            indoc! {"
                mod x {
                    mod y «{»

                    «}»
                }
                let foo = 1;"},
        ],
        cx,
    );

    assert(
        indoc! {"
            mod x {
                mod y {

                }
            ˇ}
            let foo = 1;"},
        vec![indoc! {"
            mod x «{»
                mod y {

                }
            «}»
            let foo = 1;"}],
        cx,
    );

    assert(
        indoc! {"
            mod x {
                mod y {

                }
            }
            let fˇoo = 1;"},
        Vec::new(),
        cx,
    );

    // Regression test: avoid crash when querying at the end of the buffer.
    assert(
        indoc! {"
            mod x {
                mod y {

                }
            }
            let foo = 1;ˇ"},
        Vec::new(),
        cx,
    );
}

#[gpui::test]
fn test_enclosing_bracket_ranges_where_brackets_are_not_outermost_children(cx: &mut App) {
    let mut assert = |selection_text, bracket_pair_texts| {
        assert_bracket_pairs(
            selection_text,
            bracket_pair_texts,
            Arc::new(javascript_lang()),
            cx,
        )
    };

    assert(
        indoc! {"
        for (const a in b)ˇ {
            // a comment that's longer than the for-loop header
        }"},
        vec![indoc! {"
        for «(»const a in b«)» {
            // a comment that's longer than the for-loop header
        }"}],
    );

    // Regression test: even though the parent node of the parentheses (the for loop) does
    // intersect the given range, the parentheses themselves do not contain the range, so
    // they should not be returned. Only the curly braces contain the range.
    assert(
        indoc! {"
        for (const a in b) {ˇ
            // a comment that's longer than the for-loop header
        }"},
        vec![indoc! {"
        for (const a in b) «{»
            // a comment that's longer than the for-loop header
        «}»"}],
    );
}

#[gpui::test]
fn test_range_for_syntax_ancestor(cx: &mut App) {
    cx.new(|cx| {
        let text = "fn a() { b(|c| {}) }";
        let buffer = Buffer::local(text, cx).with_language(rust_lang(), cx);
        let snapshot = buffer.snapshot();

        assert_eq!(
            snapshot
                .syntax_ancestor(empty_range_at(text, "|"))
                .unwrap()
                .byte_range(),
            range_of(text, "|")
        );
        assert_eq!(
            snapshot
                .syntax_ancestor(range_of(text, "|"))
                .unwrap()
                .byte_range(),
            range_of(text, "|c|")
        );
        assert_eq!(
            snapshot
                .syntax_ancestor(range_of(text, "|c|"))
                .unwrap()
                .byte_range(),
            range_of(text, "|c| {}")
        );
        assert_eq!(
            snapshot
                .syntax_ancestor(range_of(text, "|c| {}"))
                .unwrap()
                .byte_range(),
            range_of(text, "(|c| {})")
        );

        buffer
    });

    fn empty_range_at(text: &str, part: &str) -> Range<usize> {
        let start = text.find(part).unwrap();
        start..start
    }

    fn range_of(text: &str, part: &str) -> Range<usize> {
        let start = text.find(part).unwrap();
        start..start + part.len()
    }
}
