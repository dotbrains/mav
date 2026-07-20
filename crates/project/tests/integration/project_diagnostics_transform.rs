use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_updating_lsp_settings_sends_one_did_change_configuration(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "a.rs": "" })).await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    let mut fake_rust_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "rust-lsp",
            ..Default::default()
        },
    );
    language_registry.add(rust_lang());

    let _rs_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();

    let fake_rust_server = fake_rust_servers.next().await.unwrap();

    let did_change_count = Arc::new(atomic::AtomicUsize::new(0));
    fake_rust_server.handle_notification::<lsp::notification::DidChangeConfiguration, _>({
        let did_change_count = did_change_count.clone();
        move |_, _| {
            did_change_count.fetch_add(1, atomic::Ordering::SeqCst);
        }
    });
    cx.executor().run_until_parked();
    did_change_count.store(0, atomic::Ordering::SeqCst);

    cx.update(|cx| {
        SettingsStore::update_global(cx, |settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.project.lsp.0.insert(
                    "rust-lsp".into(),
                    settings::LspSettings {
                        settings: Some(json!({ "foo": true })),
                        ..Default::default()
                    },
                );
            });
        })
    });
    cx.executor().run_until_parked();

    assert_eq!(
        did_change_count.load(atomic::Ordering::SeqCst),
        1,
        "expected exactly one workspace/didChangeConfiguration after a settings change"
    );
}

