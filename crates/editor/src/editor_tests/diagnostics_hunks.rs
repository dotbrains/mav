use super::*;

#[gpui::test]
async fn go_to_prev_overlapping_diagnostic(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let lsp_store =
        cx.update_editor(|editor, _, cx| editor.project().unwrap().read(cx).lsp_store());

    cx.set_state(indoc! {"
        ˇfn func(abc def: i32) -> u32 {
        }
    "});

    cx.update(|_, cx| {
        lsp_store.update(cx, |lsp_store, cx| {
            lsp_store
                .update_diagnostics(
                    LanguageServerId(0),
                    lsp::PublishDiagnosticsParams {
                        uri: lsp::Uri::from_file_path(path!("/root/file")).unwrap(),
                        version: None,
                        diagnostics: vec![
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 11),
                                    lsp::Position::new(0, 12),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 12),
                                    lsp::Position::new(0, 15),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 25),
                                    lsp::Position::new(0, 28),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                        ],
                    },
                    None,
                    DiagnosticSourceKind::Pushed,
                    &[],
                    cx,
                )
                .unwrap()
        });
    });

    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_diagnostic(&GoToPreviousDiagnostic::default(), window, cx);
    });

    cx.assert_editor_state(indoc! {"
        fn func(abc def: i32) -> ˇu32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_diagnostic(&GoToPreviousDiagnostic::default(), window, cx);
    });

    cx.assert_editor_state(indoc! {"
        fn func(abc ˇdef: i32) -> u32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_diagnostic(&GoToPreviousDiagnostic::default(), window, cx);
    });

    cx.assert_editor_state(indoc! {"
        fn func(abcˇ def: i32) -> u32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_diagnostic(&GoToPreviousDiagnostic::default(), window, cx);
    });

    cx.assert_editor_state(indoc! {"
        fn func(abc def: i32) -> ˇu32 {
        }
    "});
}

#[gpui::test]
async fn go_to_diagnostic(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let lsp_store =
        cx.update_editor(|editor, _, cx| editor.project().unwrap().read(cx).lsp_store());

    // Place the cursor inside the `def` diagnostic (`[12, 15)`) before any
    // diagnostic is active so we can later confirm that running `editor: go to
    // diagnostic` will activate this diagnostic instead of advancing to the
    // next one.
    cx.set_state(indoc! {"
        fn func(abc dˇef: i32) -> u32 {
        }
    "});

    // Set up the diagnostics:
    //
    // * `[11, 12)` (the space before `def`),
    // * `[12, 15)` (`def`),
    // * `[25, 28)` (`u32`).
    cx.update(|_, cx| {
        lsp_store.update(cx, |lsp_store, cx| {
            lsp_store
                .update_diagnostics(
                    LanguageServerId(0),
                    lsp::PublishDiagnosticsParams {
                        uri: lsp::Uri::from_file_path(path!("/root/file")).unwrap(),
                        version: None,
                        diagnostics: vec![
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 11),
                                    lsp::Position::new(0, 12),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 12),
                                    lsp::Position::new(0, 15),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 25),
                                    lsp::Position::new(0, 28),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                        ],
                    },
                    None,
                    DiagnosticSourceKind::Pushed,
                    &[],
                    cx,
                )
                .unwrap()
        });
    });

    executor.run_until_parked();

    // When the cursor is at an inactive diagnostic, cursor should be moved to
    // the start of that same diagnostic and activate it.
    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abc ˇdef: i32) -> u32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abc def: i32) -> ˇu32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abcˇ def: i32) -> u32 {
        }
    "});

    // Manually move the cursor to a different, not yet active diagnostic to
    // confirm that using `editor: go to diagnostic` will now activate this one.
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([Point::new(0, 26)..Point::new(0, 26)])
        });
    });

    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abc def: i32) -> ˇu32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(0, 0)])
        });
    });
    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abcˇ def: i32) -> u32 {
        }
    "});
}

#[gpui::test]
async fn test_go_to_hunk(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        use some::mod;

        const A: u32 = 42;

        fn main() {
            println!("hello");

            println!("world");
        }
        "#
    .unindent();

    // Edits are modified, removed, modified, added
    cx.set_state(
        &r#"
        use some::modified;

        ˇ
        fn main() {
            println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.set_head_text(&diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        //Wrap around the bottom of the buffer
        for _ in 0..3 {
            editor.go_to_next_hunk(&GoToHunk, window, cx);
        }
    });

    cx.assert_editor_state(
        &r#"
        ˇuse some::modified;


        fn main() {
            println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        //Wrap around the top of the buffer
        for _ in 0..2 {
            editor.go_to_prev_hunk(&GoToPreviousHunk, window, cx);
        }
    });

    cx.assert_editor_state(
        &r#"
        use some::modified;


        fn main() {
        ˇ    println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_hunk(&GoToPreviousHunk, window, cx);
    });

    cx.assert_editor_state(
        &r#"
        use some::modified;

        ˇ
        fn main() {
            println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_hunk(&GoToPreviousHunk, window, cx);
    });

    cx.assert_editor_state(
        &r#"
        ˇuse some::modified;


        fn main() {
            println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        for _ in 0..2 {
            editor.go_to_prev_hunk(&GoToPreviousHunk, window, cx);
        }
    });

    cx.assert_editor_state(
        &r#"
        use some::modified;


        fn main() {
        ˇ    println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.fold(&Fold, window, cx);
    });

    cx.update_editor(|editor, window, cx| {
        editor.go_to_next_hunk(&GoToHunk, window, cx);
    });

    cx.assert_editor_state(
        &r#"
        ˇuse some::modified;


        fn main() {
            println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );
}
