use crate::TestServer;
use call::ActiveCall;
use collections::{HashMap, HashSet};
use editor::{
    Editor, LSP_REQUEST_DEBOUNCE_TIMEOUT, MultiBufferOffset, SelectionEffects,
    actions::{
        CopyFileLocation, CopyFileName, CopyFileNameWithoutExtension, ExpandMacroRecursively,
        MoveToEnd, SelectAll,
    },
    test::{
        editor_test_context::{AssertionContextManager, EditorTestContext},
        expand_macro_recursively,
    },
};
use fs::Fs;
use futures::{StreamExt, lock::Mutex};
use gpui::{App, TestAppContext, UpdateGlobal, VisualContext, VisualTestContext};
use indoc::indoc;
use language::{FakeLspAdapter, language_settings::LanguageSettings, rust_lang};
use lsp::DEFAULT_LSP_REQUEST_TIMEOUT;
use multi_buffer::{AnchorRangeExt as _, MultiBufferRow};
use pretty_assertions::assert_eq;
use project::{
    ProjectPath,
    lsp_store::lsp_ext_command::{ExpandedMacro, LspExtExpandMacro},
    trusted_worktrees::{PathTrust, TrustedWorktrees},
};
use serde_json::json;
use settings::{DocumentFoldingRanges, DocumentSymbols, SemanticTokens, SettingsStore};
use std::{
    collections::BTreeSet,
    num::NonZeroU32,
    ops::{Deref as _, Range},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{self, AtomicBool, AtomicUsize},
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

#[gpui::test(iterations = 30)]
async fn test_collaborating_with_editorconfig(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    cx_b.update(editor::init);

    // Set up a fake language server.
    client_a.language_registry().add(rust_lang());
    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "src": {
                    "main.rs": "mod other;\nfn main() { let foo = other::foo(); }",
                    "other_mod": {
                        "other.rs": "pub fn foo() -> usize {\n    4\n}",
                        ".editorconfig": "",
                    },
                },
                ".editorconfig": "[*]\ntab_width = 2\n",
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/a"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let main_buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/main.rs")), cx)
        })
        .await
        .unwrap();
    let other_buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/other_mod/other.rs")), cx)
        })
        .await
        .unwrap();
    let cx_a = cx_a.add_empty_window();
    let main_editor_a = cx_a.new_window_entity(|window, cx| {
        Editor::for_buffer(main_buffer_a, Some(project_a.clone()), window, cx)
    });
    let other_editor_a = cx_a.new_window_entity(|window, cx| {
        Editor::for_buffer(other_buffer_a, Some(project_a), window, cx)
    });
    let mut main_editor_cx_a = EditorTestContext {
        cx: cx_a.clone(),
        window: cx_a.window_handle(),
        editor: main_editor_a,
        assertion_cx: AssertionContextManager::new(),
    };
    let mut other_editor_cx_a = EditorTestContext {
        cx: cx_a.clone(),
        window: cx_a.window_handle(),
        editor: other_editor_a,
        assertion_cx: AssertionContextManager::new(),
    };

    // Join the project as client B.
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    let main_buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/main.rs")), cx)
        })
        .await
        .unwrap();
    let other_buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/other_mod/other.rs")), cx)
        })
        .await
        .unwrap();
    let cx_b = cx_b.add_empty_window();
    let main_editor_b = cx_b.new_window_entity(|window, cx| {
        Editor::for_buffer(main_buffer_b, Some(project_b.clone()), window, cx)
    });
    let other_editor_b = cx_b.new_window_entity(|window, cx| {
        Editor::for_buffer(other_buffer_b, Some(project_b.clone()), window, cx)
    });
    let mut main_editor_cx_b = EditorTestContext {
        cx: cx_b.clone(),
        window: cx_b.window_handle(),
        editor: main_editor_b,
        assertion_cx: AssertionContextManager::new(),
    };
    let mut other_editor_cx_b = EditorTestContext {
        cx: cx_b.clone(),
        window: cx_b.window_handle(),
        editor: other_editor_b,
        assertion_cx: AssertionContextManager::new(),
    };

    let initial_main = indoc! {"
ˇmod other;
fn main() { let foo = other::foo(); }"};
    let initial_other = indoc! {"
ˇpub fn foo() -> usize {
    4
}"};

    let first_tabbed_main = indoc! {"
  ˇmod other;
fn main() { let foo = other::foo(); }"};
    tab_undo_assert(
        &mut main_editor_cx_a,
        &mut main_editor_cx_b,
        initial_main,
        first_tabbed_main,
        true,
    );
    tab_undo_assert(
        &mut main_editor_cx_a,
        &mut main_editor_cx_b,
        initial_main,
        first_tabbed_main,
        false,
    );

    let first_tabbed_other = indoc! {"
  ˇpub fn foo() -> usize {
    4
}"};
    tab_undo_assert(
        &mut other_editor_cx_a,
        &mut other_editor_cx_b,
        initial_other,
        first_tabbed_other,
        true,
    );
    tab_undo_assert(
        &mut other_editor_cx_a,
        &mut other_editor_cx_b,
        initial_other,
        first_tabbed_other,
        false,
    );

    client_a
        .fs()
        .atomic_write(
            PathBuf::from(path!("/a/src/.editorconfig")),
            "[*]\ntab_width = 3\n".to_owned(),
        )
        .await
        .unwrap();
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let second_tabbed_main = indoc! {"
   ˇmod other;
fn main() { let foo = other::foo(); }"};
    tab_undo_assert(
        &mut main_editor_cx_a,
        &mut main_editor_cx_b,
        initial_main,
        second_tabbed_main,
        true,
    );
    tab_undo_assert(
        &mut main_editor_cx_a,
        &mut main_editor_cx_b,
        initial_main,
        second_tabbed_main,
        false,
    );

    let second_tabbed_other = indoc! {"
   ˇpub fn foo() -> usize {
    4
}"};
    tab_undo_assert(
        &mut other_editor_cx_a,
        &mut other_editor_cx_b,
        initial_other,
        second_tabbed_other,
        true,
    );
    tab_undo_assert(
        &mut other_editor_cx_a,
        &mut other_editor_cx_b,
        initial_other,
        second_tabbed_other,
        false,
    );

    let editorconfig_buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/other_mod/.editorconfig")), cx)
        })
        .await
        .unwrap();
    editorconfig_buffer_b.update(cx_b, |buffer, cx| {
        buffer.set_text("[*.rs]\ntab_width = 6\n", cx);
    });
    project_b
        .update(cx_b, |project, cx| {
            project.save_buffer(editorconfig_buffer_b.clone(), cx)
        })
        .await
        .unwrap();
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    tab_undo_assert(
        &mut main_editor_cx_a,
        &mut main_editor_cx_b,
        initial_main,
        second_tabbed_main,
        true,
    );
    tab_undo_assert(
        &mut main_editor_cx_a,
        &mut main_editor_cx_b,
        initial_main,
        second_tabbed_main,
        false,
    );

    let third_tabbed_other = indoc! {"
      ˇpub fn foo() -> usize {
    4
}"};
    tab_undo_assert(
        &mut other_editor_cx_a,
        &mut other_editor_cx_b,
        initial_other,
        third_tabbed_other,
        true,
    );

    tab_undo_assert(
        &mut other_editor_cx_a,
        &mut other_editor_cx_b,
        initial_other,
        third_tabbed_other,
        false,
    );
}

#[gpui::test(iterations = 10)]
async fn test_collaborating_with_external_editorconfig(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a.language_registry().add(rust_lang());
    client_b.language_registry().add(rust_lang());

    // Set up external .editorconfig in parent directory
    client_a
        .fs()
        .insert_tree(
            path!("/parent"),
            json!({
                ".editorconfig": "[*]\nindent_size = 5\n",
                "worktree": {
                    ".editorconfig": "[*]\n",
                    "src": {
                        "main.rs": "fn main() {}",
                    },
                },
            }),
        )
        .await;

    let (project_a, worktree_id) = client_a
        .build_local_project(path!("/parent/worktree"), cx_a)
        .await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    project_a.update(cx_a, |project, _| project.languages().add(rust_lang()));

    // Open buffer on client A
    let buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/main.rs")), cx)
        })
        .await
        .unwrap();

    cx_a.run_until_parked();

    // Verify client A sees external editorconfig settings
    cx_a.read(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer_a.read(cx), cx);
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(5));
    });

    // Client B joins the project
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    project_b.update(cx_b, |project, _| project.languages().add(rust_lang()));
    let buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/main.rs")), cx)
        })
        .await
        .unwrap();

    cx_b.run_until_parked();

    // Verify client B also sees external editorconfig settings
    cx_b.read(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer_b.read(cx), cx);
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(5));
    });

    // Client A modifies the external .editorconfig
    client_a
        .fs()
        .atomic_write(
            PathBuf::from(path!("/parent/.editorconfig")),
            "[*]\nindent_size = 9\n".to_owned(),
        )
        .await
        .unwrap();

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    // Verify client A sees updated settings
    cx_a.read(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer_a.read(cx), cx);
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(9));
    });

    // Verify client B also sees updated settings
    cx_b.read(|cx| {
        let settings = LanguageSettings::for_buffer(&buffer_b.read(cx), cx);
        assert_eq!(Some(settings.tab_size), NonZeroU32::new(9));
    });
}

