use crate::TestServer;
use call::ActiveCall;
use collections::{HashMap, HashSet};
use editor::{Editor, LSP_REQUEST_DEBOUNCE_TIMEOUT};
use futures::StreamExt;
use gpui::{TestAppContext, UpdateGlobal, VisualTestContext};
use language::{FakeLspAdapter, rust_lang};
use multi_buffer::MultiBufferRow;
use pretty_assertions::assert_eq;
use project::trusted_worktrees::{PathTrust, TrustedWorktrees};
use serde_json::json;
use settings::{DocumentFoldingRanges, DocumentSymbols, SettingsStore};
use std::{
    sync::{
        Arc,
        atomic::{self, AtomicUsize},
    },
    time::Duration,
};
use util::{path, rel_path::rel_path};
use workspace::item::Item as _;

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
