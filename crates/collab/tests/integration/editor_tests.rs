use crate::TestServer;
use call::ActiveCall;
use collections::{HashMap, HashSet};
use editor::{Editor, LSP_REQUEST_DEBOUNCE_TIMEOUT, MultiBufferOffset, actions::MoveToEnd};
use futures::{StreamExt, lock::Mutex};
use gpui::{TestAppContext, UpdateGlobal, VisualTestContext};
use language::{FakeLspAdapter, rust_lang};
use lsp::DEFAULT_LSP_REQUEST_TIMEOUT;
use multi_buffer::MultiBufferRow;
use pretty_assertions::assert_eq;
use project::trusted_worktrees::{PathTrust, TrustedWorktrees};
use serde_json::json;
use settings::{DocumentFoldingRanges, DocumentSymbols, SettingsStore};
use std::{
    collections::BTreeSet,
    ops::Deref as _,
    sync::{
        Arc,
        atomic::{self, AtomicUsize},
    },
    time::Duration,
};
use util::{path, rel_path::rel_path};
use workspace::item::Item as _;

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

    let mut workspace_diagnostic_start_count =
        workspace_diagnostics_pulls_made.load(atomic::Ordering::Acquire);

    executor.run_until_parked();
    editor_a_main.update(cx_a, |editor, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let all_diagnostics = snapshot
            .diagnostics_in_range(MultiBufferOffset(0)..snapshot.len())
            .collect::<Vec<_>>();
        assert_eq!(
            all_diagnostics.len(),
            2,
            "Expected pull and push diagnostics, but got: {all_diagnostics:?}"
        );
        let expected_messages = [
            expected_workspace_pull_diagnostics_main_message,
            expected_pull_diagnostic_main_message,
            expected_push_diagnostic_main_message,
        ];
        for diagnostic in all_diagnostics {
            assert!(
                expected_messages.contains(&diagnostic.diagnostic.message.as_str()),
                "Expected push and pull messages on the host: {expected_messages:?}, but got: {}",
                diagnostic.diagnostic.message
            );
        }
    });

    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);
    let editor_b_main = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    cx_b.run_until_parked();

    pull_diagnostics_handle.next().await.unwrap();
    assert_eq!(
        2,
        diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "Client should query pull diagnostics when its editor is opened"
    );
    executor.run_until_parked();
    assert_eq!(
        workspace_diagnostic_start_count,
        workspace_diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "Workspace diagnostics should not be changed as the remote client does not initialize the workspace diagnostics pull"
    );
    editor_b_main.update(cx_b, |editor, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let all_diagnostics = snapshot
            .diagnostics_in_range(MultiBufferOffset(0)..snapshot.len())
            .collect::<Vec<_>>();
        assert_eq!(
            all_diagnostics.len(),
            2,
            "Expected pull and push diagnostics, but got: {all_diagnostics:?}"
        );

        // Despite the workspace diagnostics not re-initialized for the remote client, we can still expect its message synced from the host.
        let expected_messages = [
            expected_workspace_pull_diagnostics_main_message,
            expected_pull_diagnostic_main_message,
            expected_push_diagnostic_main_message,
        ];
        for diagnostic in all_diagnostics {
            assert!(
                expected_messages.contains(&diagnostic.diagnostic.message.as_str()),
                "The client should get both push and pull messages: {expected_messages:?}, but got: {}",
                diagnostic.diagnostic.message
            );
        }
    });

    let editor_b_lib = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("lib.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    pull_diagnostics_handle.next().await.unwrap();
    assert_eq!(
        3,
        diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "Client should query pull diagnostics when its another editor is opened"
    );
    executor.run_until_parked();
    assert_eq!(
        workspace_diagnostic_start_count,
        workspace_diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "The remote client still did not anything to trigger the workspace diagnostics pull"
    );
    editor_b_lib.update(cx_b, |editor, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let all_diagnostics = snapshot
            .diagnostics_in_range(MultiBufferOffset(0)..snapshot.len())
            .collect::<Vec<_>>();
        let expected_messages = [
            expected_pull_diagnostic_lib_message,
            expected_push_diagnostic_lib_message,
        ];
        assert_eq!(
            all_diagnostics.len(),
            2,
            "Expected pull and push diagnostics, but got: {all_diagnostics:?}"
        );
        for diagnostic in all_diagnostics {
            assert!(
                expected_messages.contains(&diagnostic.diagnostic.message.as_str()),
                "The client should get both push and pull messages: {expected_messages:?}, but got: {}",
                diagnostic.diagnostic.message
            );
        }
    });

    if should_stream_workspace_diagnostic {
        fake_language_server.notify::<lsp::notification::Progress>(lsp::ProgressParams {
            token: expected_workspace_diagnostic_token.clone(),
            value: lsp::ProgressParamsValue::WorkspaceDiagnostic(
                lsp::WorkspaceDiagnosticReportResult::Report(lsp::WorkspaceDiagnosticReport {
                    items: vec![lsp::WorkspaceDocumentDiagnosticReport::Full(
                        lsp::WorkspaceFullDocumentDiagnosticReport {
                            uri: lsp::Uri::from_file_path(path!("/a/lib.rs")).unwrap(),
                            version: None,
                            full_document_diagnostic_report: lsp::FullDocumentDiagnosticReport {
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
                                    message: expected_workspace_pull_diagnostics_lib_message
                                        .to_string(),
                                    ..lsp::Diagnostic::default()
                                }],
                            },
                        },
                    )],
                }),
            ),
        });
        workspace_diagnostic_start_count =
            workspace_diagnostics_pulls_made.load(atomic::Ordering::Acquire);
        workspace_diagnostic_cancel_tx.send(()).await.unwrap();
        workspace_diagnostics_pulls_handle.next().await.unwrap();
        executor.run_until_parked();
        editor_b_lib.update(cx_b, |editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let all_diagnostics = snapshot
                .diagnostics_in_range(MultiBufferOffset(0)..snapshot.len())
                .collect::<Vec<_>>();
            let expected_messages = [
                // Despite workspace diagnostics provided,
                // the currently open file's diagnostics should be preferred, as LSP suggests.
                expected_pull_diagnostic_lib_message,
                expected_push_diagnostic_lib_message,
            ];
            assert_eq!(
                all_diagnostics.len(),
                2,
                "Expected pull and push diagnostics, but got: {all_diagnostics:?}"
            );
            for diagnostic in all_diagnostics {
                assert!(
                    expected_messages.contains(&diagnostic.diagnostic.message.as_str()),
                    "The client should get both push and pull messages: {expected_messages:?}, but got: {}",
                    diagnostic.diagnostic.message
                );
            }
        });
    };

    {
        assert!(
            !diagnostics_pulls_result_ids.lock().await.is_empty(),
            "Initial diagnostics pulls should report None at least"
        );
        assert_eq!(
            0,
            workspace_diagnostics_pulls_result_ids
                .lock()
                .await
                .deref()
                .len(),
            "After the initial workspace request, opening files should not reuse any result ids"
        );
    }

    editor_b_lib.update_in(cx_b, |editor, window, cx| {
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.handle_input(":", window, cx);
    });
    pull_diagnostics_handle.next().await.unwrap();
    // pull_diagnostics_handle.next().await.unwrap();
    assert_eq!(
        4,
        diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "Client lib.rs edits should trigger another diagnostics pull for open buffers"
    );
    workspace_diagnostics_pulls_handle.next().await.unwrap();
    assert_eq!(
        workspace_diagnostic_start_count + 1,
        workspace_diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "After client lib.rs edits, the workspace diagnostics request should follow"
    );
    executor.run_until_parked();

    editor_b_main.update_in(cx_b, |editor, window, cx| {
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.handle_input(":", window, cx);
    });
    pull_diagnostics_handle.next().await.unwrap();
    pull_diagnostics_handle.next().await.unwrap();
    pull_diagnostics_handle.next().await.unwrap();
    assert_eq!(
        7,
        diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "Client main.rs edits should trigger diagnostics pull by both client and host and an extra pull for the client's lib.rs"
    );
    workspace_diagnostics_pulls_handle.next().await.unwrap();
    assert_eq!(
        workspace_diagnostic_start_count + 2,
        workspace_diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "After client main.rs edits, the workspace diagnostics pull should follow"
    );
    executor.run_until_parked();

    editor_a_main.update_in(cx_a, |editor, window, cx| {
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.handle_input(":", window, cx);
    });
    pull_diagnostics_handle.next().await.unwrap();
    pull_diagnostics_handle.next().await.unwrap();
    pull_diagnostics_handle.next().await.unwrap();
    assert_eq!(
        10,
        diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "Host main.rs edits should trigger another diagnostics pull by both client and host and another pull for the client's lib.rs"
    );
    workspace_diagnostics_pulls_handle.next().await.unwrap();
    assert_eq!(
        workspace_diagnostic_start_count + 3,
        workspace_diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "After host main.rs edits, the workspace diagnostics pull should follow"
    );
    executor.run_until_parked();
    let diagnostic_pulls_result_ids = diagnostics_pulls_result_ids.lock().await.len();
    let workspace_pulls_result_ids = workspace_diagnostics_pulls_result_ids.lock().await.len();
    {
        assert!(
            diagnostic_pulls_result_ids > 1,
            "Should have sent result ids when pulling diagnostics"
        );
        assert!(
            workspace_pulls_result_ids > 1,
            "Should have sent result ids when pulling workspace diagnostics"
        );
    }

    fake_language_server
        .request::<lsp::request::WorkspaceDiagnosticRefresh>((), DEFAULT_LSP_REQUEST_TIMEOUT)
        .await
        .into_response()
        .expect("workspace diagnostics refresh request failed");
    // Workspace refresh now also triggers document diagnostic pulls for all open buffers
    pull_diagnostics_handle.next().await.unwrap();
    pull_diagnostics_handle.next().await.unwrap();
    assert_eq!(
        12,
        diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "Workspace refresh should trigger document pulls for all open buffers (main.rs and lib.rs)"
    );
    workspace_diagnostics_pulls_handle.next().await.unwrap();
    assert_eq!(
        workspace_diagnostic_start_count + 4,
        workspace_diagnostics_pulls_made.load(atomic::Ordering::Acquire),
        "Another workspace diagnostics pull should happen after the diagnostics refresh server request"
    );
    {
        assert!(
            diagnostics_pulls_result_ids.lock().await.len() > diagnostic_pulls_result_ids,
            "Document diagnostic pulls should happen after workspace refresh"
        );
        assert!(
            workspace_diagnostics_pulls_result_ids.lock().await.len() > workspace_pulls_result_ids,
            "More workspace diagnostics should be pulled"
        );
    }
    editor_b_lib.update(cx_b, |editor, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let all_diagnostics = snapshot
            .diagnostics_in_range(MultiBufferOffset(0)..snapshot.len())
            .collect::<Vec<_>>();
        let expected_messages = [
            expected_workspace_pull_diagnostics_lib_message,
            expected_pull_diagnostic_lib_message,
            expected_push_diagnostic_lib_message,
        ];
        assert_eq!(all_diagnostics.len(), 2);
        for diagnostic in &all_diagnostics {
            assert!(
                expected_messages.contains(&diagnostic.diagnostic.message.as_str()),
                "Unexpected diagnostics: {all_diagnostics:?}"
            );
        }
    });
    editor_b_main.update(cx_b, |editor, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let all_diagnostics = snapshot
            .diagnostics_in_range(MultiBufferOffset(0)..snapshot.len())
            .collect::<Vec<_>>();
        assert_eq!(all_diagnostics.len(), 2);

        let expected_messages = [
            expected_workspace_pull_diagnostics_main_message,
            expected_pull_diagnostic_main_message,
            expected_push_diagnostic_main_message,
        ];
        for diagnostic in &all_diagnostics {
            assert!(
                expected_messages.contains(&diagnostic.diagnostic.message.as_str()),
                "Unexpected diagnostics: {all_diagnostics:?}"
            );
        }
    });
    editor_a_main.update(cx_a, |editor, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let all_diagnostics = snapshot
            .diagnostics_in_range(MultiBufferOffset(0)..snapshot.len())
            .collect::<Vec<_>>();
        assert_eq!(all_diagnostics.len(), 2);
        let expected_messages = [
            expected_workspace_pull_diagnostics_main_message,
            expected_pull_diagnostic_main_message,
            expected_push_diagnostic_main_message,
        ];
        for diagnostic in &all_diagnostics {
            assert!(
                expected_messages.contains(&diagnostic.diagnostic.message.as_str()),
                "Unexpected diagnostics: {all_diagnostics:?}"
            );
        }
    });
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