#[gpui::test]
async fn test_add_breakpoints(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let executor = cx_a.executor();
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);
    cx_a.update(editor::init);
    cx_b.update(editor::init);
    client_a
        .fs()
        .insert_tree(
            "/a",
            json!({
                "test.txt": "one\ntwo\nthree\nfour\nfive",
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project("/a", cx_a).await;
    let project_path = ProjectPath {
        worktree_id,
        path: rel_path(&"test.txt").into(),
    };
    let abs_path = project_a.read_with(cx_a, |project, cx| {
        project
            .absolute_path(&project_path, cx)
            .map(Arc::from)
            .unwrap()
    });

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
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    // Client A opens an editor.
    let editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path(project_path.clone(), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    // Client B opens same editor as A.
    let editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path(project_path.clone(), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    // Client A adds breakpoint on line (1)
    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.toggle_breakpoint(&editor::actions::ToggleBreakpoint, window, cx);
    });

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let breakpoints_a = editor_a.update(cx_a, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });
    let breakpoints_b = editor_b.update(cx_b, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(1, breakpoints_a.len());
    assert_eq!(1, breakpoints_a.get(&abs_path).unwrap().len());
    assert_eq!(breakpoints_a, breakpoints_b);

    // Client B adds breakpoint on line(2)
    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.toggle_breakpoint(&editor::actions::ToggleBreakpoint, window, cx);
    });

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let breakpoints_a = editor_a.update(cx_a, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });
    let breakpoints_b = editor_b.update(cx_b, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(1, breakpoints_a.len());
    assert_eq!(breakpoints_a, breakpoints_b);
    assert_eq!(2, breakpoints_a.get(&abs_path).unwrap().len());

    // Client A removes last added breakpoint from client B
    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.toggle_breakpoint(&editor::actions::ToggleBreakpoint, window, cx);
    });

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let breakpoints_a = editor_a.update(cx_a, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });
    let breakpoints_b = editor_b.update(cx_b, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(1, breakpoints_a.len());
    assert_eq!(breakpoints_a, breakpoints_b);
    assert_eq!(1, breakpoints_a.get(&abs_path).unwrap().len());

    // Client B removes first added breakpoint by client A
    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.move_up(&mav_actions::editor::MoveUp, window, cx);
        editor.move_up(&mav_actions::editor::MoveUp, window, cx);
        editor.toggle_breakpoint(&editor::actions::ToggleBreakpoint, window, cx);
    });

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let breakpoints_a = editor_a.update(cx_a, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });
    let breakpoints_b = editor_b.update(cx_b, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(0, breakpoints_a.len());
    assert_eq!(breakpoints_a, breakpoints_b);
}

