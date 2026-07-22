use super::*;

#[gpui::test]
async fn test_buffer_diagnostics(cx: &mut TestAppContext) {
    init_test(cx);

    // We'll be creating two different files, both with diagnostics, so we can
    // later verify that, since the `BufferDiagnosticsEditor` only shows
    // diagnostics for the provided path, the diagnostics for the other file
    // will not be shown, contrary to what happens with
    // `ProjectDiagnosticsEditor`.
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/test"),
        json!({
            "main.rs": "
                fn main() {
                    let x = vec![];
                    let y = vec![];
                    a(x);
                    b(y);
                    c(y);
                    d(x);
                }
            "
            .unindent(),
            "other.rs": "
                fn other() {
                    let unused = 42;
                    undefined_function();
                }
            "
            .unindent(),
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let project_path = project::ProjectPath {
        worktree_id: project.read_with(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        }),
        path: rel_path("main.rs").into(),
    };
    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer(project_path.clone(), cx)
        })
        .await
        .ok();

    // Create the diagnostics for `main.rs`.
    let language_server_id = LanguageServerId(0);
    let uri = lsp::Uri::from_file_path(path!("/test/main.rs")).unwrap();
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());

    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store.update_diagnostics(language_server_id, lsp::PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics: vec![
                lsp::Diagnostic{
                    range: lsp::Range::new(lsp::Position::new(5, 6), lsp::Position::new(5, 7)),
                    severity: Some(lsp::DiagnosticSeverity::WARNING),
                    message: "use of moved value\nvalue used here after move".to_string(),
                    related_information: Some(vec![
                        lsp::DiagnosticRelatedInformation {
                            location: lsp::Location::new(uri.clone(), lsp::Range::new(lsp::Position::new(2, 8), lsp::Position::new(2, 9))),
                            message: "move occurs because `y` has type `Vec<char>`, which does not implement the `Copy` trait".to_string()
                        },
                        lsp::DiagnosticRelatedInformation {
                            location: lsp::Location::new(uri.clone(), lsp::Range::new(lsp::Position::new(4, 6), lsp::Position::new(4, 7))),
                            message: "value moved here".to_string()
                        },
                    ]),
                    ..Default::default()
                },
                lsp::Diagnostic{
                    range: lsp::Range::new(lsp::Position::new(6, 6), lsp::Position::new(6, 7)),
                    severity: Some(lsp::DiagnosticSeverity::ERROR),
                    message: "use of moved value\nvalue used here after move".to_string(),
                    related_information: Some(vec![
                        lsp::DiagnosticRelatedInformation {
                            location: lsp::Location::new(uri.clone(), lsp::Range::new(lsp::Position::new(1, 8), lsp::Position::new(1, 9))),
                            message: "move occurs because `x` has type `Vec<char>`, which does not implement the `Copy` trait".to_string()
                        },
                        lsp::DiagnosticRelatedInformation {
                            location: lsp::Location::new(uri.clone(), lsp::Range::new(lsp::Position::new(3, 6), lsp::Position::new(3, 7))),
                            message: "value moved here".to_string()
                        },
                    ]),
                    ..Default::default()
                }
            ],
            version: None
        }, None, DiagnosticSourceKind::Pushed, &[], cx).unwrap();

        // Create diagnostics for other.rs to ensure that the file and
        // diagnostics are not included in `BufferDiagnosticsEditor` when it is
        // deployed for main.rs.
        lsp_store.update_diagnostics(language_server_id, lsp::PublishDiagnosticsParams {
            uri: lsp::Uri::from_file_path(path!("/test/other.rs")).unwrap(),
            diagnostics: vec![
                lsp::Diagnostic{
                    range: lsp::Range::new(lsp::Position::new(1, 8), lsp::Position::new(1, 14)),
                    severity: Some(lsp::DiagnosticSeverity::WARNING),
                    message: "unused variable: `unused`".to_string(),
                    ..Default::default()
                },
                lsp::Diagnostic{
                    range: lsp::Range::new(lsp::Position::new(2, 4), lsp::Position::new(2, 22)),
                    severity: Some(lsp::DiagnosticSeverity::ERROR),
                    message: "cannot find function `undefined_function` in this scope".to_string(),
                    ..Default::default()
                }
            ],
            version: None
        }, None, DiagnosticSourceKind::Pushed, &[], cx).unwrap();
    });

    let buffer_diagnostics = window.build_entity(cx, |window, cx| {
        BufferDiagnosticsEditor::new(
            project_path.clone(),
            project.clone(),
            buffer,
            true,
            window,
            cx,
        )
    });
    let editor = buffer_diagnostics.update(cx, |buffer_diagnostics, _| {
        buffer_diagnostics.editor().clone()
    });

    // Since the excerpt updates is handled by a background task, we need to
    // wait a little bit to ensure that the buffer diagnostic's editor content
    // is rendered.
    cx.executor()
        .advance_clock(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10));

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
             § the `Copy` trait
                 a(x); § value moved here
                 b(y); § value moved here
                 c(y);
             § use of moved value
             § value used here after move
                 d(x);
             § use of moved value
             § value used here after move
             § hint: move occurs because `x` has type `Vec<char>`, which does not
             § implement the `Copy` trait
             }"
        }
    );
}