#[gpui::test]
async fn test_document_folding_ranges(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
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

    let capabilities = lsp::ServerCapabilities {
        folding_range_provider: Some(lsp::FoldingRangeProviderCapability::Simple(true)),
        ..lsp::ServerCapabilities::default()
    };
    client_a.language_registry().add(rust_lang());
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: capabilities.clone(),
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

    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "main.rs": "fn main() {\n    if true {\n        println!(\"hello\");\n    }\n}\n",
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

    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    active_call_b
        .update(cx_b, |call, cx| call.set_location(Some(&project_b), cx))
        .await
        .unwrap();

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);

    let _buffer_a = project_a
        .update(cx_a, |project, cx| {
            project.open_local_buffer(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let fake_language_server = fake_language_servers.next().await.unwrap();

    let folding_request_count = Arc::new(AtomicUsize::new(0));
    let closure_count = Arc::clone(&folding_request_count);
    let mut folding_request_handle = fake_language_server
        .set_request_handler::<lsp::request::FoldingRangeRequest, _, _>(move |_, _| {
            let count = Arc::clone(&closure_count);
            async move {
                count.fetch_add(1, atomic::Ordering::Release);
                Ok(Some(vec![lsp::FoldingRange {
                    start_line: 0,
                    start_character: Some(10),
                    end_line: 4,
                    end_character: Some(1),
                    kind: None,
                    collapsed_text: None,
                }]))
            }
        });

    executor.run_until_parked();

    assert_eq!(
        0,
        folding_request_count.load(atomic::Ordering::Acquire),
        "LSP folding ranges are off by default, no request should have been made"
    );
    editor_a.update(cx_a, |editor, cx| {
        assert!(
            !editor.document_folding_ranges_enabled(cx),
            "Host should not have LSP folding ranges enabled"
        );
    });

    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);
    let editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    executor.run_until_parked();

    editor_b.update(cx_b, |editor, cx| {
        assert!(
            !editor.document_folding_ranges_enabled(cx),
            "Client should not have LSP folding ranges enabled by default"
        );
    });

    cx_b.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project
                    .all_languages
                    .defaults
                    .document_folding_ranges = Some(DocumentFoldingRanges::On);
            });
        });
    });
    executor.advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT);
    folding_request_handle.next().await.unwrap();
    executor.run_until_parked();

    assert!(
        folding_request_count.load(atomic::Ordering::Acquire) > 0,
        "After the client enables LSP folding ranges, a request should be made"
    );
    editor_b.update(cx_b, |editor, cx| {
        assert!(
            editor.document_folding_ranges_enabled(cx),
            "Client should have LSP folding ranges enabled after toggling the setting on"
        );
    });
    editor_a.update(cx_a, |editor, cx| {
        assert!(
            !editor.document_folding_ranges_enabled(cx),
            "Host should remain unaffected by the client's setting change"
        );
    });

    editor_b.update_in(cx_b, |editor, window, cx| {
        let snapshot = editor.display_snapshot(cx);
        assert!(
            !snapshot.is_line_folded(MultiBufferRow(0)),
            "Line 0 should not be folded before fold_at"
        );
        editor.fold_at(MultiBufferRow(0), window, cx);
    });
    executor.run_until_parked();

    editor_b.update(cx_b, |editor, cx| {
        let snapshot = editor.display_snapshot(cx);
        assert!(
            snapshot.is_line_folded(MultiBufferRow(0)),
            "Line 0 should be folded after fold_at using LSP folding range"
        );
    });
}