#[gpui::test]
async fn test_client_can_query_lsp_ext(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    cx_a.update(editor::init);
    cx_b.update(editor::init);

    client_a.language_registry().add(rust_lang());
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "rust-analyzer",
            ..FakeLspAdapter::default()
        },
    );
    client_b.language_registry().add(rust_lang());
    client_b.language_registry().register_fake_lsp_adapter(
        "Rust",
        FakeLspAdapter {
            name: "rust-analyzer",
            ..FakeLspAdapter::default()
        },
    );

    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "main.rs": "fn main() {}",
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
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    let editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let fake_language_server = fake_language_servers.next().await.unwrap();

    // host
    let mut expand_request_a = fake_language_server.set_request_handler::<LspExtExpandMacro, _, _>(
        |params, _| async move {
            assert_eq!(
                params.text_document.uri,
                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
            );
            assert_eq!(params.position, lsp::Position::new(0, 0));
            Ok(Some(ExpandedMacro {
                name: "test_macro_name".to_string(),
                expansion: "test_macro_expansion on the host".to_string(),
            }))
        },
    );

    editor_a.update_in(cx_a, |editor, window, cx| {
        expand_macro_recursively(editor, &ExpandMacroRecursively, window, cx)
    });
    expand_request_a.next().await.unwrap();
    cx_a.run_until_parked();

    workspace_a.update(cx_a, |workspace, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            assert_eq!(
                pane.items_len(),
                2,
                "Should have added a macro expansion to the host's pane"
            );
            let new_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
            new_editor.update(cx, |editor, cx| {
                assert_eq!(editor.text(cx), "test_macro_expansion on the host");
            });
        })
    });

    // client
    let mut expand_request_b = fake_language_server.set_request_handler::<LspExtExpandMacro, _, _>(
        |params, _| async move {
            assert_eq!(
                params.text_document.uri,
                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
            );
            assert_eq!(
                params.position,
                lsp::Position::new(0, 12),
                "editor_b has selected the entire text and should query for a different position"
            );
            Ok(Some(ExpandedMacro {
                name: "test_macro_name".to_string(),
                expansion: "test_macro_expansion on the client".to_string(),
            }))
        },
    );

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.select_all(&SelectAll, window, cx);
        expand_macro_recursively(editor, &ExpandMacroRecursively, window, cx)
    });
    expand_request_b.next().await.unwrap();
    cx_b.run_until_parked();

    workspace_b.update(cx_b, |workspace, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            assert_eq!(
                pane.items_len(),
                2,
                "Should have added a macro expansion to the client's pane"
            );
            let new_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
            new_editor.update(cx, |editor, cx| {
                assert_eq!(editor.text(cx), "test_macro_expansion on the client");
            });
        })
    });
}