#[gpui::test]
async fn test_buffer_diagnostics_without_warnings(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/test"),
        json!({
            "main.rs": "
                fn main() {
                    let x = vec![];
                    let y = vec![];
                    a(x);
                    b(y);
                    c(y);
                    d(x);
                }
            "
            .unindent(),
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let project_path = project::ProjectPath {
        worktree_id: project.read_with(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        }),
        path: rel_path("main.rs").into(),
    };
    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer(project_path.clone(), cx)
        })
        .await
        .ok();

    let language_server_id = LanguageServerId(0);
    let uri = lsp::Uri::from_file_path(path!("/test/main.rs")).unwrap();
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());

    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store.update_diagnostics(language_server_id, lsp::PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics: vec![
                lsp::Diagnostic{
                    range: lsp::Range::new(lsp::Position::new(5, 6), lsp::Position::new(5, 7)),
                    severity: Some(lsp::DiagnosticSeverity::WARNING),
                    message: "use of moved value\nvalue used here after move".to_string(),
                    related_information: Some(vec![
                        lsp::DiagnosticRelatedInformation {
                            location: lsp::Location::new(uri.clone(), lsp::Range::new(lsp::Position::new(2, 8), lsp::Position::new(2, 9))),
                            message: "move occurs because `y` has type `Vec<char>`, which does not implement the `Copy` trait".to_string()
                        },
                        lsp::DiagnosticRelatedInformation {
                            location: lsp::Location::new(uri.clone(), lsp::Range::new(lsp::Position::new(4, 6), lsp::Position::new(4, 7))),
                            message: "value moved here".to_string()
                        },
                    ]),
                    ..Default::default()
                },
                lsp::Diagnostic{
                    range: lsp::Range::new(lsp::Position::new(6, 6), lsp::Position::new(6, 7)),
                    severity: Some(lsp::DiagnosticSeverity::ERROR),
                    message: "use of moved value\nvalue used here after move".to_string(),
                    related_information: Some(vec![
                        lsp::DiagnosticRelatedInformation {
                            location: lsp::Location::new(uri.clone(), lsp::Range::new(lsp::Position::new(1, 8), lsp::Position::new(1, 9))),
                            message: "move occurs because `x` has type `Vec<char>`, which does not implement the `Copy` trait".to_string()
                        },
                        lsp::DiagnosticRelatedInformation {
                            location: lsp::Location::new(uri.clone(), lsp::Range::new(lsp::Position::new(3, 6), lsp::Position::new(3, 7))),
                            message: "value moved here".to_string()
                        },
                    ]),
                    ..Default::default()
                }
            ],
            version: None
        }, None, DiagnosticSourceKind::Pushed, &[], cx).unwrap();
    });

    let include_warnings = false;
    let buffer_diagnostics = window.build_entity(cx, |window, cx| {
        BufferDiagnosticsEditor::new(
            project_path.clone(),
            project.clone(),
            buffer,
            include_warnings,
            window,
            cx,
        )
    });

    let editor = buffer_diagnostics.update(cx, |buffer_diagnostics, _cx| {
        buffer_diagnostics.editor().clone()
    });

    // Since the excerpt updates is handled by a background task, we need to
    // wait a little bit to ensure that the buffer diagnostic's editor content
    // is rendered.
    cx.executor()
        .advance_clock(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10));

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
                 a(x); § value moved here
                 b(y);
                 c(y);
                 d(x);
             § use of moved value
             § value used here after move
             § hint: move occurs because `x` has type `Vec<char>`, which does not
             § implement the `Copy` trait
             }"
        }
    );
}

