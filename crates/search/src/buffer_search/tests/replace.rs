use super::*;

#[gpui::test]
async fn test_replace_simple(cx: &mut TestAppContext) {
    let (editor, search_bar, cx) = init_test(cx);

    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("expression", None, true, window, cx)
        })
        .await
        .unwrap();

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.replacement_editor.update(cx, |editor, cx| {
            // We use $1 here as initially we should be in Text mode, where `$1` should be treated literally.
            editor.set_text("expr$1", window, cx);
        });
        search_bar.replace_all(&ReplaceAll, window, cx)
    });
    assert_eq!(
        editor.read_with(cx, |this, cx| { this.text(cx) }),
        r#"
        A regular expr$1 (shortened as regex or regexp;[1] also referred to as
        rational expr$1[2][3]) is a sequence of characters that specifies a search
        pattern in text. Usually such patterns are used by string-searching algorithms
        for "find" or "find and replace" operations on strings, or for input validation.
        "#
        .unindent()
    );

    // Search for word boundaries and replace just a single one.
    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("or", Some(SearchOptions::WHOLE_WORD), true, window, cx)
        })
        .await
        .unwrap();

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.replacement_editor.update(cx, |editor, cx| {
            editor.set_text("banana", window, cx);
        });
        search_bar.replace_next(&ReplaceNext, window, cx)
    });
    // Notice how the first or in the text (shORtened) is not replaced. Neither are the remaining hits of `or` in the text.
    assert_eq!(
        editor.read_with(cx, |this, cx| { this.text(cx) }),
        r#"
        A regular expr$1 (shortened as regex banana regexp;[1] also referred to as
        rational expr$1[2][3]) is a sequence of characters that specifies a search
        pattern in text. Usually such patterns are used by string-searching algorithms
        for "find" or "find and replace" operations on strings, or for input validation.
        "#
        .unindent()
    );
    // Let's turn on regex mode.
    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search(
                "\\[([^\\]]+)\\]",
                Some(SearchOptions::REGEX),
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap();
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.replacement_editor.update(cx, |editor, cx| {
            editor.set_text("${1}number", window, cx);
        });
        search_bar.replace_all(&ReplaceAll, window, cx)
    });
    assert_eq!(
        editor.read_with(cx, |this, cx| { this.text(cx) }),
        r#"
        A regular expr$1 (shortened as regex banana regexp;1number also referred to as
        rational expr$12number3number) is a sequence of characters that specifies a search
        pattern in text. Usually such patterns are used by string-searching algorithms
        for "find" or "find and replace" operations on strings, or for input validation.
        "#
        .unindent()
    );
    // Now with a whole-word twist.
    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search(
                "a\\w+s",
                Some(SearchOptions::REGEX | SearchOptions::WHOLE_WORD),
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap();
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.replacement_editor.update(cx, |editor, cx| {
            editor.set_text("things", window, cx);
        });
        search_bar.replace_all(&ReplaceAll, window, cx)
    });
    // The only word affected by this edit should be `algorithms`, even though there's a bunch
    // of words in this text that would match this regex if not for WHOLE_WORD.
    assert_eq!(
        editor.read_with(cx, |this, cx| { this.text(cx) }),
        r#"
        A regular expr$1 (shortened as regex banana regexp;1number also referred to as
        rational expr$12number3number) is a sequence of characters that specifies a search
        pattern in text. Usually such patterns are used by string-searching things
        for "find" or "find and replace" operations on strings, or for input validation.
        "#
        .unindent()
    );
}

#[gpui::test]
async fn test_replace_focus(cx: &mut TestAppContext) {
    let (editor, search_bar, cx) = init_test(cx);

    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("What a bad day!", window, cx)
    });

    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("bad", None, true, window, cx)
        })
        .await
        .unwrap();

    // Calling `toggle_replace` in the search bar ensures that the "Replace
    // *" buttons are rendered, so we can then simulate clicking the
    // buttons.
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.toggle_replace(&ToggleReplace, window, cx)
    });

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.replacement_editor.update(cx, |editor, cx| {
            editor.set_text("great", window, cx);
        });
    });

    // Focus on the editor instead of the search bar, as we want to ensure
    // that pressing the "Replace Next Match" button will work, even if the
    // search bar is not focused.
    cx.focus(&editor);

    // We'll not simulate clicking the "Replace Next Match " button, asserting that
    // the replacement was done.
    let button_bounds = cx
        .debug_bounds("ICON-ReplaceNext")
        .expect("'Replace Next Match' button should be visible");
    cx.simulate_click(button_bounds.center(), gpui::Modifiers::none());

    assert_eq!(
        editor.read_with(cx, |editor, cx| editor.text(cx)),
        "What a great day!"
    );
}