#[gpui::test]
async fn test_copy_file_name_without_extension(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    cx_b.update(editor::init);

    client_a
        .fs()
        .insert_tree(
            path!("/root"),
            json!({
                "src": {
                    "main.rs": indoc! {"
                        fn main() {
                            println!(\"Hello, world!\");
                        }
                    "},
                }
            }),
        )
        .await;

    let (project_a, worktree_id) = client_a.build_local_project(path!("/root"), cx_a).await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    let editor_a = workspace_a
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

    let editor_b = workspace_b
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

    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.copy_file_name_without_extension(&CopyFileNameWithoutExtension, window, cx);
    });

    assert_eq!(
        cx_a.read_from_clipboard().and_then(|item| item.text()),
        Some("main".to_string())
    );

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.copy_file_name_without_extension(&CopyFileNameWithoutExtension, window, cx);
    });

    assert_eq!(
        cx_b.read_from_clipboard().and_then(|item| item.text()),
        Some("main".to_string())
    );
}

#[gpui::test]
async fn test_copy_file_name(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    cx_b.update(editor::init);

    client_a
        .fs()
        .insert_tree(
            path!("/root"),
            json!({
                "src": {
                    "main.rs": indoc! {"
                        fn main() {
                            println!(\"Hello, world!\");
                        }
                    "},
                }
            }),
        )
        .await;

    let (project_a, worktree_id) = client_a.build_local_project(path!("/root"), cx_a).await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    let editor_a = workspace_a
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

    let editor_b = workspace_b
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

    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.copy_file_name(&CopyFileName, window, cx);
    });

    assert_eq!(
        cx_a.read_from_clipboard().and_then(|item| item.text()),
        Some("main.rs".to_string())
    );

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.copy_file_name(&CopyFileName, window, cx);
    });

    assert_eq!(
        cx_b.read_from_clipboard().and_then(|item| item.text()),
        Some("main.rs".to_string())
    );
}

