use crate::TestServer;
use call::ActiveCall;
use editor::{
    Editor, MultiBufferOffset, SelectionEffects, actions::ToggleCodeActions,
    code_context_menus::CodeContextMenu,
};
use futures::{SinkExt, StreamExt, channel::mpsc};
use gpui::{TestAppContext, UpdateGlobal};
use language::{FakeLspAdapter, rust_lang};
use lsp::DEFAULT_LSP_REQUEST_TIMEOUT;
use pretty_assertions::assert_eq;
use serde_json::json;
use settings::SettingsStore;
use std::sync::{
    Arc,
    atomic::{self, AtomicUsize},
};
use text::Point;
use util::{path, rel_path::rel_path, uri};

#[gpui::test]
async fn test_slow_lsp_server(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    cx_b.update(editor::init);
    cx_b.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.code_lens = Some(settings::CodeLens::Menu);
            });
        });
    });

    let command_name = "test_command";
    let capabilities = lsp::ServerCapabilities {
        code_lens_provider: Some(lsp::CodeLensOptions {
            resolve_provider: None,
        }),
        execute_command_provider: Some(lsp::ExecuteCommandOptions {
            commands: vec![command_name.to_string()],
            ..lsp::ExecuteCommandOptions::default()
        }),
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
            path!("/dir"),
            json!({
                "one.rs": "const ONE: usize = 1;"
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/dir"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);
    let editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("one.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    let (lsp_store_b, buffer_b) = editor_b.update(cx_b, |editor, cx| {
        let lsp_store = editor.project().unwrap().read(cx).lsp_store();
        let buffer = editor.buffer().read(cx).as_singleton().unwrap();
        (lsp_store, buffer)
    });
    let fake_language_server = fake_language_servers.next().await.unwrap();
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let long_request_time = DEFAULT_LSP_REQUEST_TIMEOUT / 2;
    let (request_started_tx, mut request_started_rx) = mpsc::unbounded();
    let requests_started = Arc::new(AtomicUsize::new(0));
    let requests_completed = Arc::new(AtomicUsize::new(0));
    let _lens_requests = fake_language_server
        .set_request_handler::<lsp::request::CodeLensRequest, _, _>({
            let request_started_tx = request_started_tx.clone();
            let requests_started = requests_started.clone();
            let requests_completed = requests_completed.clone();
            move |params, cx| {
                let mut request_started_tx = request_started_tx.clone();
                let requests_started = requests_started.clone();
                let requests_completed = requests_completed.clone();
                async move {
                    assert_eq!(
                        params.text_document.uri.as_str(),
                        uri!("file:///dir/one.rs")
                    );
                    requests_started.fetch_add(1, atomic::Ordering::Release);
                    request_started_tx.send(()).await.unwrap();
                    cx.background_executor().timer(long_request_time).await;
                    let i = requests_completed.fetch_add(1, atomic::Ordering::Release) + 1;
                    Ok(Some(vec![lsp::CodeLens {
                        range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 9)),
                        command: Some(lsp::Command {
                            title: format!("LSP Command {i}"),
                            command: command_name.to_string(),
                            arguments: None,
                        }),
                        data: None,
                    }]))
                }
            }
        });

    // Move cursor to a location, this should trigger the code lens call.
    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(7)..MultiBufferOffset(7)])
        });
    });
    let () = request_started_rx.next().await.unwrap();
    assert_eq!(
        requests_started.load(atomic::Ordering::Acquire),
        1,
        "Selection change should have initiated the first request"
    );
    assert_eq!(
        requests_completed.load(atomic::Ordering::Acquire),
        0,
        "Slow requests should be running still"
    );
    let _first_task = lsp_store_b.update(cx_b, |lsp_store, cx| {
        lsp_store
            .forget_code_lens_task(buffer_b.read(cx).remote_id())
            .expect("Should have the fetch task started")
    });

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(1)..MultiBufferOffset(1)])
        });
    });
    let () = request_started_rx.next().await.unwrap();
    assert_eq!(
        requests_started.load(atomic::Ordering::Acquire),
        2,
        "Selection change should have initiated the second request"
    );
    assert_eq!(
        requests_completed.load(atomic::Ordering::Acquire),
        0,
        "Slow requests should be running still"
    );
    let _second_task = lsp_store_b.update(cx_b, |lsp_store, cx| {
        lsp_store
            .forget_code_lens_task(buffer_b.read(cx).remote_id())
            .expect("Should have the fetch task started for the 2nd time")
    });

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(2)..MultiBufferOffset(2)])
        });
    });
    let () = request_started_rx.next().await.unwrap();
    assert_eq!(
        requests_started.load(atomic::Ordering::Acquire),
        3,
        "Selection change should have initiated the third request"
    );
    assert_eq!(
        requests_completed.load(atomic::Ordering::Acquire),
        0,
        "Slow requests should be running still"
    );

    _first_task.await.unwrap();
    _second_task.await.unwrap();
    cx_b.run_until_parked();
    assert_eq!(
        requests_started.load(atomic::Ordering::Acquire),
        3,
        "No selection changes should trigger no more code lens requests"
    );
    assert_eq!(
        requests_completed.load(atomic::Ordering::Acquire),
        1,
        "After enough time, a single, deduplicated, LSP request should have been served by the language server"
    );
    let resulting_lens_actions = editor_b
        .update(cx_b, |editor, cx| {
            let lsp_store = editor.project().unwrap().read(cx).lsp_store();
            lsp_store.update(cx, |lsp_store, cx| {
                lsp_store.code_lens_actions(&buffer_b, cx)
            })
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        resulting_lens_actions.len(),
        1,
        "Should have fetched one code lens action, but got: {resulting_lens_actions:?}"
    );
    assert_eq!(
        resulting_lens_actions
            .values()
            .next()
            .unwrap()
            .lsp_action
            .title(),
        "LSP Command 1",
        "Only the final code lens action should be in the data"
    )
}

