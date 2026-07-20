use crate::TestServer;
use call::ActiveCall;
use editor::{Editor, MultiBufferOffset, actions::MoveToEnd};
use futures::{StreamExt, lock::Mutex};
use gpui::TestAppContext;
use language::{FakeLspAdapter, rust_lang};
use lsp::DEFAULT_LSP_REQUEST_TIMEOUT;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::{
    collections::BTreeSet,
    ops::Deref as _,
    sync::{
        Arc,
        atomic::{self, AtomicUsize},
    },
};
use util::{path, rel_path::rel_path};

async fn test_lsp_pull_diagnostics(
    should_stream_workspace_diagnostic: bool,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let executor = cx_a.executor();
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    cx_a.update(editor::init);
    cx_b.update(editor::init);

    let expected_push_diagnostic_main_message = "pushed main diagnostic";
    let expected_push_diagnostic_lib_message = "pushed lib diagnostic";
    let expected_pull_diagnostic_main_message = "pulled main diagnostic";
    let expected_pull_diagnostic_lib_message = "pulled lib diagnostic";
    let expected_workspace_pull_diagnostics_main_message = "pulled workspace main diagnostic";
    let expected_workspace_pull_diagnostics_lib_message = "pulled workspace lib diagnostic";

    let diagnostics_pulls_result_ids = Arc::new(Mutex::new(BTreeSet::<Option<String>>::new()));
    let workspace_diagnostics_pulls_result_ids = Arc::new(Mutex::new(BTreeSet::<String>::new()));
    let diagnostics_pulls_made = Arc::new(AtomicUsize::new(0));
    let closure_diagnostics_pulls_made = diagnostics_pulls_made.clone();
    let closure_diagnostics_pulls_result_ids = diagnostics_pulls_result_ids.clone();
    let workspace_diagnostics_pulls_made = Arc::new(AtomicUsize::new(0));
    let closure_workspace_diagnostics_pulls_made = workspace_diagnostics_pulls_made.clone();
    let closure_workspace_diagnostics_pulls_result_ids =
        workspace_diagnostics_pulls_result_ids.clone();
    let (workspace_diagnostic_cancel_tx, closure_workspace_diagnostic_cancel_rx) =
        async_channel::bounded::<()>(1);
    let (closure_workspace_diagnostic_received_tx, workspace_diagnostic_received_rx) =
        async_channel::bounded::<()>(1);

    let capabilities = lsp::ServerCapabilities {
        diagnostic_provider: Some(lsp::DiagnosticServerCapabilities::Options(
            lsp::DiagnosticOptions {
                identifier: Some("test-pulls".to_string()),
                inter_file_dependencies: true,
                workspace_diagnostics: true,
                work_done_progress_options: lsp::WorkDoneProgressOptions {
                    work_done_progress: None,
                },
            },
        )),
        ..lsp::ServerCapabilities::default()
    };
    client_a.language_registry().add(rust_lang());

    let pull_diagnostics_handle = Arc::new(parking_lot::Mutex::new(None));
    let workspace_diagnostics_pulls_handle = Arc::new(parking_lot::Mutex::new(None));

    let closure_pull_diagnostics_handle = pull_diagnostics_handle.clone();
    let closure_workspace_diagnostics_pulls_handle = workspace_diagnostics_pulls_handle.clone();
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: capabilities.clone(),
            initializer: Some(Box::new(move |fake_language_server| {
                let expected_workspace_diagnostic_token = lsp::ProgressToken::String(format!(
                    "workspace/diagnostic/{}/1",
                    fake_language_server.server.server_id()
                ));
                let closure_workspace_diagnostics_pulls_result_ids = closure_workspace_diagnostics_pulls_result_ids.clone();
                let diagnostics_pulls_made = closure_diagnostics_pulls_made.clone();
                let diagnostics_pulls_result_ids = closure_diagnostics_pulls_result_ids.clone();
                let closure_pull_diagnostics_handle = closure_pull_diagnostics_handle.clone();
                let closure_workspace_diagnostics_pulls_handle = closure_workspace_diagnostics_pulls_handle.clone();
                let closure_workspace_diagnostic_cancel_rx = closure_workspace_diagnostic_cancel_rx.clone();
                let closure_workspace_diagnostic_received_tx = closure_workspace_diagnostic_received_tx.clone();
                let pull_diagnostics_handle = fake_language_server
                    .set_request_handler::<lsp::request::DocumentDiagnosticRequest, _, _>(
                        move |params, _| {
                            let requests_made = diagnostics_pulls_made.clone();
                            let diagnostics_pulls_result_ids =
                                diagnostics_pulls_result_ids.clone();
                            async move {
                                let message = if lsp::Uri::from_file_path(path!("/a/main.rs"))
                                    .unwrap()
                                    == params.text_document.uri
                                {
                                    expected_pull_diagnostic_main_message.to_string()
                                } else if lsp::Uri::from_file_path(path!("/a/lib.rs")).unwrap()
                                    == params.text_document.uri
                                {
                                    expected_pull_diagnostic_lib_message.to_string()
                                } else {
                                    panic!("Unexpected document: {}", params.text_document.uri)
                                };
                                {
                                    diagnostics_pulls_result_ids
                                        .lock()
                                        .await
                                        .insert(params.previous_result_id);
                                }
                                let new_requests_count =
                                    requests_made.fetch_add(1, atomic::Ordering::Release) + 1;
                                Ok(lsp::DocumentDiagnosticReportResult::Report(
                                    lsp::DocumentDiagnosticReport::Full(
                                        lsp::RelatedFullDocumentDiagnosticReport {
                                            related_documents: None,
                                            full_document_diagnostic_report:
                                                lsp::FullDocumentDiagnosticReport {
                                                    result_id: Some(format!(
                                                        "pull-{new_requests_count}"
                                                    )),
                                                    items: vec![lsp::Diagnostic {
                                                        range: lsp::Range {
                                                            start: lsp::Position {
                                                                line: 0,
                                                                character: 0,
                                                            },
                                                            end: lsp::Position {
                                                                line: 0,
                                                                character: 2,
                                                            },
                                                        },
                                                        severity: Some(
                                                            lsp::DiagnosticSeverity::ERROR,
                                                        ),
                                                        message,
                                                        ..lsp::Diagnostic::default()
                                                    }],
                                                },
                                        },
                                    ),
                                ))
                            }
                        },
                    );
                let _ = closure_pull_diagnostics_handle.lock().insert(pull_diagnostics_handle);

                let closure_workspace_diagnostics_pulls_made = closure_workspace_diagnostics_pulls_made.clone();
                let workspace_diagnostics_pulls_handle = fake_language_server.set_request_handler::<lsp::request::WorkspaceDiagnosticRequest, _, _>(
                    move |params, _| {
                        let workspace_requests_made = closure_workspace_diagnostics_pulls_made.clone();
                        let workspace_diagnostics_pulls_result_ids =
                            closure_workspace_diagnostics_pulls_result_ids.clone();
                        let workspace_diagnostic_cancel_rx = closure_workspace_diagnostic_cancel_rx.clone();
                        let workspace_diagnostic_received_tx = closure_workspace_diagnostic_received_tx.clone();
                        let expected_workspace_diagnostic_token = expected_workspace_diagnostic_token.clone();
                        async move {
                            let workspace_request_count =
                                workspace_requests_made.fetch_add(1, atomic::Ordering::Release) + 1;
                            {
                                workspace_diagnostics_pulls_result_ids
                                    .lock()
                                    .await
                                    .extend(params.previous_result_ids.into_iter().map(|id| id.value));
                            }
                            if should_stream_workspace_diagnostic && !workspace_diagnostic_cancel_rx.is_closed()
                            {
                                assert_eq!(
                                    params.partial_result_params.partial_result_token,
                                    Some(expected_workspace_diagnostic_token)
                                );
                                workspace_diagnostic_received_tx.send(()).await.unwrap();
                                workspace_diagnostic_cancel_rx.recv().await.unwrap();
                                workspace_diagnostic_cancel_rx.close();
                                // https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#partialResults
                                // > The final response has to be empty in terms of result values.
                                return Ok(lsp::WorkspaceDiagnosticReportResult::Report(
                                    lsp::WorkspaceDiagnosticReport { items: Vec::new() },
                                ));
                            }
                            Ok(lsp::WorkspaceDiagnosticReportResult::Report(
                                lsp::WorkspaceDiagnosticReport {
                                    items: vec![
                                        lsp::WorkspaceDocumentDiagnosticReport::Full(
                                            lsp::WorkspaceFullDocumentDiagnosticReport {
                                                uri: lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                                                version: None,
                                                full_document_diagnostic_report:
                                                    lsp::FullDocumentDiagnosticReport {
                                                        result_id: Some(format!(
                                                            "workspace_{workspace_request_count}"
                                                        )),
                                                        items: vec![lsp::Diagnostic {
                                                            range: lsp::Range {
                                                                start: lsp::Position {
                                                                    line: 0,
                                                                    character: 1,
                                                                },
                                                                end: lsp::Position {
                                                                    line: 0,
                                                                    character: 3,
                                                                },
                                                            },
                                                            severity: Some(lsp::DiagnosticSeverity::WARNING),
                                                            message:
                                                                expected_workspace_pull_diagnostics_main_message
                                                                    .to_string(),
                                                            ..lsp::Diagnostic::default()
                                                        }],
                                                    },
                                            },
                                        ),
                                        lsp::WorkspaceDocumentDiagnosticReport::Full(
                                            lsp::WorkspaceFullDocumentDiagnosticReport {
                                                uri: lsp::Uri::from_file_path(path!("/a/lib.rs")).unwrap(),
                                                version: None,
                                                full_document_diagnostic_report:
                                                    lsp::FullDocumentDiagnosticReport {
                                                        result_id: Some(format!(
                                                            "workspace_{workspace_request_count}"
                                                        )),
                                                        items: vec![lsp::Diagnostic {
                                                            range: lsp::Range {
                                                                start: lsp::Position {
                                                                    line: 0,
                                                                    character: 1,
                                                                },
                                                                end: lsp::Position {
                                                                    line: 0,
                                                                    character: 3,
                                                                },
                                                            },
                                                            severity: Some(lsp::DiagnosticSeverity::WARNING),
                                                            message:
                                                                expected_workspace_pull_diagnostics_lib_message
                                                                    .to_string(),
                                                            ..lsp::Diagnostic::default()
                                                        }],
                                                    },
                                            },
                                        ),
                                    ],
                                },
                            ))
                        }
                    });
                let _ = closure_workspace_diagnostics_pulls_handle.lock().insert(workspace_diagnostics_pulls_handle);
            })),
            ..FakeLspAdapter::default()
        },
    );

    client_b.language_registry().add(rust_lang());
    client_b.language_registry().register_fake_lsp_adapter(
        "Rust",
        FakeLspAdapter {
            capabilities,
            ..FakeLspAdapter::default()
        },
    );

    // Client A opens a project.
    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "main.rs": "fn main() { a }",
                "lib.rs": "fn other() {}",
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/a"), cx_a).await;
    active_call_a
        .update(cx_a, |call, cx| call.set_location(Some(&project_a), cx))
        .await
        .unwrap();
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    // Client B joins the project
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    active_call_b
        .update(cx_b, |call, cx| call.set_location(Some(&project_b), cx))
        .await
        .unwrap();

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);

    // The host opens a rust file.
    let _buffer_a = project_a
        .update(cx_a, |project, cx| {
            project.open_local_buffer(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let editor_a_main = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let fake_language_server = fake_language_servers.next().await.unwrap();
    let expected_workspace_diagnostic_token = lsp::ProgressToken::String(format!(
        "workspace/diagnostic-{}-1",
        fake_language_server.server.server_id()
    ));
    cx_a.run_until_parked();
    cx_b.run_until_parked();
    let mut pull_diagnostics_handle = pull_diagnostics_handle.lock().take().unwrap();
    let mut workspace_diagnostics_pulls_handle =
        workspace_diagnostics_pulls_handle.lock().take().unwrap();

    if should_stream_workspace_diagnostic {
        workspace_diagnostic_received_rx.recv().await.unwrap();
    } else {
        workspace_diagnostics_pulls_handle.next().await.unwrap();
    }
    assert_eq!(
        1,
        workspace_diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "Workspace diagnostics should be pulled initially on a server startup"
    );
    pull_diagnostics_handle.next().await.unwrap();
    assert_eq!(
        1,
        diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "Host should query pull diagnostics when the editor is opened"
    );
    executor.run_until_parked();
    editor_a_main.update(cx_a, |editor, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let all_diagnostics = snapshot
            .diagnostics_in_range(MultiBufferOffset(0)..snapshot.len())
            .collect::<Vec<_>>();
        assert_eq!(
            all_diagnostics.len(),
            1,
            "Expected single diagnostic, but got: {all_diagnostics:?}"
        );
        let diagnostic = &all_diagnostics[0];
        let mut expected_messages = vec![expected_pull_diagnostic_main_message];
        if !should_stream_workspace_diagnostic {
            expected_messages.push(expected_workspace_pull_diagnostics_main_message);
        }
        assert!(
            expected_messages.contains(&diagnostic.diagnostic.message.as_str()),
            "Expected {expected_messages:?} on the host, but got: {}",
            diagnostic.diagnostic.message
        );
    });

    fake_language_server.notify::<lsp::notification::PublishDiagnostics>(
        lsp::PublishDiagnosticsParams {
            uri: lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
            diagnostics: vec![lsp::Diagnostic {
                range: lsp::Range {
                    start: lsp::Position {
                        line: 0,
                        character: 3,
                    },
                    end: lsp::Position {
                        line: 0,
                        character: 4,
                    },
                },
                severity: Some(lsp::DiagnosticSeverity::INFORMATION),
                message: expected_push_diagnostic_main_message.to_string(),
                ..lsp::Diagnostic::default()
            }],
            version: None,
        },
    );
    fake_language_server.notify::<lsp::notification::PublishDiagnostics>(
        lsp::PublishDiagnosticsParams {
            uri: lsp::Uri::from_file_path(path!("/a/lib.rs")).unwrap(),
            diagnostics: vec![lsp::Diagnostic {
                range: lsp::Range {
                    start: lsp::Position {
                        line: 0,
                        character: 3,
                    },
                    end: lsp::Position {
                        line: 0,
                        character: 4,
                    },
                },
                severity: Some(lsp::DiagnosticSeverity::INFORMATION),
                message: expected_push_diagnostic_lib_message.to_string(),
                ..lsp::Diagnostic::default()
            }],
            version: None,
        },
    );

    if should_stream_workspace_diagnostic {
        fake_language_server.notify::<lsp::notification::Progress>(lsp::ProgressParams {
            token: expected_workspace_diagnostic_token.clone(),
            value: lsp::ProgressParamsValue::WorkspaceDiagnostic(
                lsp::WorkspaceDiagnosticReportResult::Report(lsp::WorkspaceDiagnosticReport {
                    items: vec![
                        lsp::WorkspaceDocumentDiagnosticReport::Full(
                            lsp::WorkspaceFullDocumentDiagnosticReport {
                                uri: lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                                version: None,
                                full_document_diagnostic_report:
                                    lsp::FullDocumentDiagnosticReport {
                                        result_id: Some(format!(
                                            "workspace_{}",
                                            workspace_diagnostics_pulls_made
                                                .fetch_add(1, atomic::Ordering::Release)
                                                + 1
                                        )),
                                        items: vec![lsp::Diagnostic {
                                            range: lsp::Range {
                                                start: lsp::Position {
                                                    line: 0,
                                                    character: 1,
                                                },
                                                end: lsp::Position {
                                                    line: 0,
                                                    character: 2,
                                                },
                                            },
                                            severity: Some(lsp::DiagnosticSeverity::ERROR),
                                            message:
                                                expected_workspace_pull_diagnostics_main_message
                                                    .to_string(),
                                            ..lsp::Diagnostic::default()
                                        }],
                                    },
                            },
                        ),
                        lsp::WorkspaceDocumentDiagnosticReport::Full(
                            lsp::WorkspaceFullDocumentDiagnosticReport {
                                uri: lsp::Uri::from_file_path(path!("/a/lib.rs")).unwrap(),
                                version: None,
                                full_document_diagnostic_report:
                                    lsp::FullDocumentDiagnosticReport {
                                        result_id: Some(format!(
                                            "workspace_{}",
                                            workspace_diagnostics_pulls_made
                                                .fetch_add(1, atomic::Ordering::Release)
                                                + 1
                                        )),
                                        items: Vec::new(),
                                    },
                            },
                        ),
                    ],
                }),
            ),
        });
    };

    include!("editor_diagnostics_refresh_tail.rs");
}

#[gpui::test(iterations = 10)]
async fn test_non_streamed_lsp_pull_diagnostics(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    test_lsp_pull_diagnostics(false, cx_a, cx_b).await;
}

#[gpui::test(iterations = 10)]
async fn test_streamed_lsp_pull_diagnostics(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    test_lsp_pull_diagnostics(true, cx_a, cx_b).await;
}