#[gpui::test]
async fn test_copy_file_location(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    cx_b.update(editor::init);

    client_a
        .fs()
        .insert_tree(
            path!("/root"),
            json!({
                "src": {
                    "main.rs": indoc! {"
                        fn main() {
                            println!(\"Hello, world!\");
                        }
                    "},
                }
            }),
        )
        .await;

    let (project_a, worktree_id) = client_a.build_local_project(path!("/root"), cx_a).await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    let editor_a = workspace_a
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

    let editor_b = workspace_b
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

    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(16)..MultiBufferOffset(16)]);
        });
        editor.copy_file_location(&CopyFileLocation, window, cx);
    });

    assert_eq!(
        cx_a.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("{}:2", path!("src/main.rs")))
    );

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(16)..MultiBufferOffset(16)]);
        });
        editor.copy_file_location(&CopyFileLocation, window, cx);
    });

    assert_eq!(
        cx_b.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("{}:2", path!("src/main.rs")))
    );

    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(16)..MultiBufferOffset(44)]);
        });
        editor.copy_file_location(&CopyFileLocation, window, cx);
    });

    assert_eq!(
        cx_a.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("{}:2-3", path!("src/main.rs")))
    );

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(16)..MultiBufferOffset(44)]);
        });
        editor.copy_file_location(&CopyFileLocation, window, cx);
    });

    assert_eq!(
        cx_b.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("{}:2-3", path!("src/main.rs")))
    );

    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(16)..MultiBufferOffset(43)]);
        });
        editor.copy_file_location(&CopyFileLocation, window, cx);
    });

    assert_eq!(
        cx_a.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("{}:2", path!("src/main.rs")))
    );

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(16)..MultiBufferOffset(43)]);
        });
        editor.copy_file_location(&CopyFileLocation, window, cx);
    });

    assert_eq!(
        cx_b.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("{}:2", path!("src/main.rs")))
    );
}

#[track_caller]
fn tab_undo_assert(
    cx_a: &mut EditorTestContext,
    cx_b: &mut EditorTestContext,
    expected_initial: &str,
    expected_tabbed: &str,
    a_tabs: bool,
) {
    cx_a.assert_editor_state(expected_initial);
    cx_b.assert_editor_state(expected_initial);

    if a_tabs {
        cx_a.update_editor(|editor, window, cx| {
            editor.tab(&editor::actions::Tab, window, cx);
        });
    } else {
        cx_b.update_editor(|editor, window, cx| {
            editor.tab(&editor::actions::Tab, window, cx);
        });
    }

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    cx_a.assert_editor_state(expected_tabbed);
    cx_b.assert_editor_state(expected_tabbed);

    if a_tabs {
        cx_a.update_editor(|editor, window, cx| {
            editor.undo(&editor::actions::Undo, window, cx);
        });
    } else {
        cx_b.update_editor(|editor, window, cx| {
            editor.undo(&editor::actions::Undo, window, cx);
        });
    }
    cx_a.run_until_parked();
    cx_b.run_until_parked();
    cx_a.assert_editor_state(expected_initial);
    cx_b.assert_editor_state(expected_initial);
}

fn extract_semantic_token_ranges(editor: &Editor, cx: &App) -> Vec<Range<MultiBufferOffset>> {
    let multi_buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
    editor
        .display_map
        .read(cx)
        .semantic_token_highlights
        .iter()
        .flat_map(|(_, (v, _))| v.iter())
        .map(|highlights| highlights.range.to_offset(&multi_buffer_snapshot))
        .collect()
}

