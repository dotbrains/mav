use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_disk_based_diagnostics_progress(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let progress_token = "the-progress-token";

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.rs": "fn a() { A }",
            "b.rs": "const y: i32 = 1",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            disk_based_diagnostics_progress_token: Some(progress_token.into()),
            disk_based_diagnostics_sources: vec!["disk".into()],
            ..Default::default()
        },
    );

    let worktree_id = project.update(cx, |p, cx| p.worktrees(cx).next().unwrap().read(cx).id());

    // Cause worktree to start the fake language server
    let _ = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/b.rs"), cx)
        })
        .await
        .unwrap();

    let mut events = cx.events(&project);

    let fake_server = fake_servers.next().await.unwrap();
    assert_eq!(
        events.next().await.unwrap(),
        Event::LanguageServerAdded(
            LanguageServerId(0),
            fake_server.server.name(),
            Some(worktree_id)
        ),
    );

    fake_server
        .start_progress(format!("{}/0", progress_token))
        .await;
    assert_eq!(
        events.next().await.unwrap(),
        Event::DiskBasedDiagnosticsStarted {
            language_server_id: LanguageServerId(0),
        }
    );

    fake_server.notify::<lsp::notification::PublishDiagnostics>(lsp::PublishDiagnosticsParams {
        uri: Uri::from_file_path(path!("/dir/a.rs")).unwrap(),
        version: None,
        diagnostics: vec![lsp::Diagnostic {
            range: lsp::Range::new(lsp::Position::new(0, 9), lsp::Position::new(0, 10)),
            severity: Some(lsp::DiagnosticSeverity::ERROR),
            message: "undefined variable 'A'".to_string(),
            ..Default::default()
        }],
    });
    assert_eq!(
        events.next().await.unwrap(),
        Event::DiagnosticsUpdated {
            language_server_id: LanguageServerId(0),
            paths: vec![(worktree_id, rel_path("a.rs")).into()],
        }
    );

    fake_server.end_progress(format!("{}/0", progress_token));
    assert_eq!(
        events.next().await.unwrap(),
        Event::DiskBasedDiagnosticsFinished {
            language_server_id: LanguageServerId(0)
        }
    );

    let buffer = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/a.rs"), cx))
        .await
        .unwrap();

    buffer.update(cx, |buffer, _| {
        let snapshot = buffer.snapshot();
        let diagnostics = snapshot
            .diagnostics_in_range::<_, Point>(0..buffer.len(), false)
            .collect::<Vec<_>>();
        assert_eq!(
            diagnostics,
            &[DiagnosticEntryRef {
                range: Point::new(0, 9)..Point::new(0, 10),
                diagnostic: &Diagnostic {
                    severity: lsp::DiagnosticSeverity::ERROR,
                    message: "undefined variable 'A'".to_string(),
                    group_id: 0,
                    is_primary: true,
                    source_kind: DiagnosticSourceKind::Pushed,
                    ..Diagnostic::default()
                }
            }]
        )
    });

    // Ensure publishing empty diagnostics twice only results in one update event.
    fake_server.notify::<lsp::notification::PublishDiagnostics>(lsp::PublishDiagnosticsParams {
        uri: Uri::from_file_path(path!("/dir/a.rs")).unwrap(),
        version: None,
        diagnostics: Default::default(),
    });
    assert_eq!(
        events.next().await.unwrap(),
        Event::DiagnosticsUpdated {
            language_server_id: LanguageServerId(0),
            paths: vec![(worktree_id, rel_path("a.rs")).into()],
        }
    );

    fake_server.notify::<lsp::notification::PublishDiagnostics>(lsp::PublishDiagnosticsParams {
        uri: Uri::from_file_path(path!("/dir/a.rs")).unwrap(),
        version: None,
        diagnostics: Default::default(),
    });
    cx.executor().run_until_parked();
    assert_eq!(futures::poll!(events.next()), Poll::Pending);
}