#[gpui::test]
async fn test_buffer_diagnostics_multiple_servers(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/test"),
        json!({
            "main.rs": "
                fn main() {
                    let x = vec![];
                    let y = vec![];
                    a(x);
                    b(y);
                    c(y);
                    d(x);
                }
            "
            .unindent(),
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let project_path = project::ProjectPath {
        worktree_id: project.read_with(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        }),
        path: rel_path("main.rs").into(),
    };
    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer(project_path.clone(), cx)
        })
        .await
        .ok();

    // Create the diagnostics for `main.rs`.
    // Two warnings are being created, one for each language server, in order to
    // assert that both warnings are rendered in the editor.
    let language_server_id_a = LanguageServerId(0);
    let language_server_id_b = LanguageServerId(1);
    let uri = lsp::Uri::from_file_path(path!("/test/main.rs")).unwrap();
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());

    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store
            .update_diagnostics(
                language_server_id_a,
                lsp::PublishDiagnosticsParams {
                    uri: uri.clone(),
                    diagnostics: vec![lsp::Diagnostic {
                        range: lsp::Range::new(lsp::Position::new(5, 6), lsp::Position::new(5, 7)),
                        severity: Some(lsp::DiagnosticSeverity::WARNING),
                        message: "use of moved value\nvalue used here after move".to_string(),
                        related_information: None,
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

        lsp_store
            .update_diagnostics(
                language_server_id_b,
                lsp::PublishDiagnosticsParams {
                    uri: uri.clone(),
                    diagnostics: vec![lsp::Diagnostic {
                        range: lsp::Range::new(lsp::Position::new(6, 6), lsp::Position::new(6, 7)),
                        severity: Some(lsp::DiagnosticSeverity::WARNING),
                        message: "use of moved value\nvalue used here after move".to_string(),
                        related_information: None,
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

    let buffer_diagnostics = window.build_entity(cx, |window, cx| {
        BufferDiagnosticsEditor::new(
            project_path.clone(),
            project.clone(),
            buffer,
            true,
            window,
            cx,
        )
    });
    let editor = buffer_diagnostics.update(cx, |buffer_diagnostics, _| {
        buffer_diagnostics.editor().clone()
    });

    // Since the excerpt updates is handled by a background task, we need to
    // wait a little bit to ensure that the buffer diagnostic's editor content
    // is rendered.
    cx.executor()
        .advance_clock(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10));

    pretty_assertions::assert_eq!(
        editor_content_with_blocks(&editor, cx),
        indoc::indoc! {
            "§ main.rs
             § -----
                 a(x);
                 b(y);
                 c(y);
             § use of moved value
             § value used here after move
                 d(x);
             § use of moved value
             § value used here after move
             }"
        }
    );

    buffer_diagnostics.update(cx, |buffer_diagnostics, _cx| {
        assert_eq!(
            *buffer_diagnostics.summary(),
            DiagnosticSummary {
                warning_count: 2,
                error_count: 0
            }
        );
    })
}
