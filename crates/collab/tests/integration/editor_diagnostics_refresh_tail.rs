{
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
