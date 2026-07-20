use crate::TestServer;
use call::ActiveCall;
use editor::{Editor, LSP_REQUEST_DEBOUNCE_TIMEOUT};
use futures::StreamExt;
use gpui::{TestAppContext, UpdateGlobal};
use language::{FakeLspAdapter, rust_lang};
use pretty_assertions::assert_eq;
use serde_json::json;
use settings::SettingsStore;
use std::{
    str::FromStr as _,
    sync::{
        Arc,
        atomic::{self, AtomicUsize},
    },
    time::Duration,
};
use text::Point;
use util::{path, rel_path::rel_path};

#[gpui::test]
async fn test_lsp_document_links(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
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

    for cx in [&mut *cx_a, &mut *cx_b] {
        cx.update(|cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.editor.lsp_document_links = Some(true);
                });
            });
        });
    }

    let capabilities = lsp::ServerCapabilities {
        document_link_provider: Some(lsp::DocumentLinkOptions {
            resolve_provider: Some(true),
            work_done_progress_options: lsp::WorkDoneProgressOptions::default(),
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

    let other_contents = concat!(
        "fn first() {}\n",
        "fn second() {}\n",
        "fn third(x: i32) {}\n",
        "fn fourth() {}\n",
        "fn fifth() {}\n",
    );
    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "main.rs": "// see LICENSE for details\nfn main() {}",
                "other.rs": other_contents,
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
    let _editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let fake_language_server = fake_language_servers.next().await.unwrap();

    let link_range = lsp::Range {
        start: lsp::Position {
            line: 0,
            character: 7,
        },
        end: lsp::Position {
            line: 0,
            character: 14,
        },
    };
    let other_uri = lsp::Uri::from_file_path(path!("/a/other.rs")).unwrap();
    // The server points at line 3, column 5 (1-based) of `other.rs` using the
    // json-language-server fragment convention.
    let other_uri_with_fragment =
        lsp::Uri::from_str(&format!("{}#3,5", other_uri.as_str())).unwrap();
    let other_target = other_uri_with_fragment.to_string();
    let tooltip = "Open other.rs";
    let resolve_marker = serde_json::json!({"id": 42});

    let document_link_requests = Arc::new(AtomicUsize::new(0));
    let document_link_count = Arc::clone(&document_link_requests);
    let resolve_marker_for_links = resolve_marker.clone();
    let mut document_link_handle = fake_language_server
        .set_request_handler::<lsp::request::DocumentLinkRequest, _, _>(move |params, _| {
            let document_link_count = Arc::clone(&document_link_count);
            let resolve_marker = resolve_marker_for_links.clone();
            async move {
                assert_eq!(
                    params.text_document.uri,
                    lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                );
                document_link_count.fetch_add(1, atomic::Ordering::Release);
                Ok(Some(vec![lsp::DocumentLink {
                    range: link_range,
                    target: None,
                    tooltip: None,
                    data: Some(resolve_marker),
                }]))
            }
        });

    let resolve_requests = Arc::new(AtomicUsize::new(0));
    let resolve_count = Arc::clone(&resolve_requests);
    let other_uri_for_resolve = other_uri_with_fragment.clone();
    let resolve_marker_for_resolve = resolve_marker.clone();
    let _resolve_handle = fake_language_server
        .set_request_handler::<lsp::request::DocumentLinkResolve, _, _>(move |link, _| {
            let resolve_count = Arc::clone(&resolve_count);
            let other_uri = other_uri_for_resolve.clone();
            let expected_marker = resolve_marker_for_resolve.clone();
            async move {
                assert_eq!(link.range, link_range);
                assert_eq!(link.data.as_ref(), Some(&expected_marker));
                resolve_count.fetch_add(1, atomic::Ordering::Release);
                Ok(lsp::DocumentLink {
                    range: link.range,
                    target: Some(other_uri),
                    tooltip: Some(tooltip.to_string()),
                    data: None,
                })
            }
        });

    document_link_handle.next().await.unwrap();
    executor.advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT);
    executor.run_until_parked();

    assert_eq!(
        1,
        document_link_requests.load(atomic::Ordering::Acquire),
        "Host opening the file should issue exactly one documentLink request"
    );
    assert_eq!(
        0,
        resolve_requests.load(atomic::Ordering::Acquire),
        "No resolve happens until a hover triggers it"
    );

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

    assert_eq!(
        1,
        document_link_requests.load(atomic::Ordering::Acquire),
        "Guest's proto fetch should be served from the host's cached document links \
         without issuing a fresh documentLink LSP request"
    );

    let guest_buffer = editor_b
        .read_with(cx_b, |editor, cx| editor.buffer().read(cx).as_singleton())
        .unwrap();
    let buffer_id = guest_buffer.read_with(cx_b, |buffer, _| buffer.remote_id());
    let unresolved = project_b
        .read_with(cx_b, |project, cx| {
            project
                .lsp_store()
                .read(cx)
                .document_links_for_buffer(buffer_id)
                .unwrap_or_default()
        })
        .into_values()
        .flat_map(|per_server| per_server.into_values())
        .next()
        .expect("guest should mirror the fetched document link");
    assert!(
        !unresolved.resolved,
        "freshly fetched links must come back unresolved"
    );

    let resolve_task = editor_b
        .update(cx_b, |editor, cx| {
            editor.document_links_at(guest_buffer.clone(), unresolved.range.start, cx)
        })
        .expect("editor should have a cached link covering the position");
    let resolved_links = resolve_task.await;
    assert_eq!(
        1,
        resolved_links.len(),
        "`document_links_at` should yield the single matching link"
    );
    executor.run_until_parked();

    assert_eq!(
        1,
        resolve_requests.load(atomic::Ordering::Acquire),
        "Guest's resolve should reach the host's LSP exactly once"
    );

    let guest_links = project_b.read_with(cx_b, |project, cx| {
        project
            .lsp_store()
            .read(cx)
            .document_links_for_buffer(buffer_id)
            .unwrap_or_default()
    });
    assert_eq!(
        1,
        guest_links.values().map(|m| m.len()).sum::<usize>(),
        "Guest should mirror exactly one document link from the host"
    );
    let link = guest_links
        .values()
        .flat_map(|per_server| per_server.values())
        .next()
        .expect("guest cache should contain the mirrored link");
    assert_eq!(
        link.target.as_deref(),
        Some(other_target.as_str()),
        "Guest should see the resolved file:// target from the host"
    );
    assert_eq!(link.tooltip.as_deref(), Some(tooltip));

    let click_anchor = guest_buffer.read_with(cx_b, |buffer, _| buffer.anchor_before(10));
    let resolved_at_click = editor_b
        .update(cx_b, |editor, cx| {
            editor.document_links_at(guest_buffer.clone(), click_anchor, cx)
        })
        .expect("cached document link should cover the click anchor")
        .await;
    let (click_server_id, click_link) = resolved_at_click
        .into_iter()
        .next()
        .expect("resolved links should not be empty");
    let click_target = click_link
        .target
        .as_deref()
        .expect("link should be resolved")
        .to_owned();
    let navigated = editor_b
        .update_in(cx_b, |editor, window, cx| {
            let hover_link = editor::hover_links::document_link_target_to_hover_link(
                &click_target,
                click_server_id,
            );
            editor.navigate_to_hover_links(None, vec![hover_link], None, false, window, cx)
        })
        .await
        .expect("navigation task should complete");
    assert_eq!(
        navigated,
        editor::Navigated::Yes,
        "Clicking a resolved file:// document link should navigate",
    );
    executor.run_until_parked();

    let other_editor = workspace_b.update(cx_b, |workspace, cx| {
        workspace.active_item_as::<Editor>(cx).unwrap()
    });
    other_editor.update(cx_b, |editor, cx| {
        let buffer = editor.buffer().read(cx).as_singleton().unwrap();
        assert_eq!(
            buffer.read(cx).text(),
            other_contents,
            "Following the resolved link should open other.rs from the same worktree",
        );
        let head = editor
            .selections
            .newest::<Point>(&editor.display_snapshot(cx))
            .head();
        assert_eq!(
            head,
            Point::new(2, 4),
            "Cursor should land at the URI fragment's line/column (1-based 3,5 -> 0-based 2,4)",
        );
    });
}
