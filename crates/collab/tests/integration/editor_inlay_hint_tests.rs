use crate::TestServer;
use call::ActiveCall;
use editor::{Editor, MultiBufferOffset, SelectionEffects};
use futures::StreamExt;
use gpui::{App, TestAppContext, UpdateGlobal, VisualContext};
use language::{FakeLspAdapter, rust_lang};
use lsp::DEFAULT_LSP_REQUEST_TIMEOUT;
use pretty_assertions::assert_eq;
use serde_json::json;
use settings::{InlayHintSettingsContent, SettingsStore};
use std::{
    sync::{
        Arc,
        atomic::{self, AtomicBool, AtomicUsize},
    },
    time::Duration,
};
use util::{path, rel_path::rel_path};

#[gpui::test(iterations = 10)]
async fn test_mutual_editor_inlay_hint_cache_update(
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
                settings.project.all_languages.defaults.inlay_hints =
                    Some(InlayHintSettingsContent {
                        enabled: Some(true),
                        ..InlayHintSettingsContent::default()
                    })
            });
        });
    });
    cx_b.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.inlay_hints =
                    Some(InlayHintSettingsContent {
                        enabled: Some(true),
                        ..InlayHintSettingsContent::default()
                    })
            });
        });
    });

    let capabilities = lsp::ServerCapabilities {
        inlay_hint_provider: Some(lsp::OneOf::Left(true)),
        ..lsp::ServerCapabilities::default()
    };
    client_a.language_registry().add(rust_lang());

    // Set up the language server to return an additional inlay hint on each request.
    let edits_made = Arc::new(AtomicUsize::new(0));
    let closure_edits_made = Arc::clone(&edits_made);
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: capabilities.clone(),
            initializer: Some(Box::new(move |fake_language_server| {
                let closure_edits_made = closure_edits_made.clone();
                fake_language_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                    move |params, _| {
                        let edits_made_2 = Arc::clone(&closure_edits_made);
                        async move {
                            assert_eq!(
                                params.text_document.uri,
                                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                            );
                            let edits_made =
                                AtomicUsize::load(&edits_made_2, atomic::Ordering::Acquire);
                            Ok(Some(vec![lsp::InlayHint {
                                position: lsp::Position::new(0, edits_made as u32),
                                label: lsp::InlayHintLabel::String(edits_made.to_string()),
                                kind: None,
                                text_edits: None,
                                tooltip: None,
                                padding_left: None,
                                padding_right: None,
                                data: None,
                            }]))
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

    // Client A opens a project.
    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "main.rs": "fn main() { a } // and some long comment to ensure inlay hints are not trimmed out",
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

    // Client B joins the project
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    active_call_b
        .update(cx_b, |call, cx| call.set_location(Some(&project_b), cx))
        .await
        .unwrap();

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);

    // The host opens a rust file.
    let file_a = workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
    });
    let fake_language_server = fake_language_servers.next().await.unwrap();
    let editor_a = file_a.await.unwrap().downcast::<Editor>().unwrap();
    executor.advance_clock(Duration::from_millis(100));
    executor.run_until_parked();

    let initial_edit = edits_made.load(atomic::Ordering::Acquire);
    editor_a.update(cx_a, |editor, cx| {
        assert_eq!(
            vec![initial_edit.to_string()],
            extract_hint_labels(editor, cx),
            "Host should get its first hints when opens an editor"
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
        assert_eq!(
            vec![initial_edit.to_string()],
            extract_hint_labels(editor, cx),
            "Client should get its first hints when opens an editor"
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
        assert_eq!(
            vec![after_client_edit.to_string()],
            extract_hint_labels(editor, cx),
        );
    });
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            vec![after_client_edit.to_string()],
            extract_hint_labels(editor, cx),
        );
    });

    let after_host_edit = edits_made.fetch_add(1, atomic::Ordering::Release) + 1;
    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(13)..MultiBufferOffset(13)])
        });
        editor.handle_input("a change to increment both buffers' versions", window, cx);
    });
    cx_a.focus(&editor_a);

    executor.advance_clock(Duration::from_secs(1));
    executor.run_until_parked();
    editor_a.update(cx_a, |editor, cx| {
        assert_eq!(
            vec![after_host_edit.to_string()],
            extract_hint_labels(editor, cx),
        );
    });
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            vec![after_host_edit.to_string()],
            extract_hint_labels(editor, cx),
        );
    });

    let after_special_edit_for_refresh = edits_made.fetch_add(1, atomic::Ordering::Release) + 1;
    fake_language_server
        .request::<lsp::request::InlayHintRefreshRequest>((), DEFAULT_LSP_REQUEST_TIMEOUT)
        .await
        .into_response()
        .expect("inlay refresh request failed");

    executor.advance_clock(Duration::from_secs(1));
    executor.run_until_parked();
    editor_a.update(cx_a, |editor, cx| {
        assert_eq!(
            vec![after_special_edit_for_refresh.to_string()],
            extract_hint_labels(editor, cx),
            "Host should react to /refresh LSP request"
        );
    });
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            vec![after_special_edit_for_refresh.to_string()],
            extract_hint_labels(editor, cx),
            "Guest should get a /refresh LSP request propagated by host"
        );
    });
}