#[gpui::test(iterations = 10)]
async fn test_mutual_editor_semantic_token_cache_update(
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

    cx_a.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.semantic_tokens =
                    Some(SemanticTokens::Full);
            });
        });
    });
    cx_b.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.semantic_tokens =
                    Some(SemanticTokens::Full);
            });
        });
    });

    let capabilities = lsp::ServerCapabilities {
        semantic_tokens_provider: Some(
            lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(
                lsp::SemanticTokensOptions {
                    legend: lsp::SemanticTokensLegend {
                        token_types: vec!["function".into()],
                        token_modifiers: vec![],
                    },
                    full: Some(lsp::SemanticTokensFullOptions::Delta { delta: None }),
                    ..Default::default()
                },
            ),
        ),
        ..lsp::ServerCapabilities::default()
    };
    client_a.language_registry().add(rust_lang());

    let edits_made = Arc::new(AtomicUsize::new(0));
    let closure_edits_made = Arc::clone(&edits_made);
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: capabilities.clone(),
            initializer: Some(Box::new(move |fake_language_server| {
                let closure_edits_made = closure_edits_made.clone();
                fake_language_server
                    .set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(
                        move |_, _| {
                            let edits_made_2 = Arc::clone(&closure_edits_made);
                            async move {
                                let edits_made =
                                    AtomicUsize::load(&edits_made_2, atomic::Ordering::Acquire);
                                Ok(Some(lsp::SemanticTokensResult::Tokens(
                                    lsp::SemanticTokens {
                                        data: vec![
                                            0,                     // delta_line
                                            3,                     // delta_start
                                            edits_made as u32 + 4, // length
                                            0,                     // token_type
                                            0,                     // token_modifiers_bitset
                                        ],
                                        result_id: None,
                                    },
                                )))
                            }
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
                "main.rs": "fn main() { a }",
                "other.rs": "// Test file",
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

    let file_a = workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
    });
    let _fake_language_server = fake_language_servers.next().await.unwrap();
    let editor_a = file_a.await.unwrap().downcast::<Editor>().unwrap();
    executor.advance_clock(Duration::from_millis(100));
    executor.run_until_parked();

    let initial_edit = edits_made.load(atomic::Ordering::Acquire);
    editor_a.update(cx_a, |editor, cx| {
        let ranges = extract_semantic_token_ranges(editor, cx);
        assert_eq!(
            ranges,
            vec![MultiBufferOffset(3)..MultiBufferOffset(3 + initial_edit + 4)],
            "Host should get its first semantic tokens when opening an editor"
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

    executor.advance_clock(Duration::from_millis(100));
    executor.run_until_parked();
    editor_b.update(cx_b, |editor, cx| {
        let ranges = extract_semantic_token_ranges(editor, cx);
        assert_eq!(
            ranges,
            vec![MultiBufferOffset(3)..MultiBufferOffset(3 + initial_edit + 4)],
            "Client should get its first semantic tokens when opening an editor"
        );
    });

    let after_client_edit = edits_made.fetch_add(1, atomic::Ordering::Release) + 1;
    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(13)..MultiBufferOffset(13)].clone())
        });
        editor.handle_input(":", window, cx);
    });
    cx_b.focus(&editor_b);

    executor.advance_clock(Duration::from_secs(1));
    executor.run_until_parked();
    editor_a.update(cx_a, |editor, cx| {
        let ranges = extract_semantic_token_ranges(editor, cx);
        assert_eq!(
            ranges,
            vec![MultiBufferOffset(3)..MultiBufferOffset(3 + after_client_edit + 4)],
        );
    });
    editor_b.update(cx_b, |editor, cx| {
        let ranges = extract_semantic_token_ranges(editor, cx);
        assert_eq!(
            ranges,
            vec![MultiBufferOffset(3)..MultiBufferOffset(3 + after_client_edit + 4)],
        );
    });

    let after_host_edit = edits_made.fetch_add(1, atomic::Ordering::Release) + 1;
    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(14)..MultiBufferOffset(14)])
        });
        editor.handle_input("a change", window, cx);
    });
    cx_a.focus(&editor_a);

    executor.advance_clock(Duration::from_secs(1));
    executor.run_until_parked();
    editor_a.update(cx_a, |editor, cx| {
        let ranges = extract_semantic_token_ranges(editor, cx);
        assert_eq!(
            ranges,
            vec![MultiBufferOffset(3)..MultiBufferOffset(3 + after_host_edit + 4)],
        );
    });
    editor_b.update(cx_b, |editor, cx| {
        let ranges = extract_semantic_token_ranges(editor, cx);
        assert_eq!(
            ranges,
            vec![MultiBufferOffset(3)..MultiBufferOffset(3 + after_host_edit + 4)],
        );
    });
}

