use super::*;

#[gpui::test]
async fn test_extra_newline_insertion(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(
        Language::new(
            LanguageConfig {
                brackets: BracketPairConfig {
                    pairs: vec![
                        BracketPair {
                            start: "{".to_string(),
                            end: "}".to_string(),
                            close: true,
                            surround: true,
                            newline: true,
                        },
                        BracketPair {
                            start: "/* ".to_string(),
                            end: " */".to_string(),
                            close: true,
                            surround: true,
                            newline: true,
                        },
                    ],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_indents_query("")
        .unwrap(),
    );

    let text = concat!(
        "{   }\n",     //
        "  x\n",       //
        "  /*   */\n", //
        "x\n",         //
        "{{} }\n",     //
    );

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));
    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 3),
                DisplayPoint::new(DisplayRow(2), 5)..DisplayPoint::new(DisplayRow(2), 5),
                DisplayPoint::new(DisplayRow(4), 4)..DisplayPoint::new(DisplayRow(4), 4),
            ])
        });
        editor.newline(&Newline, window, cx);

        assert_eq!(
            editor.buffer().read(cx).read(cx).text(),
            concat!(
                "{ \n",    // Suppress rustfmt
                "\n",      //
                "}\n",     //
                "  x\n",   //
                "  /* \n", //
                "  \n",    //
                "  */\n",  //
                "x\n",     //
                "{{} \n",  //
                "}\n",     //
            )
        );
    });
}

#[gpui::test]
fn test_highlighted_ranges(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(&sample_text(16, 8, 'a'), cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        let buffer = editor.buffer.read(cx).snapshot(cx);

        let anchor_range =
            |range: Range<Point>| buffer.anchor_after(range.start)..buffer.anchor_after(range.end);

        editor.highlight_background(
            HighlightKey::ColorizeBracket(0),
            &[
                anchor_range(Point::new(2, 1)..Point::new(2, 3)),
                anchor_range(Point::new(4, 2)..Point::new(4, 4)),
                anchor_range(Point::new(6, 3)..Point::new(6, 5)),
                anchor_range(Point::new(8, 4)..Point::new(8, 6)),
            ],
            |_, _| Hsla::red(),
            cx,
        );
        editor.highlight_background(
            HighlightKey::ColorizeBracket(1),
            &[
                anchor_range(Point::new(3, 2)..Point::new(3, 5)),
                anchor_range(Point::new(5, 3)..Point::new(5, 6)),
                anchor_range(Point::new(7, 4)..Point::new(7, 7)),
                anchor_range(Point::new(9, 5)..Point::new(9, 8)),
            ],
            |_, _| Hsla::green(),
            cx,
        );

        let snapshot = editor.snapshot(window, cx);
        let highlighted_ranges = editor.sorted_background_highlights_in_range(
            anchor_range(Point::new(3, 4)..Point::new(7, 4)),
            &snapshot,
            cx.theme(),
        );
        assert_eq!(
            highlighted_ranges,
            &[
                (
                    DisplayPoint::new(DisplayRow(3), 2)..DisplayPoint::new(DisplayRow(3), 5),
                    Hsla::green(),
                ),
                (
                    DisplayPoint::new(DisplayRow(4), 2)..DisplayPoint::new(DisplayRow(4), 4),
                    Hsla::red(),
                ),
                (
                    DisplayPoint::new(DisplayRow(5), 3)..DisplayPoint::new(DisplayRow(5), 6),
                    Hsla::green(),
                ),
                (
                    DisplayPoint::new(DisplayRow(6), 3)..DisplayPoint::new(DisplayRow(6), 5),
                    Hsla::red(),
                ),
            ]
        );
        assert_eq!(
            editor.sorted_background_highlights_in_range(
                anchor_range(Point::new(5, 6)..Point::new(6, 4)),
                &snapshot,
                cx.theme(),
            ),
            &[(
                DisplayPoint::new(DisplayRow(6), 3)..DisplayPoint::new(DisplayRow(6), 5),
                Hsla::red(),
            )]
        );
    });
}

