use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_empty_diagnostic_ranges(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let text = concat!(
        "let one = ;\n", //
        "let two = \n",
        "let three = 3;\n",
    );

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "a.rs": text })).await;

    let project = Project::test(fs, [Path::new(path!("/dir"))], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();

    project.update(cx, |project, cx| {
        project.lsp_store().update(cx, |lsp_store, cx| {
            lsp_store
                .update_diagnostic_entries(
                    LanguageServerId(0),
                    PathBuf::from(path!("/dir/a.rs")),
                    None,
                    None,
                    vec![
                        DiagnosticEntry {
                            range: Unclipped(PointUtf16::new(0, 10))
                                ..Unclipped(PointUtf16::new(0, 10)),
                            diagnostic: Diagnostic {
                                severity: DiagnosticSeverity::ERROR,
                                message: "syntax error 1".to_string(),
                                source_kind: DiagnosticSourceKind::Pushed,
                                ..Diagnostic::default()
                            },
                        },
                        DiagnosticEntry {
                            range: Unclipped(PointUtf16::new(1, 10))
                                ..Unclipped(PointUtf16::new(1, 10)),
                            diagnostic: Diagnostic {
                                severity: DiagnosticSeverity::ERROR,
                                message: "syntax error 2".to_string(),
                                source_kind: DiagnosticSourceKind::Pushed,
                                ..Diagnostic::default()
                            },
                        },
                    ],
                    cx,
                )
                .unwrap();
        })
    });

    // An empty range is extended forward to include the following character.
    // At the end of a line, an empty range is extended backward to include
    // the preceding character.
    buffer.update(cx, |buffer, _| {
        let chunks = chunks_with_diagnostics(buffer, 0..buffer.len());
        assert_eq!(
            chunks
                .iter()
                .map(|(s, d)| (s.as_str(), *d))
                .collect::<Vec<_>>(),
            &[
                ("let one = ", None),
                (";", Some(DiagnosticSeverity::ERROR)),
                ("\nlet two =", None),
                (" ", Some(DiagnosticSeverity::ERROR)),
                ("\nlet three = 3;\n", None)
            ]
        );
    });
}

#[gpui::test]
async fn test_diagnostics_from_multiple_language_servers(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "a.rs": "one two three" }))
        .await;

    let project = Project::test(fs, [Path::new(path!("/dir"))], cx).await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());

    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store
            .update_diagnostic_entries(
                LanguageServerId(0),
                Path::new(path!("/dir/a.rs")).to_owned(),
                None,
                None,
                vec![DiagnosticEntry {
                    range: Unclipped(PointUtf16::new(0, 0))..Unclipped(PointUtf16::new(0, 3)),
                    diagnostic: Diagnostic {
                        severity: DiagnosticSeverity::ERROR,
                        is_primary: true,
                        message: "syntax error a1".to_string(),
                        source_kind: DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    },
                }],
                cx,
            )
            .unwrap();
        lsp_store
            .update_diagnostic_entries(
                LanguageServerId(1),
                Path::new(path!("/dir/a.rs")).to_owned(),
                None,
                None,
                vec![DiagnosticEntry {
                    range: Unclipped(PointUtf16::new(0, 0))..Unclipped(PointUtf16::new(0, 3)),
                    diagnostic: Diagnostic {
                        severity: DiagnosticSeverity::ERROR,
                        is_primary: true,
                        message: "syntax error b1".to_string(),
                        source_kind: DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    },
                }],
                cx,
            )
            .unwrap();

        assert_eq!(
            lsp_store.diagnostic_summary(false, cx),
            DiagnosticSummary {
                error_count: 2,
                warning_count: 0,
            }
        );
    });
}

#[gpui::test]
async fn test_diagnostic_summaries_cleared_on_worktree_entry_removal(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "a.rs": "one", "b.rs": "two" }))
        .await;

    let project = Project::test(fs.clone(), [Path::new(path!("/dir"))], cx).await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());

    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store
            .update_diagnostic_entries(
                LanguageServerId(0),
                Path::new(path!("/dir/a.rs")).to_owned(),
                None,
                None,
                vec![DiagnosticEntry {
                    range: Unclipped(PointUtf16::new(0, 0))..Unclipped(PointUtf16::new(0, 3)),
                    diagnostic: Diagnostic {
                        severity: DiagnosticSeverity::ERROR,
                        is_primary: true,
                        message: "error in a".to_string(),
                        source_kind: DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    },
                }],
                cx,
            )
            .unwrap();
        lsp_store
            .update_diagnostic_entries(
                LanguageServerId(0),
                Path::new(path!("/dir/b.rs")).to_owned(),
                None,
                None,
                vec![DiagnosticEntry {
                    range: Unclipped(PointUtf16::new(0, 0))..Unclipped(PointUtf16::new(0, 3)),
                    diagnostic: Diagnostic {
                        severity: DiagnosticSeverity::WARNING,
                        is_primary: true,
                        message: "warning in b".to_string(),
                        source_kind: DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    },
                }],
                cx,
            )
            .unwrap();

        assert_eq!(
            lsp_store.diagnostic_summary(false, cx),
            DiagnosticSummary {
                error_count: 1,
                warning_count: 1,
            }
        );
    });

    fs.remove_file(path!("/dir/a.rs").as_ref(), Default::default())
        .await
        .unwrap();
    cx.executor().run_until_parked();

    lsp_store.update(cx, |lsp_store, cx| {
        assert_eq!(
            lsp_store.diagnostic_summary(false, cx),
            DiagnosticSummary {
                error_count: 0,
                warning_count: 1,
            },
        );
    });
}