#[gpui::test(iterations = 10)]
async fn test_semantic_token_refresh_is_forwarded(
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

    cx_a.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.semantic_tokens = Some(SemanticTokens::Off);
            });
        });
    });
    cx_b.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.semantic_tokens =
                    Some(SemanticTokens::Full);
            });
        });
    });

    let capabilities = lsp::ServerCapabilities {
        semantic_tokens_provider: Some(
            lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(
                lsp::SemanticTokensOptions {
                    legend: lsp::SemanticTokensLegend {
                        token_types: vec!["function".into()],
                        token_modifiers: vec![],
                    },
                    full: Some(lsp::SemanticTokensFullOptions::Delta { delta: None }),
                    ..Default::default()
                },
            ),
        ),
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
                "main.rs": "fn main() { a }",
                "other.rs": "// Test file",
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
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    let editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let other_tokens = Arc::new(AtomicBool::new(false));
    let fake_language_server = fake_language_servers.next().await.unwrap();
    let closure_other_tokens = Arc::clone(&other_tokens);
    fake_language_server
        .set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(move |params, _| {
            let task_other_tokens = Arc::clone(&closure_other_tokens);
            async move {
                assert_eq!(
                    params.text_document.uri,
                    lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                );
                let other_tokens = task_other_tokens.load(atomic::Ordering::Acquire);
                let (delta_start, length) = if other_tokens { (0, 2) } else { (3, 4) };
                Ok(Some(lsp::SemanticTokensResult::Tokens(
                    lsp::SemanticTokens {
                        data: vec![
                            0, // delta_line
                            delta_start,
                            length,
                            0, // token_type
                            0, // token_modifiers_bitset
                        ],
                        result_id: None,
                    },
                )))
            }
        })
        .next()
        .await
        .unwrap();

    executor.run_until_parked();
    editor_a.update(cx_a, |editor, cx| {
        assert!(
            extract_semantic_token_ranges(editor, cx).is_empty(),
            "Host should get no semantic tokens due to them turned off"
        );
    });

    executor.run_until_parked();
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            vec![MultiBufferOffset(3)..MultiBufferOffset(7)],
            extract_semantic_token_ranges(editor, cx),
            "Client should get its first semantic tokens when opening an editor"
        );
    });

    other_tokens.fetch_or(true, atomic::Ordering::Release);
    fake_language_server
        .request::<lsp::request::SemanticTokensRefresh>((), DEFAULT_LSP_REQUEST_TIMEOUT)
        .await
        .into_response()
        .expect("semantic tokens refresh request failed");
    // wait out the debounce timeout
    executor.advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT);
    executor.run_until_parked();
    editor_a.update(cx_a, |editor, cx| {
        assert!(
            extract_semantic_token_ranges(editor, cx).is_empty(),
            "Host should get no semantic tokens due to them turned off, even after the /refresh"
        );
    });

    executor.run_until_parked();
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            vec![MultiBufferOffset(0)..MultiBufferOffset(2)],
            extract_semantic_token_ranges(editor, cx),
            "Guest should get a /refresh LSP request propagated by host despite host tokens are off"
        );
    });
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
