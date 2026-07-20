use super::*;

#[gpui::test(iterations = 10)]
async fn test_pulling_diagnostics(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let diagnostic_requests = Arc::new(AtomicUsize::new(0));
    let counter = diagnostic_requests.clone();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "first.rs": "fn main() { let a = 5; }",
            "second.rs": "// Test file",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                diagnostic_provider: Some(lsp::DiagnosticServerCapabilities::Options(
                    lsp::DiagnosticOptions {
                        identifier: None,
                        inter_file_dependencies: true,
                        workspace_diagnostics: true,
                        work_done_progress_options: Default::default(),
                    },
                )),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/a/first.rs")),
                OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    let fake_server = fake_servers.next().await.unwrap();
    let server_id = fake_server.server.server_id();
    let mut first_request = fake_server
        .set_request_handler::<lsp::request::DocumentDiagnosticRequest, _, _>(move |params, _| {
            let new_result_id = counter.fetch_add(1, atomic::Ordering::Release) + 1;
            let result_id = Some(new_result_id.to_string());
            assert_eq!(
                params.text_document.uri,
                lsp::Uri::from_file_path(path!("/a/first.rs")).unwrap()
            );
            async move {
                Ok(lsp::DocumentDiagnosticReportResult::Report(
                    lsp::DocumentDiagnosticReport::Full(lsp::RelatedFullDocumentDiagnosticReport {
                        related_documents: None,
                        full_document_diagnostic_report: lsp::FullDocumentDiagnosticReport {
                            items: Vec::new(),
                            result_id,
                        },
                    }),
                ))
            }
        });

    let ensure_result_id = |expected_result_id: Option<SharedString>, cx: &mut TestAppContext| {
        project.update(cx, |project, cx| {
            let buffer_id = editor
                .read(cx)
                .buffer()
                .read(cx)
                .as_singleton()
                .expect("created a singleton buffer")
                .read(cx)
                .remote_id();
            let buffer_result_id = project
                .lsp_store()
                .read(cx)
                .result_id_for_buffer_pull(server_id, buffer_id, &None, cx);
            assert_eq!(expected_result_id, buffer_result_id);
        });
    };

    ensure_result_id(None, cx);
    cx.executor().advance_clock(Duration::from_millis(60));
    cx.executor().run_until_parked();
    assert_eq!(
        diagnostic_requests.load(atomic::Ordering::Acquire),
        1,
        "Opening file should trigger diagnostic request"
    );
    first_request
        .next()
        .await
        .expect("should have sent the first diagnostics pull request");
    ensure_result_id(Some(SharedString::new_static("1")), cx);

    // Editing should trigger diagnostics
    editor.update_in(cx, |editor, window, cx| {
        editor.handle_input("2", window, cx)
    });
    cx.executor().advance_clock(Duration::from_millis(60));
    cx.executor().run_until_parked();
    assert_eq!(
        diagnostic_requests.load(atomic::Ordering::Acquire),
        2,
        "Editing should trigger diagnostic request"
    );
    ensure_result_id(Some(SharedString::new_static("2")), cx);

    // Moving cursor should not trigger diagnostic request
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(0, 0)])
        });
    });
    cx.executor().advance_clock(Duration::from_millis(60));
    cx.executor().run_until_parked();
    assert_eq!(
        diagnostic_requests.load(atomic::Ordering::Acquire),
        2,
        "Cursor movement should not trigger diagnostic request"
    );
    ensure_result_id(Some(SharedString::new_static("2")), cx);
    // Multiple rapid edits should be debounced
    for _ in 0..5 {
        editor.update_in(cx, |editor, window, cx| {
            editor.handle_input("x", window, cx)
        });
    }
    cx.executor().advance_clock(Duration::from_millis(60));
    cx.executor().run_until_parked();

    let final_requests = diagnostic_requests.load(atomic::Ordering::Acquire);
    assert!(
        final_requests <= 4,
        "Multiple rapid edits should be debounced (got {final_requests} requests)",
    );
    ensure_result_id(Some(SharedString::new(final_requests.to_string())), cx);
}