#[gpui::test]
async fn test_restarting_server_with_diagnostics_running(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let progress_token = "the-progress-token";

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "a.rs": "" })).await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "the-language-server",
            disk_based_diagnostics_sources: vec!["disk".into()],
            disk_based_diagnostics_progress_token: Some(progress_token.into()),
            ..FakeLspAdapter::default()
        },
    );

    let worktree_id = project.update(cx, |p, cx| p.worktrees(cx).next().unwrap().read(cx).id());

    let (buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();
    let buffer_id = buffer.read_with(cx, |buffer, _| buffer.remote_id());
    // Simulate diagnostics starting to update.
    let fake_server = fake_servers.next().await.unwrap();
    cx.executor().run_until_parked();
    fake_server.start_progress(progress_token).await;

    // Restart the server before the diagnostics finish updating.
    project.update(cx, |project, cx| {
        project.restart_language_servers_for_buffers(vec![buffer], HashSet::default(), true, cx);
    });
    let mut events = cx.events(&project);

    // Simulate the newly started server sending more diagnostics.
    let fake_server = fake_servers.next().await.unwrap();
    cx.executor().run_until_parked();
    assert_eq!(
        events.next().await.unwrap(),
        Event::LanguageServerRemoved(LanguageServerId(0))
    );
    assert_eq!(
        events.next().await.unwrap(),
        Event::LanguageServerAdded(
            LanguageServerId(1),
            fake_server.server.name(),
            Some(worktree_id)
        )
    );
    fake_server.start_progress(progress_token).await;
    assert_eq!(
        events.next().await.unwrap(),
        Event::LanguageServerBufferRegistered {
            server_id: LanguageServerId(1),
            buffer_id,
            buffer_abs_path: PathBuf::from(path!("/dir/a.rs")),
            name: Some(fake_server.server.name())
        }
    );
    assert_eq!(
        events.next().await.unwrap(),
        Event::DiskBasedDiagnosticsStarted {
            language_server_id: LanguageServerId(1)
        }
    );
    project.update(cx, |project, cx| {
        assert_eq!(
            project
                .language_servers_running_disk_based_diagnostics(cx)
                .collect::<Vec<_>>(),
            [LanguageServerId(1)]
        );
    });

    // All diagnostics are considered done, despite the old server's diagnostic
    // task never completing.
    fake_server.end_progress(progress_token);
    assert_eq!(
        events.next().await.unwrap(),
        Event::DiskBasedDiagnosticsFinished {
            language_server_id: LanguageServerId(1)
        }
    );
    project.update(cx, |project, cx| {
        assert_eq!(
            project
                .language_servers_running_disk_based_diagnostics(cx)
                .collect::<Vec<_>>(),
            [] as [language::LanguageServerId; 0]
        );
    });
}

#[gpui::test]
async fn test_restarting_server_with_diagnostics_published(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "a.rs": "x" })).await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp("Rust", FakeLspAdapter::default());

    let (buffer, _) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();

    // Publish diagnostics
    let fake_server = fake_servers.next().await.unwrap();
    fake_server.notify::<lsp::notification::PublishDiagnostics>(lsp::PublishDiagnosticsParams {
        uri: Uri::from_file_path(path!("/dir/a.rs")).unwrap(),
        version: None,
        diagnostics: vec![lsp::Diagnostic {
            range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 0)),
            severity: Some(lsp::DiagnosticSeverity::ERROR),
            message: "the message".to_string(),
            ..Default::default()
        }],
    });

    cx.executor().run_until_parked();
    buffer.update(cx, |buffer, _| {
        assert_eq!(
            buffer
                .snapshot()
                .diagnostics_in_range::<_, usize>(0..1, false)
                .map(|entry| entry.diagnostic.message.clone())
                .collect::<Vec<_>>(),
            ["the message".to_string()]
        );
    });
    project.update(cx, |project, cx| {
        assert_eq!(
            project.diagnostic_summary(false, cx),
            DiagnosticSummary {
                error_count: 1,
                warning_count: 0,
            }
        );
    });

    project.update(cx, |project, cx| {
        project.restart_language_servers_for_buffers(
            vec![buffer.clone()],
            HashSet::default(),
            true,
            cx,
        );
    });

    // The diagnostics are cleared.
    cx.executor().run_until_parked();
    buffer.update(cx, |buffer, _| {
        assert_eq!(
            buffer
                .snapshot()
                .diagnostics_in_range::<_, usize>(0..1, false)
                .map(|entry| entry.diagnostic.message.clone())
                .collect::<Vec<_>>(),
            Vec::<String>::new(),
        );
    });
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
async fn test_restarted_server_reporting_invalid_buffer_version(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "a.rs": "" })).await;

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

    // Before restarting the server, report diagnostics with an unknown buffer version.
    let fake_server = fake_servers.next().await.unwrap();
    fake_server.notify::<lsp::notification::PublishDiagnostics>(lsp::PublishDiagnosticsParams {
        uri: lsp::Uri::from_file_path(path!("/dir/a.rs")).unwrap(),
        version: Some(10000),
        diagnostics: Vec::new(),
    });
    cx.executor().run_until_parked();
    project.update(cx, |project, cx| {
        project.restart_language_servers_for_buffers(
            vec![buffer.clone()],
            HashSet::default(),
            true,
            cx,
        );
    });

    let mut fake_server = fake_servers.next().await.unwrap();
    let notification = fake_server
        .receive_notification::<lsp::notification::DidOpenTextDocument>()
        .await
        .text_document;
    assert_eq!(notification.version, 0);
}