#[gpui::test]
async fn test_copy_highlight_json(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(indoc! {"
        fn main() {
            let x = 1;ˇ
        }
    "});
    setup_syntax_highlighting(rust_lang(), &mut cx);

    cx.update_editor(|editor, window, cx| {
        editor.copy_highlight_json(&CopyHighlightJson, window, cx);
    });

    let clipboard_json: serde_json::Value =
        serde_json::from_str(&cx.read_from_clipboard().unwrap().text().unwrap()).unwrap();
    assert_eq!(
        clipboard_json,
        json!([
            [
                {"text": "fn", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "main", "highlight": "function"},
                {"text": "()", "highlight": "punctuation.bracket"},
                {"text": " ", "highlight": null},
                {"text": "{", "highlight": "punctuation.bracket"},
            ],
            [
                {"text": "    ", "highlight": null},
                {"text": "let", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "x", "highlight": "variable"},
                {"text": " ", "highlight": null},
                {"text": "=", "highlight": "operator"},
                {"text": " ", "highlight": null},
                {"text": "1", "highlight": "number"},
                {"text": ";", "highlight": "punctuation.delimiter"},
            ],
            [
                {"text": "}", "highlight": "punctuation.bracket"},
            ],
        ])
    );
}

#[gpui::test]
async fn test_copy_highlight_json_selected_range(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(indoc! {"
        fn main() {
            «let x = 1;
            let yˇ» = 2;
        }
    "});
    setup_syntax_highlighting(rust_lang(), &mut cx);

    cx.update_editor(|editor, window, cx| {
        editor.copy_highlight_json(&CopyHighlightJson, window, cx);
    });

    let clipboard_json: serde_json::Value =
        serde_json::from_str(&cx.read_from_clipboard().unwrap().text().unwrap()).unwrap();
    assert_eq!(
        clipboard_json,
        json!([
            [
                {"text": "let", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "x", "highlight": "variable"},
                {"text": " ", "highlight": null},
                {"text": "=", "highlight": "operator"},
                {"text": " ", "highlight": null},
                {"text": "1", "highlight": "number"},
                {"text": ";", "highlight": "punctuation.delimiter"},
            ],
            [
                {"text": "    ", "highlight": null},
                {"text": "let", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "y", "highlight": "variable"},
            ],
        ])
    );
}

#[gpui::test]
async fn test_copy_highlight_json_selected_line_range(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        fn main() {
            «let x = 1;
            let yˇ» = 2;
        }
    "});
    setup_syntax_highlighting(rust_lang(), &mut cx);

    cx.update_editor(|editor, window, cx| {
        editor.selections.set_line_mode(true);
        editor.copy_highlight_json(&CopyHighlightJson, window, cx);
    });

    let clipboard_json: serde_json::Value =
        serde_json::from_str(&cx.read_from_clipboard().unwrap().text().unwrap()).unwrap();
    assert_eq!(
        clipboard_json,
        json!([
            [
                {"text": "    ", "highlight": null},
                {"text": "let", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "x", "highlight": "variable"},
                {"text": " ", "highlight": null},
                {"text": "=", "highlight": "operator"},
                {"text": " ", "highlight": null},
                {"text": "1", "highlight": "number"},
                {"text": ";", "highlight": "punctuation.delimiter"},
            ],
            [
                {"text": "    ", "highlight": null},
                {"text": "let", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "y", "highlight": "variable"},
                {"text": " ", "highlight": null},
                {"text": "=", "highlight": "operator"},
                {"text": " ", "highlight": null},
                {"text": "2", "highlight": "number"},
                {"text": ";", "highlight": "punctuation.delimiter"},
            ],
        ])
    );
}

#[gpui::test]
async fn test_copy_highlight_json_single_line(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        fn main() {
            let ˇx = 1;
            let y = 2;
        }
    "});
    setup_syntax_highlighting(rust_lang(), &mut cx);

    cx.update_editor(|editor, window, cx| {
        editor.selections.set_line_mode(true);
        editor.copy_highlight_json(&CopyHighlightJson, window, cx);
    });

    let clipboard_json: serde_json::Value =
        serde_json::from_str(&cx.read_from_clipboard().unwrap().text().unwrap()).unwrap();
    assert_eq!(
        clipboard_json,
        json!([
            [
                {"text": "    ", "highlight": null},
                {"text": "let", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "x", "highlight": "variable"},
                {"text": " ", "highlight": null},
                {"text": "=", "highlight": "operator"},
                {"text": " ", "highlight": null},
                {"text": "1", "highlight": "number"},
                {"text": ";", "highlight": "punctuation.delimiter"},
            ]
        ])
    );
}
