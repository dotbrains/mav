use crate::TestServer;
use call::ActiveCall;
use editor::{Editor, LSP_REQUEST_DEBOUNCE_TIMEOUT, MultiBufferOffset, SelectionEffects};
use futures::StreamExt;
use gpui::{App, TestAppContext, UpdateGlobal, VisualContext};
use language::{FakeLspAdapter, rust_lang};
use lsp::DEFAULT_LSP_REQUEST_TIMEOUT;
use multi_buffer::AnchorRangeExt as _;
use pretty_assertions::assert_eq;
use serde_json::json;
use settings::{SemanticTokens, SettingsStore};
use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{self, AtomicBool, AtomicUsize},
    },
    time::Duration,
};
use util::{path, rel_path::rel_path};

#[track_caller]

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
