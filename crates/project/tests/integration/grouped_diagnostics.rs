use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_grouped_diagnostics(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.rs": "
                fn foo(mut v: Vec<usize>) {
                    for x in &v {
                        v.push(1);
                    }
                }
            "
            .unindent(),
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());
    let buffer = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/a.rs"), cx))
        .await
        .unwrap();

    let buffer_uri = Uri::from_file_path(path!("/dir/a.rs")).unwrap();
    let message = lsp::PublishDiagnosticsParams {
        uri: buffer_uri.clone(),
        diagnostics: vec![
            lsp::Diagnostic {
                range: lsp::Range::new(lsp::Position::new(1, 8), lsp::Position::new(1, 9)),
                severity: Some(DiagnosticSeverity::WARNING),
                message: "error 1".to_string(),
                related_information: Some(vec![lsp::DiagnosticRelatedInformation {
                    location: lsp::Location {
                        uri: buffer_uri.clone(),
                        range: lsp::Range::new(lsp::Position::new(1, 8), lsp::Position::new(1, 9)),
                    },
                    message: "error 1 hint 1".to_string(),
                }]),
                ..Default::default()
            },
            lsp::Diagnostic {
                range: lsp::Range::new(lsp::Position::new(1, 8), lsp::Position::new(1, 9)),
                severity: Some(DiagnosticSeverity::HINT),
                message: "error 1 hint 1".to_string(),
                related_information: Some(vec![lsp::DiagnosticRelatedInformation {
                    location: lsp::Location {
                        uri: buffer_uri.clone(),
                        range: lsp::Range::new(lsp::Position::new(1, 8), lsp::Position::new(1, 9)),
                    },
                    message: "original diagnostic".to_string(),
                }]),
                ..Default::default()
            },
            lsp::Diagnostic {
                range: lsp::Range::new(lsp::Position::new(2, 8), lsp::Position::new(2, 17)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: "error 2".to_string(),
                related_information: Some(vec![
                    lsp::DiagnosticRelatedInformation {
                        location: lsp::Location {
                            uri: buffer_uri.clone(),
                            range: lsp::Range::new(
                                lsp::Position::new(1, 13),
                                lsp::Position::new(1, 15),
                            ),
                        },
                        message: "error 2 hint 1".to_string(),
                    },
                    lsp::DiagnosticRelatedInformation {
                        location: lsp::Location {
                            uri: buffer_uri.clone(),
                            range: lsp::Range::new(
                                lsp::Position::new(1, 13),
                                lsp::Position::new(1, 15),
                            ),
                        },
                        message: "error 2 hint 2".to_string(),
                    },
                ]),
                ..Default::default()
            },
            lsp::Diagnostic {
                range: lsp::Range::new(lsp::Position::new(1, 13), lsp::Position::new(1, 15)),
                severity: Some(DiagnosticSeverity::HINT),
                message: "error 2 hint 1".to_string(),
                related_information: Some(vec![lsp::DiagnosticRelatedInformation {
                    location: lsp::Location {
                        uri: buffer_uri.clone(),
                        range: lsp::Range::new(lsp::Position::new(2, 8), lsp::Position::new(2, 17)),
                    },
                    message: "original diagnostic".to_string(),
                }]),
                ..Default::default()
            },
            lsp::Diagnostic {
                range: lsp::Range::new(lsp::Position::new(1, 13), lsp::Position::new(1, 15)),
                severity: Some(DiagnosticSeverity::HINT),
                message: "error 2 hint 2".to_string(),
                related_information: Some(vec![lsp::DiagnosticRelatedInformation {
                    location: lsp::Location {
                        uri: buffer_uri,
                        range: lsp::Range::new(lsp::Position::new(2, 8), lsp::Position::new(2, 17)),
                    },
                    message: "original diagnostic".to_string(),
                }]),
                ..Default::default()
            },
        ],
        version: None,
    };

    lsp_store
        .update(cx, |lsp_store, cx| {
            lsp_store.update_diagnostics(
                LanguageServerId(0),
                message,
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
        })
        .unwrap();
    let buffer = buffer.update(cx, |buffer, _| buffer.snapshot());

    assert_eq!(
        buffer
            .diagnostics_in_range::<_, Point>(0..buffer.len(), false)
            .collect::<Vec<_>>(),
        &[
            DiagnosticEntry {
                range: Point::new(1, 8)..Point::new(1, 9),
                diagnostic: Diagnostic {
                    severity: DiagnosticSeverity::WARNING,
                    message: "error 1".to_string(),
                    group_id: 1,
                    is_primary: true,
                    source_kind: DiagnosticSourceKind::Pushed,
                    ..Diagnostic::default()
                }
            },
            DiagnosticEntry {
                range: Point::new(1, 8)..Point::new(1, 9),
                diagnostic: Diagnostic {
                    severity: DiagnosticSeverity::HINT,
                    message: "error 1 hint 1".to_string(),
                    group_id: 1,
                    is_primary: false,
                    source_kind: DiagnosticSourceKind::Pushed,
                    ..Diagnostic::default()
                }
            },
            DiagnosticEntry {
                range: Point::new(1, 13)..Point::new(1, 15),
                diagnostic: Diagnostic {
                    severity: DiagnosticSeverity::HINT,
                    message: "error 2 hint 1".to_string(),
                    group_id: 0,
                    is_primary: false,
                    source_kind: DiagnosticSourceKind::Pushed,
                    ..Diagnostic::default()
                }
            },
            DiagnosticEntry {
                range: Point::new(1, 13)..Point::new(1, 15),
                diagnostic: Diagnostic {
                    severity: DiagnosticSeverity::HINT,
                    message: "error 2 hint 2".to_string(),
                    group_id: 0,
                    is_primary: false,
                    source_kind: DiagnosticSourceKind::Pushed,
                    ..Diagnostic::default()
                }
            },
            DiagnosticEntry {
                range: Point::new(2, 8)..Point::new(2, 17),
                diagnostic: Diagnostic {
                    severity: DiagnosticSeverity::ERROR,
                    message: "error 2".to_string(),
                    group_id: 0,
                    is_primary: true,
                    source_kind: DiagnosticSourceKind::Pushed,
                    ..Diagnostic::default()
                }
            }
        ]
    );

    assert_eq!(
        buffer.diagnostic_group::<Point>(0).collect::<Vec<_>>(),
        &[
            DiagnosticEntry {
                range: Point::new(1, 13)..Point::new(1, 15),
                diagnostic: Diagnostic {
                    severity: DiagnosticSeverity::HINT,
                    message: "error 2 hint 1".to_string(),
                    group_id: 0,
                    is_primary: false,
                    source_kind: DiagnosticSourceKind::Pushed,
                    ..Diagnostic::default()
                }
            },
            DiagnosticEntry {
                range: Point::new(1, 13)..Point::new(1, 15),
                diagnostic: Diagnostic {
                    severity: DiagnosticSeverity::HINT,
                    message: "error 2 hint 2".to_string(),
                    group_id: 0,
                    is_primary: false,
                    source_kind: DiagnosticSourceKind::Pushed,
                    ..Diagnostic::default()
                }
            },
            DiagnosticEntry {
                range: Point::new(2, 8)..Point::new(2, 17),
                diagnostic: Diagnostic {
                    severity: DiagnosticSeverity::ERROR,
                    message: "error 2".to_string(),
                    group_id: 0,
                    is_primary: true,
                    source_kind: DiagnosticSourceKind::Pushed,
                    ..Diagnostic::default()
                }
            }
        ]
    );

    assert_eq!(
        buffer.diagnostic_group::<Point>(1).collect::<Vec<_>>(),
        &[
            DiagnosticEntry {
                range: Point::new(1, 8)..Point::new(1, 9),
                diagnostic: Diagnostic {
                    severity: DiagnosticSeverity::WARNING,
                    message: "error 1".to_string(),
                    group_id: 1,
                    is_primary: true,
                    source_kind: DiagnosticSourceKind::Pushed,
                    ..Diagnostic::default()
                }
            },
            DiagnosticEntry {
                range: Point::new(1, 8)..Point::new(1, 9),
                diagnostic: Diagnostic {
                    severity: DiagnosticSeverity::HINT,
                    message: "error 1 hint 1".to_string(),
                    group_id: 1,
                    is_primary: false,
                    source_kind: DiagnosticSourceKind::Pushed,
                    ..Diagnostic::default()
                }
            },
        ]
    );
}
