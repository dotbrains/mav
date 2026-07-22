use super::*;

#[gpui::test]
async fn test_diagnostics(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/test"),
        json!({
            "consts.rs": "
                const a: i32 = 'a';
                const b: i32 = c;
            "
            .unindent(),

            "main.rs": "
                fn main() {
                    let x = vec![];
                    let y = vec![];
                    a(x);
                    b(y);
                    // comment 1
                    // comment 2
                    c(y);
                    d(x);
                }
            "
            .unindent(),
        }),
    )
    .await;

    let language_server_id = LanguageServerId(0);
    let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let uri = lsp::Uri::from_file_path(path!("/test/main.rs")).unwrap();

    // Create some diagnostics
    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store.update_diagnostics(language_server_id, lsp::PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics: vec![lsp::Diagnostic{
                range: lsp::Range::new(lsp::Position::new(7, 6),lsp::Position::new(7, 7)),
                severity:Some(lsp::DiagnosticSeverity::ERROR),
                message: "use of moved value\nvalue used here after move".to_string(),
                related_information: Some(vec![lsp::DiagnosticRelatedInformation {
                    location: lsp::Location::new(uri.clone(), lsp::Range::new(lsp::Position::new(2,8),lsp::Position::new(2,9))),
                    message: "move occurs because `y` has type `Vec<char>`, which does not implement the `Copy` trait".to_string()
                },
                lsp::DiagnosticRelatedInformation {
                    location: lsp::Location::new(uri.clone(), lsp::Range::new(lsp::Position::new(4,6),lsp::Position::new(4,7))),
                    message: "value moved here".to_string()
                },
                ]),
                ..Default::default()
            },
            lsp::Diagnostic{
                range: lsp::Range::new(lsp::Position::new(8, 6),lsp::Position::new(8, 7)),
                severity:Some(lsp::DiagnosticSeverity::ERROR),
                message: "use of moved value\nvalue used here after move".to_string(),
                related_information: Some(vec![lsp::DiagnosticRelatedInformation {
                    location: lsp::Location::new(uri.clone(), lsp::Range::new(lsp::Position::new(1,8),lsp::Position::new(1,9))),
                    message: "move occurs because `x` has type `Vec<char>`, which does not implement the `Copy` trait".to_string()
                },
                lsp::DiagnosticRelatedInformation {
                    location: lsp::Location::new(uri.clone(), lsp::Range::new(lsp::Position::new(3,6),lsp::Position::new(3,7))),
                    message: "value moved here".to_string()
                },
                ]),
                ..Default::default()
            }
            ],
            version: None
        }, None, DiagnosticSourceKind::Pushed, &[], cx).unwrap();
    });

    // Open the project diagnostics view while there are already diagnostics.
    let diagnostics = window.build_entity(cx, |window, cx| {
        ProjectDiagnosticsEditor::new(true, project.clone(), workspace.downgrade(), window, cx)
    });
    let editor = diagnostics.update(cx, |diagnostics, _| diagnostics.editor.clone());

    diagnostics
        .next_notification(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10), cx)
        .await;

    pretty_assertions::assert_eq!(
        editor_content_with_blocks(&editor, cx),
        indoc::indoc! {
            "§ main.rs
             § -----
             fn main() {
                 let x = vec![];
             § move occurs because `x` has type `Vec<char>`, which does not implement
             § the `Copy` trait (back)
                 let y = vec![];
             § move occurs because `y` has type `Vec<char>`, which does not implement
             § the `Copy` trait (back)
                 a(x); § value moved here (back)
                 b(y); § value moved here
                 // comment 1
                 // comment 2
                 c(y);
             § use of moved value
             § value used here after move
             § hint: move occurs because `y` has type `Vec<char>`, which does not
             § implement the `Copy` trait
                 d(x);
             § use of moved value
             § value used here after move
             § hint: move occurs because `x` has type `Vec<char>`, which does not
             § implement the `Copy` trait
             § hint: value moved here
             }"
        }
    );

    // Cursor is at the first diagnostic
    editor.update(cx, |editor, cx| {
        assert_eq!(
            editor
                .selections
                .display_ranges(&editor.display_snapshot(cx)),
            [DisplayPoint::new(DisplayRow(3), 8)..DisplayPoint::new(DisplayRow(3), 8)]
        );
    });

    // Diagnostics are added for another earlier path.
    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store.disk_based_diagnostics_started(language_server_id, cx);
        lsp_store
            .update_diagnostics(
                language_server_id,
                lsp::PublishDiagnosticsParams {
                    uri: lsp::Uri::from_file_path(path!("/test/consts.rs")).unwrap(),
                    diagnostics: vec![lsp::Diagnostic {
                        range: lsp::Range::new(
                            lsp::Position::new(0, 15),
                            lsp::Position::new(0, 15),
                        ),
                        severity: Some(lsp::DiagnosticSeverity::ERROR),
                        message: "mismatched types expected `usize`, found `char`".to_string(),
                        ..Default::default()
                    }],
                    version: None,
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
            .unwrap();
        lsp_store.disk_based_diagnostics_finished(language_server_id, cx);
    });

    diagnostics
        .next_notification(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10), cx)
        .await;

    pretty_assertions::assert_eq!(
        editor_content_with_blocks(&editor, cx),
        indoc::indoc! {
            "§ consts.rs
             § -----
             const a: i32 = 'a'; § mismatched types expected `usize`, found `char`
             const b: i32 = c;

             § main.rs
             § -----
             fn main() {
                 let x = vec![];
             § move occurs because `x` has type `Vec<char>`, which does not implement
             § the `Copy` trait (back)
                 let y = vec![];
             § move occurs because `y` has type `Vec<char>`, which does not implement
             § the `Copy` trait (back)
                 a(x); § value moved here (back)
                 b(y); § value moved here
                 // comment 1
                 // comment 2
                 c(y);
             § use of moved value
             § value used here after move
             § hint: move occurs because `y` has type `Vec<char>`, which does not
             § implement the `Copy` trait
                 d(x);
             § use of moved value
             § value used here after move
             § hint: move occurs because `x` has type `Vec<char>`, which does not
             § implement the `Copy` trait
             § hint: value moved here
             }"
        }
    );

    // Cursor keeps its position.
    editor.update(cx, |editor, cx| {
        assert_eq!(
            editor
                .selections
                .display_ranges(&editor.display_snapshot(cx)),
            [DisplayPoint::new(DisplayRow(8), 8)..DisplayPoint::new(DisplayRow(8), 8)]
        );
    });

    // Diagnostics are added to the first path
    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store.disk_based_diagnostics_started(language_server_id, cx);
        lsp_store
            .update_diagnostics(
                language_server_id,
                lsp::PublishDiagnosticsParams {
                    uri: lsp::Uri::from_file_path(path!("/test/consts.rs")).unwrap(),
                    diagnostics: vec![
                        lsp::Diagnostic {
                            range: lsp::Range::new(
                                lsp::Position::new(0, 15),
                                lsp::Position::new(0, 15),
                            ),
                            severity: Some(lsp::DiagnosticSeverity::ERROR),
                            message: "mismatched types expected `usize`, found `char`".to_string(),
                            ..Default::default()
                        },
                        lsp::Diagnostic {
                            range: lsp::Range::new(
                                lsp::Position::new(1, 15),
                                lsp::Position::new(1, 15),
                            ),
                            severity: Some(lsp::DiagnosticSeverity::ERROR),
                            message: "unresolved name `c`".to_string(),
                            ..Default::default()
                        },
                    ],
                    version: None,
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
            .unwrap();
        lsp_store.disk_based_diagnostics_finished(language_server_id, cx);
    });

    diagnostics
        .next_notification(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10), cx)
        .await;

    pretty_assertions::assert_eq!(
        editor_content_with_blocks(&editor, cx),
        indoc::indoc! {
            "§ consts.rs
             § -----
             const a: i32 = 'a'; § mismatched types expected `usize`, found `char`
             const b: i32 = c; § unresolved name `c`

             § main.rs
             § -----
             fn main() {
                 let x = vec![];
             § move occurs because `x` has type `Vec<char>`, which does not implement
             § the `Copy` trait (back)
                 let y = vec![];
             § move occurs because `y` has type `Vec<char>`, which does not implement
             § the `Copy` trait (back)
                 a(x); § value moved here (back)
                 b(y); § value moved here
                 // comment 1
                 // comment 2
                 c(y);
             § use of moved value
             § value used here after move
             § hint: move occurs because `y` has type `Vec<char>`, which does not
             § implement the `Copy` trait
                 d(x);
             § use of moved value
             § value used here after move
             § hint: move occurs because `x` has type `Vec<char>`, which does not
             § implement the `Copy` trait
             § hint: value moved here
             }"
        }
    );
}