#[gpui::test]
async fn test_remote_project_worktree_trust(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let has_restricted_worktrees = |project: &gpui::Entity<project::Project>,
                                    cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let worktree_store = project.read(cx).worktree_store();
            TrustedWorktrees::try_get_global(cx)
                .unwrap()
                .read(cx)
                .has_restricted_worktrees(&worktree_store, cx)
        })
    };

    cx_a.update(|cx| {
        project::trusted_worktrees::init(HashMap::default(), cx);
    });
    cx_b.update(|cx| {
        project::trusted_worktrees::init(HashMap::default(), cx);
    });

    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "file.txt": "contents",
            }),
        )
        .await;

    let (project_a, worktree_id) = client_a
        .build_local_project_with_trust(path!("/a"), cx_a)
        .await;
    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    let _editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("src/main.rs")),
                None,
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let _editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("src/main.rs")),
                None,
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    assert!(
        has_restricted_worktrees(&project_a, cx_a),
        "local client should have restricted worktrees after opening it"
    );
    assert!(
        !has_restricted_worktrees(&project_b, cx_b),
        "remote client joined a project should have no restricted worktrees"
    );

    cx_a.update(|_, cx| {
        if let Some(trusted_worktrees) = TrustedWorktrees::try_get_global(cx) {
            trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                trusted_worktrees.trust(
                    &project_a.read(cx).worktree_store(),
                    HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
                    cx,
                );
            });
        }
    });
    assert!(
        !has_restricted_worktrees(&project_a, cx_a),
        "local client should have no worktrees after trusting those"
    );
    assert!(
        !has_restricted_worktrees(&project_b, cx_b),
        "remote client should still be trusted"
    );
}