#[gpui::test]
async fn test_collaborating_with_code_lens_resolve(
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
    cx_b.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.code_lens = Some(settings::CodeLens::Menu);
            });
        });
    });

    let capabilities = lsp::ServerCapabilities {
        code_lens_provider: Some(lsp::CodeLensOptions {
            resolve_provider: Some(true),
        }),
        ..lsp::ServerCapabilities::default()
    };
    client_a.language_registry().add(rust_lang());
    client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: capabilities.clone(),
            initializer: Some(Box::new(|fake_lsp| {
                fake_lsp.set_request_handler::<lsp::request::CodeLensRequest, _, _>(
                    |_, _| async move {
                        Ok(Some(vec![lsp::CodeLens {
                            range: lsp::Range::new(
                                lsp::Position::new(0, 0),
                                lsp::Position::new(0, 9),
                            ),
                            command: None,
                            data: Some(serde_json::json!({ "id": "lens" })),
                        }]))
                    },
                );
                fake_lsp.set_request_handler::<lsp::request::CodeLensResolve, _, _>(
                    |lens, _| async move {
                        Ok(lsp::CodeLens {
                            command: Some(lsp::Command {
                                title: "1 reference".to_string(),
                                command: "noop".to_string(),
                                arguments: None,
                            }),
                            ..lens
                        })
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
            path!("/dir"),
            json!({
                "one.rs": "const ONE: usize = 1;"
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/dir"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);
    let editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("one.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(0, 0)]);
        });
    });
    cx_a.background_executor
        .advance_clock(editor::CODE_ACTIONS_DEBOUNCE_TIMEOUT * 2);
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.toggle_code_actions(
            &ToggleCodeActions {
                deployed_from: None,
                quick_launch: false,
            },
            window,
            cx,
        );
    });
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    editor_b.update(cx_b, |editor, _| {
        assert!(editor.context_menu_visible());
        let menu = editor.context_menu().borrow();
        let actions_menu = match menu.as_ref() {
            Some(CodeContextMenu::CodeActions(m)) => m,
            _ => panic!("Expected code actions menu to be visible"),
        };
        let item = actions_menu
            .actions
            .get(0)
            .expect("Expected at least one item in menu");
        assert_eq!(item.label(), "1 reference");
    });
}