#[gpui::test(iterations = 10)]
async fn test_inlay_hint_refresh_is_forwarded(
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
                settings.project.all_languages.defaults.inlay_hints =
                    Some(InlayHintSettingsContent {
                        show_value_hints: Some(true),
                        enabled: Some(false),
                        edit_debounce_ms: Some(0),
                        scroll_debounce_ms: Some(0),
                        show_type_hints: Some(false),
                        show_parameter_hints: Some(false),
                        show_other_hints: Some(false),
                        show_background: Some(false),
                        toggle_on_modifiers_press: None,
                    })
            });
        });
    });
    cx_b.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.inlay_hints =
                    Some(InlayHintSettingsContent {
                        show_value_hints: Some(true),
                        enabled: Some(true),
                        edit_debounce_ms: Some(0),
                        scroll_debounce_ms: Some(0),
                        show_type_hints: Some(true),
                        show_parameter_hints: Some(true),
                        show_other_hints: Some(true),
                        show_background: Some(false),
                        toggle_on_modifiers_press: None,
                    })
            });
        });
    });

    let capabilities = lsp::ServerCapabilities {
        inlay_hint_provider: Some(lsp::OneOf::Left(true)),
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
                "main.rs": "fn main() { a } // and some long comment to ensure inlay hints are not trimmed out",
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

    let other_hints = Arc::new(AtomicBool::new(false));
    let fake_language_server = fake_language_servers.next().await.unwrap();
    let closure_other_hints = Arc::clone(&other_hints);
    fake_language_server
        .set_request_handler::<lsp::request::InlayHintRequest, _, _>(move |params, _| {
            let task_other_hints = Arc::clone(&closure_other_hints);
            async move {
                assert_eq!(
                    params.text_document.uri,
                    lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                );
                let other_hints = task_other_hints.load(atomic::Ordering::Acquire);
                let character = if other_hints { 0 } else { 2 };
                let label = if other_hints {
                    "other hint"
                } else {
                    "initial hint"
                };
                Ok(Some(vec![
                    lsp::InlayHint {
                        position: lsp::Position::new(0, character),
                        label: lsp::InlayHintLabel::String(label.to_string()),
                        kind: None,
                        text_edits: None,
                        tooltip: None,
                        padding_left: None,
                        padding_right: None,
                        data: None,
                    },
                    lsp::InlayHint {
                        position: lsp::Position::new(1090, 1090),
                        label: lsp::InlayHintLabel::String("out-of-bounds hint".to_string()),
                        kind: None,
                        text_edits: None,
                        tooltip: None,
                        padding_left: None,
                        padding_right: None,
                        data: None,
                    },
                ]))
            }
        })
        .next()
        .await
        .unwrap();

    executor.run_until_parked();
    editor_a.update(cx_a, |editor, cx| {
        assert!(
            extract_hint_labels(editor, cx).is_empty(),
            "Host should get no hints due to them turned off"
        );
    });

    executor.run_until_parked();
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            vec!["initial hint".to_string()],
            extract_hint_labels(editor, cx),
            "Client should get its first hints when opens an editor"
        );
    });

    other_hints.fetch_or(true, atomic::Ordering::Release);
    fake_language_server
        .request::<lsp::request::InlayHintRefreshRequest>((), DEFAULT_LSP_REQUEST_TIMEOUT)
        .await
        .into_response()
        .expect("inlay refresh request failed");
    executor.run_until_parked();
    editor_a.update(cx_a, |editor, cx| {
        assert!(
            extract_hint_labels(editor, cx).is_empty(),
            "Host should get no hints due to them turned off, even after the /refresh"
        );
    });

    executor.run_until_parked();
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            vec!["other hint".to_string()],
            extract_hint_labels(editor, cx),
            "Guest should get a /refresh LSP request propagated by host despite host hints are off"
        );
    });
}

fn extract_hint_labels(editor: &Editor, cx: &mut App) -> Vec<String> {
    let lsp_store = editor.project().unwrap().read(cx).lsp_store();

    let mut all_cached_labels = Vec::new();
    let mut all_fetched_hints = Vec::new();
    for buffer in editor.buffer().read(cx).all_buffers() {
        lsp_store.update(cx, |lsp_store, cx| {
            let hints = &lsp_store.latest_lsp_data(&buffer, cx).inlay_hints();
            all_cached_labels.extend(hints.all_cached_hints().into_iter().map(|hint| {
                let mut label = hint.text().to_string();
                if hint.padding_left {
                    label.insert(0, ' ');
                }
                if hint.padding_right {
                    label.push_str(" ");
                }
                label
            }));
            all_fetched_hints.extend(hints.all_fetched_hints());
        });
    }

    assert!(
        all_fetched_hints.is_empty(),
        "Did not expect background hints fetch tasks, but got {} of them",
        all_fetched_hints.len()
    );

    all_cached_labels
}