struct ReplacementTestParams<'a> {
    editor: &'a Entity<Editor>,
    search_bar: &'a Entity<BufferSearchBar>,
    cx: &'a mut VisualTestContext,
    search_text: &'static str,
    search_options: Option<SearchOptions>,
    replacement_text: &'static str,
    replace_all: bool,
    expected_text: String,
}

async fn run_replacement_test(options: ReplacementTestParams<'_>) {
    options
        .search_bar
        .update_in(options.cx, |search_bar, window, cx| {
            if let Some(options) = options.search_options {
                search_bar.set_search_options(options, cx);
            }
            search_bar.search(
                options.search_text,
                options.search_options,
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap();

    options
        .search_bar
        .update_in(options.cx, |search_bar, window, cx| {
            search_bar.replacement_editor.update(cx, |editor, cx| {
                editor.set_text(options.replacement_text, window, cx);
            });

            if options.replace_all {
                search_bar.replace_all(&ReplaceAll, window, cx)
            } else {
                search_bar.replace_next(&ReplaceNext, window, cx)
            }
        });

    assert_eq!(
        options
            .editor
            .read_with(options.cx, |this, cx| { this.text(cx) }),
        options.expected_text
    );
}

#[perf]
#[gpui::test]
async fn test_replace_special_characters(cx: &mut TestAppContext) {
    let (editor, search_bar, cx) = init_test(cx);

    run_replacement_test(ReplacementTestParams {
        editor: &editor,
        search_bar: &search_bar,
        cx,
        search_text: "expression",
        search_options: None,
        replacement_text: r"\n",
        replace_all: true,
        expected_text: r#"
            A regular \n (shortened as regex or regexp;[1] also referred to as
            rational \n[2][3]) is a sequence of characters that specifies a search
            pattern in text. Usually such patterns are used by string-searching algorithms
            for "find" or "find and replace" operations on strings, or for input validation.
            "#
        .unindent(),
    })
    .await;

    run_replacement_test(ReplacementTestParams {
        editor: &editor,
        search_bar: &search_bar,
        cx,
        search_text: "or",
        search_options: Some(SearchOptions::WHOLE_WORD | SearchOptions::REGEX),
        replacement_text: r"\\\n\\\\",
        replace_all: false,
        expected_text: r#"
            A regular \n (shortened as regex \
            \\ regexp;[1] also referred to as
            rational \n[2][3]) is a sequence of characters that specifies a search
            pattern in text. Usually such patterns are used by string-searching algorithms
            for "find" or "find and replace" operations on strings, or for input validation.
            "#
        .unindent(),
    })
    .await;

    run_replacement_test(ReplacementTestParams {
        editor: &editor,
        search_bar: &search_bar,
        cx,
        search_text: r"(that|used) ",
        search_options: Some(SearchOptions::REGEX),
        replacement_text: r"$1\n",
        replace_all: true,
        expected_text: r#"
            A regular \n (shortened as regex \
            \\ regexp;[1] also referred to as
            rational \n[2][3]) is a sequence of characters that
            specifies a search
            pattern in text. Usually such patterns are used
            by string-searching algorithms
            for "find" or "find and replace" operations on strings, or for input validation.
            "#
        .unindent(),
    })
    .await;
}

#[gpui::test]
async fn test_deploy_replace_focuses_replacement_editor(cx: &mut TestAppContext) {
    init_globals(cx);
    let (editor, search_bar, cx) = init_test(cx);

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 8)..DisplayPoint::new(DisplayRow(0), 16)
            ])
        });
    });

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.deploy(
            &Deploy {
                focus: true,
                replace_enabled: true,
                selection_search_enabled: false,
            },
            None,
            window,
            cx,
        );
    });
    cx.run_until_parked();

    search_bar.update_in(cx, |search_bar, window, cx| {
        assert!(
            search_bar
                .replacement_editor
                .focus_handle(cx)
                .is_focused(window),
            "replacement editor should be focused when deploying replace with a selection",
        );
        assert!(
            !search_bar.query_editor.focus_handle(cx).is_focused(window),
            "search editor should not be focused when replacement editor is focused",
        );
    });
}