#[gpui::test(iterations = 3)]
async fn test_transforming_diagnostics(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let text = "
        fn a() { A }
        fn b() { BB }
        fn c() { CCC }
    "
    .unindent();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "a.rs": text })).await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            disk_based_diagnostics_sources: vec!["disk".into()],
            ..Default::default()
        },
    );

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();

    let _handle = project.update(cx, |project, cx| {
        project.register_buffer_with_language_servers(&buffer, cx)
    });

    let mut fake_server = fake_servers.next().await.unwrap();
    let open_notification = fake_server
        .receive_notification::<lsp::notification::DidOpenTextDocument>()
        .await;

    // Edit the buffer, moving the content down
    buffer.update(cx, |buffer, cx| buffer.edit([(0..0, "\n\n")], None, cx));
    let change_notification_1 = fake_server
        .receive_notification::<lsp::notification::DidChangeTextDocument>()
        .await;
    assert!(change_notification_1.text_document.version > open_notification.text_document.version);

    // Report some diagnostics for the initial version of the buffer
    fake_server.notify::<lsp::notification::PublishDiagnostics>(lsp::PublishDiagnosticsParams {
        uri: lsp::Uri::from_file_path(path!("/dir/a.rs")).unwrap(),
        version: Some(open_notification.text_document.version),
        diagnostics: vec![
            lsp::Diagnostic {
                range: lsp::Range::new(lsp::Position::new(0, 9), lsp::Position::new(0, 10)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: "undefined variable 'A'".to_string(),
                source: Some("disk".to_string()),
                ..Default::default()
            },
            lsp::Diagnostic {
                range: lsp::Range::new(lsp::Position::new(1, 9), lsp::Position::new(1, 11)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: "undefined variable 'BB'".to_string(),
                source: Some("disk".to_string()),
                ..Default::default()
            },
            lsp::Diagnostic {
                range: lsp::Range::new(lsp::Position::new(2, 9), lsp::Position::new(2, 12)),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("disk".to_string()),
                message: "undefined variable 'CCC'".to_string(),
                ..Default::default()
            },
        ],
    });

    // The diagnostics have moved down since they were created.
    cx.executor().run_until_parked();
    buffer.update(cx, |buffer, _| {
        assert_eq!(
            buffer
                .snapshot()
                .diagnostics_in_range::<_, Point>(Point::new(3, 0)..Point::new(5, 0), false)
                .collect::<Vec<_>>(),
            &[
                DiagnosticEntry {
                    range: Point::new(3, 9)..Point::new(3, 11),
                    diagnostic: Diagnostic {
                        source: Some("disk".into()),
                        severity: DiagnosticSeverity::ERROR,
                        message: "undefined variable 'BB'".to_string(),
                        is_disk_based: true,
                        group_id: 1,
                        is_primary: true,
                        source_kind: DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    },
                },
                DiagnosticEntry {
                    range: Point::new(4, 9)..Point::new(4, 12),
                    diagnostic: Diagnostic {
                        source: Some("disk".into()),
                        severity: DiagnosticSeverity::ERROR,
                        message: "undefined variable 'CCC'".to_string(),
                        is_disk_based: true,
                        group_id: 2,
                        is_primary: true,
                        source_kind: DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    }
                }
            ]
        );
        assert_eq!(
            chunks_with_diagnostics(buffer, 0..buffer.len()),
            [
                ("\n\nfn a() { ".to_string(), None),
                ("A".to_string(), Some(DiagnosticSeverity::ERROR)),
                (" }\nfn b() { ".to_string(), None),
                ("BB".to_string(), Some(DiagnosticSeverity::ERROR)),
                (" }\nfn c() { ".to_string(), None),
                ("CCC".to_string(), Some(DiagnosticSeverity::ERROR)),
                (" }\n".to_string(), None),
            ]
        );
        assert_eq!(
            chunks_with_diagnostics(buffer, Point::new(3, 10)..Point::new(4, 11)),
            [
                ("B".to_string(), Some(DiagnosticSeverity::ERROR)),
                (" }\nfn c() { ".to_string(), None),
                ("CC".to_string(), Some(DiagnosticSeverity::ERROR)),
            ]
        );
    });

    // Ensure overlapping diagnostics are highlighted correctly.
    fake_server.notify::<lsp::notification::PublishDiagnostics>(lsp::PublishDiagnosticsParams {
        uri: lsp::Uri::from_file_path(path!("/dir/a.rs")).unwrap(),
        version: Some(open_notification.text_document.version),
        diagnostics: vec![
            lsp::Diagnostic {
                range: lsp::Range::new(lsp::Position::new(0, 9), lsp::Position::new(0, 10)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: "undefined variable 'A'".to_string(),
                source: Some("disk".to_string()),
                ..Default::default()
            },
            lsp::Diagnostic {
                range: lsp::Range::new(lsp::Position::new(0, 9), lsp::Position::new(0, 12)),
                severity: Some(DiagnosticSeverity::WARNING),
                message: "unreachable statement".to_string(),
                source: Some("disk".to_string()),
                ..Default::default()
            },
        ],
    });

    cx.executor().run_until_parked();
    buffer.update(cx, |buffer, _| {
        assert_eq!(
            buffer
                .snapshot()
                .diagnostics_in_range::<_, Point>(Point::new(2, 0)..Point::new(3, 0), false)
                .collect::<Vec<_>>(),
            &[
                DiagnosticEntry {
                    range: Point::new(2, 9)..Point::new(2, 12),
                    diagnostic: Diagnostic {
                        source: Some("disk".into()),
                        severity: DiagnosticSeverity::WARNING,
                        message: "unreachable statement".to_string(),
                        is_disk_based: true,
                        group_id: 4,
                        is_primary: true,
                        source_kind: DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    }
                },
                DiagnosticEntry {
                    range: Point::new(2, 9)..Point::new(2, 10),
                    diagnostic: Diagnostic {
                        source: Some("disk".into()),
                        severity: DiagnosticSeverity::ERROR,
                        message: "undefined variable 'A'".to_string(),
                        is_disk_based: true,
                        group_id: 3,
                        is_primary: true,
                        source_kind: DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    },
                }
            ]
        );
        assert_eq!(
            chunks_with_diagnostics(buffer, Point::new(2, 0)..Point::new(3, 0)),
            [
                ("fn a() { ".to_string(), None),
                ("A".to_string(), Some(DiagnosticSeverity::ERROR)),
                (" }".to_string(), Some(DiagnosticSeverity::WARNING)),
                ("\n".to_string(), None),
            ]
        );
        assert_eq!(
            chunks_with_diagnostics(buffer, Point::new(2, 10)..Point::new(3, 0)),
            [
                (" }".to_string(), Some(DiagnosticSeverity::WARNING)),
                ("\n".to_string(), None),
            ]
        );
    });

    // Keep editing the buffer and ensure disk-based diagnostics get translated according to the
    // changes since the last save.
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(2, 0)..Point::new(2, 0), "    ")], None, cx);
        buffer.edit(
            [(Point::new(2, 8)..Point::new(2, 10), "(x: usize)")],
            None,
            cx,
        );
        buffer.edit([(Point::new(3, 10)..Point::new(3, 10), "xxx")], None, cx);
    });
    let change_notification_2 = fake_server
        .receive_notification::<lsp::notification::DidChangeTextDocument>()
        .await;
    assert!(
        change_notification_2.text_document.version > change_notification_1.text_document.version
    );

    // Handle out-of-order diagnostics
    fake_server.notify::<lsp::notification::PublishDiagnostics>(lsp::PublishDiagnosticsParams {
        uri: lsp::Uri::from_file_path(path!("/dir/a.rs")).unwrap(),
        version: Some(change_notification_2.text_document.version),
        diagnostics: vec![
            lsp::Diagnostic {
                range: lsp::Range::new(lsp::Position::new(1, 9), lsp::Position::new(1, 11)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: "undefined variable 'BB'".to_string(),
                source: Some("disk".to_string()),
                ..Default::default()
            },
            lsp::Diagnostic {
                range: lsp::Range::new(lsp::Position::new(0, 9), lsp::Position::new(0, 10)),
                severity: Some(DiagnosticSeverity::WARNING),
                message: "undefined variable 'A'".to_string(),
                source: Some("disk".to_string()),
                ..Default::default()
            },
        ],
    });

    cx.executor().run_until_parked();
    buffer.update(cx, |buffer, _| {
        assert_eq!(
            buffer
                .snapshot()
                .diagnostics_in_range::<_, Point>(0..buffer.len(), false)
                .collect::<Vec<_>>(),
            &[
                DiagnosticEntry {
                    range: Point::new(2, 21)..Point::new(2, 22),
                    diagnostic: Diagnostic {
                        source: Some("disk".into()),
                        severity: DiagnosticSeverity::WARNING,
                        message: "undefined variable 'A'".to_string(),
                        is_disk_based: true,
                        group_id: 6,
                        is_primary: true,
                        source_kind: DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    }
                },
                DiagnosticEntry {
                    range: Point::new(3, 9)..Point::new(3, 14),
                    diagnostic: Diagnostic {
                        source: Some("disk".into()),
                        severity: DiagnosticSeverity::ERROR,
                        message: "undefined variable 'BB'".to_string(),
                        is_disk_based: true,
                        group_id: 5,
                        is_primary: true,
                        source_kind: DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    },
                }
            ]
        );
    });
}