#[gpui::test]
async fn test_diagnostics_with_folds(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/test"),
        json!({
            "main.js": "
            function test() {
                return 1
            };

            tset();
            ".unindent()
        }),
    )
    .await;

    let server_id_1 = LanguageServerId(100);
    let server_id_2 = LanguageServerId(101);
    let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let diagnostics = window.build_entity(cx, |window, cx| {
        ProjectDiagnosticsEditor::new(true, project.clone(), workspace.downgrade(), window, cx)
    });
    let editor = diagnostics.update(cx, |diagnostics, _| diagnostics.editor.clone());

    // Two language servers start updating diagnostics
    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store.disk_based_diagnostics_started(server_id_1, cx);
        lsp_store.disk_based_diagnostics_started(server_id_2, cx);
        lsp_store
            .update_diagnostics(
                server_id_1,
                lsp::PublishDiagnosticsParams {
                    uri: lsp::Uri::from_file_path(path!("/test/main.js")).unwrap(),
                    diagnostics: vec![lsp::Diagnostic {
                        range: lsp::Range::new(lsp::Position::new(4, 0), lsp::Position::new(4, 4)),
                        severity: Some(lsp::DiagnosticSeverity::WARNING),
                        message: "no method `tset`".to_string(),
                        related_information: Some(vec![lsp::DiagnosticRelatedInformation {
                            location: lsp::Location::new(
                                lsp::Uri::from_file_path(path!("/test/main.js")).unwrap(),
                                lsp::Range::new(
                                    lsp::Position::new(0, 9),
                                    lsp::Position::new(0, 13),
                                ),
                            ),
                            message: "method `test` defined here".to_string(),
                        }]),
                        ..Default::default()
                    }],
                    version: None,
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
            .unwrap();
    });

    // The first language server finishes
    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store.disk_based_diagnostics_finished(server_id_1, cx);
    });

    // Only the first language server's diagnostics are shown.
    cx.executor()
        .advance_clock(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10));
    cx.executor().run_until_parked();
    editor.update_in(cx, |editor, window, cx| {
        editor.fold_ranges(vec![Point::new(0, 0)..Point::new(3, 0)], false, window, cx);
    });

    pretty_assertions::assert_eq!(
        editor_content_with_blocks(&editor, cx),
        indoc::indoc! {
            "§ main.js
             § -----
             ⋯
             tset(); § no method `tset`"
        }
    );

    editor.update(cx, |editor, cx| {
        editor.unfold_ranges(&[Point::new(0, 0)..Point::new(3, 0)], false, false, cx);
    });

    pretty_assertions::assert_eq!(
        editor_content_with_blocks(&editor, cx),
        indoc::indoc! {
            "§ main.js
             § -----
             function test() { § method `test` defined here
                 return 1
             };

             tset(); § no method `tset`"
        }
    );
}