#[gpui::test]
async fn test_document_symbols(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
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

    let capabilities = lsp::ServerCapabilities {
        document_symbol_provider: Some(lsp::OneOf::Left(true)),
        ..lsp::ServerCapabilities::default()
    };
    client_a.language_registry().add(rust_lang());
    #[allow(deprecated)]
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: capabilities.clone(),
            initializer: Some(Box::new(|fake_language_server| {
                #[allow(deprecated)]
                fake_language_server
                    .set_request_handler::<lsp::request::DocumentSymbolRequest, _, _>(
                        move |_, _| async move {
                            Ok(Some(lsp::DocumentSymbolResponse::Nested(vec![
                                lsp::DocumentSymbol {
                                    name: "Foo".to_string(),
                                    detail: None,
                                    kind: lsp::SymbolKind::STRUCT,
                                    tags: None,
                                    deprecated: None,
                                    range: lsp::Range::new(
                                        lsp::Position::new(0, 0),
                                        lsp::Position::new(2, 1),
                                    ),
                                    selection_range: lsp::Range::new(
                                        lsp::Position::new(0, 7),
                                        lsp::Position::new(0, 10),
                                    ),
                                    children: Some(vec![lsp::DocumentSymbol {
                                        name: "bar".to_string(),
                                        detail: None,
                                        kind: lsp::SymbolKind::FIELD,
                                        tags: None,
                                        deprecated: None,
                                        range: lsp::Range::new(
                                            lsp::Position::new(1, 4),
                                            lsp::Position::new(1, 13),
                                        ),
                                        selection_range: lsp::Range::new(
                                            lsp::Position::new(1, 4),
                                            lsp::Position::new(1, 7),
                                        ),
                                        children: None,
                                    }]),
                                },
                            ])))
                        },
                    );
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

    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "main.rs": "struct Foo {\n    bar: u32,\n}\n",
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

    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    active_call_b
        .update(cx_b, |call, cx| call.set_location(Some(&project_b), cx))
        .await
        .unwrap();

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);

    let editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let _fake_language_server = fake_language_servers.next().await.unwrap();
    executor.run_until_parked();

    cx_a.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.document_symbols =
                    Some(DocumentSymbols::On);
            });
        });
    });
    executor.advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT + Duration::from_millis(100));
    executor.run_until_parked();

    editor_a.update(cx_a, |editor, cx| {
        let (breadcrumbs, _) = editor
            .breadcrumbs(cx)
            .expect("Host should have breadcrumbs");
        let texts: Vec<_> = breadcrumbs.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["main.rs", "struct Foo"],
            "Host should see file path and LSP symbol 'Foo' in breadcrumbs"
        );
    });

    cx_b.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.document_symbols =
                    Some(DocumentSymbols::On);
            });
        });
    });
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);
    let editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    executor.advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT + Duration::from_millis(100));
    executor.run_until_parked();

    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            editor
                .breadcrumbs(cx)
                .expect("Client B should have breadcrumbs")
                .0
                .iter()
                .map(|b| b.text.as_str())
                .collect::<Vec<_>>(),
            vec!["main.rs", "struct Foo"],
            "Client B should see file path and LSP symbol 'Foo' via remote project"
        );
    });
}
