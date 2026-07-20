use crate::TestServer;
use call::ActiveCall;
use editor::{
    DocumentColorsRenderMode, Editor, LSP_REQUEST_DEBOUNCE_TIMEOUT, MultiBufferOffset,
    SelectionEffects,
};
use futures::StreamExt;
use gpui::{App, Rgba, TestAppContext, UpdateGlobal};
use language::{FakeLspAdapter, rust_lang};
use pretty_assertions::assert_eq;
use serde_json::json;
use settings::SettingsStore;
use std::sync::{
    Arc,
    atomic::{self, AtomicUsize},
};
use util::{path, rel_path::rel_path};

#[gpui::test(iterations = 10)]
async fn test_lsp_document_color(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let expected_color = Rgba {
        r: 0.33,
        g: 0.33,
        b: 0.33,
        a: 0.33,
    };
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
                settings.editor.lsp_document_colors = Some(DocumentColorsRenderMode::None);
            });
        });
    });
    cx_b.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.lsp_document_colors = Some(DocumentColorsRenderMode::Inlay);
            });
        });
    });

    let capabilities = lsp::ServerCapabilities {
        color_provider: Some(lsp::ColorProviderCapability::Simple(true)),
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

    // Client A opens a project.
    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "main.rs": "fn main() { a }",
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
    let editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let fake_language_server = fake_language_servers.next().await.unwrap();
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let requests_made = Arc::new(AtomicUsize::new(0));
    let closure_requests_made = Arc::clone(&requests_made);
    let mut color_request_handle = fake_language_server
        .set_request_handler::<lsp::request::DocumentColor, _, _>(move |params, _| {
            let requests_made = Arc::clone(&closure_requests_made);
            async move {
                assert_eq!(
                    params.text_document.uri,
                    lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                );
                requests_made.fetch_add(1, atomic::Ordering::Release);
                Ok(vec![lsp::ColorInformation {
                    range: lsp::Range {
                        start: lsp::Position {
                            line: 0,
                            character: 0,
                        },
                        end: lsp::Position {
                            line: 0,
                            character: 1,
                        },
                    },
                    color: lsp::Color {
                        red: 0.33,
                        green: 0.33,
                        blue: 0.33,
                        alpha: 0.33,
                    },
                }])
            }
        });
    executor.run_until_parked();

    assert_eq!(
        0,
        requests_made.load(atomic::Ordering::Acquire),
        "Host did not enable document colors, hence should query for none"
    );
    editor_a.update(cx_a, |editor, cx| {
        assert_eq!(
            Vec::<Rgba>::new(),
            extract_color_inlays(editor, cx),
            "No query colors should result in no hints"
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

    color_request_handle.next().await.unwrap();
    executor.advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT);
    executor.run_until_parked();

    assert_eq!(
        1,
        requests_made.load(atomic::Ordering::Acquire),
        "The client opened the file and got its first colors back"
    );
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            vec![expected_color],
            extract_color_inlays(editor, cx),
            "With document colors as inlays, color inlays should be pushed"
        );
    });

    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(13)..MultiBufferOffset(13)].clone())
        });
        editor.handle_input(":", window, cx);
    });
    color_request_handle.next().await.unwrap();
    executor.run_until_parked();
    assert_eq!(
        2,
        requests_made.load(atomic::Ordering::Acquire),
        "After the host edits his file, the client should request the colors again"
    );
    editor_a.update(cx_a, |editor, cx| {
        assert_eq!(
            Vec::<Rgba>::new(),
            extract_color_inlays(editor, cx),
            "Host has no colors still"
        );
    });
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(vec![expected_color], extract_color_inlays(editor, cx),);
    });

    cx_b.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.lsp_document_colors = Some(DocumentColorsRenderMode::Background);
            });
        });
    });
    executor.run_until_parked();
    assert_eq!(
        2,
        requests_made.load(atomic::Ordering::Acquire),
        "After the client have changed the colors settings, no extra queries should happen"
    );
    editor_a.update(cx_a, |editor, cx| {
        assert_eq!(
            Vec::<Rgba>::new(),
            extract_color_inlays(editor, cx),
            "Host is unaffected by the client's settings changes"
        );
    });
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            Vec::<Rgba>::new(),
            extract_color_inlays(editor, cx),
            "Client should have no colors hints, as in the settings"
        );
    });

    cx_b.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.lsp_document_colors = Some(DocumentColorsRenderMode::Inlay);
            });
        });
    });
    executor.run_until_parked();
    assert_eq!(
        2,
        requests_made.load(atomic::Ordering::Acquire),
        "After falling back to colors as inlays, no extra LSP queries are made"
    );
    editor_a.update(cx_a, |editor, cx| {
        assert_eq!(
            Vec::<Rgba>::new(),
            extract_color_inlays(editor, cx),
            "Host is unaffected by the client's settings changes, again"
        );
    });
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            vec![expected_color],
            extract_color_inlays(editor, cx),
            "Client should have its color hints back"
        );
    });

    cx_a.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.lsp_document_colors = Some(DocumentColorsRenderMode::Border);
            });
        });
    });
    color_request_handle.next().await.unwrap();
    executor.run_until_parked();
    assert_eq!(
        3,
        requests_made.load(atomic::Ordering::Acquire),
        "After the host enables document colors, another LSP query should be made"
    );
    editor_a.update(cx_a, |editor, cx| {
        assert_eq!(
            Vec::<Rgba>::new(),
            extract_color_inlays(editor, cx),
            "Host did not configure document colors as hints hence gets nothing"
        );
    });
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            vec![expected_color],
            extract_color_inlays(editor, cx),
            "Client should be unaffected by the host's settings changes"
        );
    });
}

fn extract_color_inlays(editor: &Editor, cx: &App) -> Vec<Rgba> {
    editor
        .all_inlays(cx)
        .into_iter()
        .filter_map(|inlay| inlay.get_color())
        .map(Rgba::from)
        .collect()
}