#[gpui::test]
async fn test_diagnostic_summaries_cleared_on_server_restart(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "a.rs": "x" })).await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp("Rust", FakeLspAdapter::default());

    let (buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();

    let fake_server = fake_servers.next().await.unwrap();
    fake_server.notify::<lsp::notification::PublishDiagnostics>(lsp::PublishDiagnosticsParams {
        uri: Uri::from_file_path(path!("/dir/a.rs")).unwrap(),
        version: None,
        diagnostics: vec![lsp::Diagnostic {
            range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 1)),
            severity: Some(lsp::DiagnosticSeverity::ERROR),
            message: "error before restart".to_string(),
            ..Default::default()
        }],
    });
    cx.executor().run_until_parked();

    project.update(cx, |project, cx| {
        assert_eq!(
            project.diagnostic_summary(false, cx),
            DiagnosticSummary {
                error_count: 1,
                warning_count: 0,
            }
        );
    });

    let mut events = cx.events(&project);

    project.update(cx, |project, cx| {
        project.restart_language_servers_for_buffers(
            vec![buffer.clone()],
            HashSet::default(),
            true,
            cx,
        );
    });
    cx.executor().run_until_parked();

    let mut received_diagnostics_updated = false;
    while let Some(Some(event)) =
        futures::FutureExt::now_or_never(futures::StreamExt::next(&mut events))
    {
        if matches!(event, Event::DiagnosticsUpdated { .. }) {
            received_diagnostics_updated = true;
        }
    }
    assert!(
        received_diagnostics_updated,
        "DiagnosticsUpdated event should be emitted when a language server is stopped"
    );

    project.update(cx, |project, cx| {
        assert_eq!(
            project.diagnostic_summary(false, cx),
            DiagnosticSummary {
                error_count: 0,
                warning_count: 0,
            }
        );
    });
}

#[gpui::test]
async fn test_diagnostic_summaries_cleared_on_buffer_reload(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "a.rs": "one two three" }))
        .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let pull_count = Arc::new(atomic::AtomicUsize::new(0));
    let closure_pull_count = pull_count.clone();
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                diagnostic_provider: Some(lsp::DiagnosticServerCapabilities::Options(
                    lsp::DiagnosticOptions {
                        identifier: Some("test-reload".to_string()),
                        inter_file_dependencies: true,
                        workspace_diagnostics: false,
                        work_done_progress_options: Default::default(),
                    },
                )),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new(move |fake_server| {
                let pull_count = closure_pull_count.clone();
                fake_server.set_request_handler::<lsp::request::DocumentDiagnosticRequest, _, _>(
                    move |_, _| {
                        let pull_count = pull_count.clone();
                        async move {
                            pull_count.fetch_add(1, atomic::Ordering::SeqCst);
                            Ok(lsp::DocumentDiagnosticReportResult::Report(
                                lsp::DocumentDiagnosticReport::Full(
                                    lsp::RelatedFullDocumentDiagnosticReport {
                                        related_documents: None,
                                        full_document_diagnostic_report:
                                            lsp::FullDocumentDiagnosticReport {
                                                result_id: None,
                                                items: Vec::new(),
                                            },
                                    },
                                ),
                            ))
                        }
                    },
                );
            })),
            ..FakeLspAdapter::default()
        },
    );

    let (_buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();

    let fake_server = fake_servers.next().await.unwrap();
    cx.executor().run_until_parked();

    // Publish initial diagnostics via the fake server.
    fake_server.notify::<lsp::notification::PublishDiagnostics>(lsp::PublishDiagnosticsParams {
        uri: Uri::from_file_path(path!("/dir/a.rs")).unwrap(),
        version: None,
        diagnostics: vec![lsp::Diagnostic {
            range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 3)),
            severity: Some(lsp::DiagnosticSeverity::ERROR),
            message: "error in a".to_string(),
            ..Default::default()
        }],
    });
    cx.executor().run_until_parked();

    project.update(cx, |project, cx| {
        assert_eq!(
            project.diagnostic_summary(false, cx),
            DiagnosticSummary {
                error_count: 1,
                warning_count: 0,
            }
        );
    });

    let pulls_before = pull_count.load(atomic::Ordering::SeqCst);

    // Change the file on disk. The FS event triggers buffer reload,
    // which in turn triggers pull_diagnostics_for_buffer.
    fs.save(
        path!("/dir/a.rs").as_ref(),
        &"fixed content".into(),
        LineEnding::Unix,
    )
    .await
    .unwrap();
    cx.executor().run_until_parked();

    let pulls_after = pull_count.load(atomic::Ordering::SeqCst);
    assert!(
        pulls_after > pulls_before,
        "Expected document diagnostic pull after buffer reload (before={pulls_before}, after={pulls_after})"
    );
}
