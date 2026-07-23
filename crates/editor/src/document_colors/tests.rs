use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{self, AtomicUsize},
    },
    time::Duration,
};

use futures::StreamExt;
use gpui::{Rgba, TestAppContext};
use language::FakeLspAdapter;
use languages::rust_lang;
use project::{FakeFs, Project};
use serde_json::json;
use util::{path, rel_path::rel_path};
use workspace::{
    CloseActiveItem, MoveItemToPaneInDirection, MultiWorkspace, OpenOptions,
    item::{Item as _, SaveOptions},
};

use crate::{Editor, LSP_REQUEST_DEBOUNCE_TIMEOUT, actions::MoveToEnd, editor_tests::init_test};

fn extract_color_inlays(editor: &Editor, cx: &gpui::App) -> Vec<Rgba> {
    editor
        .all_inlays(cx)
        .into_iter()
        .filter_map(|inlay| inlay.get_color())
        .map(Rgba::from)
        .collect()
}

#[gpui::test(iterations = 10)]
async fn test_document_colors(cx: &mut TestAppContext) {
    let expected_color = Rgba {
        r: 0.33,
        g: 0.33,
        b: 0.33,
        a: 0.33,
    };

    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "first.rs": "fn main() { let a = 5; }",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                color_provider: Some(lsp::ColorProviderCapability::Simple(true)),
                ..lsp::ServerCapabilities::default()
            },
            name: "rust-analyzer",
            ..FakeLspAdapter::default()
        },
    );
    let mut fake_servers_without_capabilities = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                color_provider: Some(lsp::ColorProviderCapability::Simple(false)),
                ..lsp::ServerCapabilities::default()
            },
            name: "not-rust-analyzer",
            ..FakeLspAdapter::default()
        },
    );

    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/a/first.rs")),
                OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    let fake_language_server = fake_servers.next().await.unwrap();
    let fake_language_server_without_capabilities =
        fake_servers_without_capabilities.next().await.unwrap();
    let requests_made = Arc::new(AtomicUsize::new(0));
    let closure_requests_made = Arc::clone(&requests_made);
    let mut color_request_handle = fake_language_server
        .set_request_handler::<lsp::request::DocumentColor, _, _>(move |params, _| {
            let requests_made = Arc::clone(&closure_requests_made);
            async move {
                assert_eq!(
                    params.text_document.uri,
                    lsp::Uri::from_file_path(path!("/a/first.rs")).unwrap()
                );
                requests_made.fetch_add(1, atomic::Ordering::Release);
                Ok(vec![
                    lsp::ColorInformation {
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
                    },
                    lsp::ColorInformation {
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
                    },
                ])
            }
        });

    let _handle = fake_language_server_without_capabilities
        .set_request_handler::<lsp::request::DocumentColor, _, _>(move |_, _| async move {
            panic!("Should not be called");
        });
    cx.executor().advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT);
    color_request_handle.next().await.unwrap();
    cx.run_until_parked();
    assert_eq!(
        1,
        requests_made.load(atomic::Ordering::Acquire),
        "Should query for colors once per editor open"
    );
    editor.update_in(cx, |editor, _, cx| {
        assert_eq!(
            vec![expected_color],
            extract_color_inlays(editor, cx),
            "Should have an initial inlay"
        );
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert_eq!(
            workspace.panes().len(),
            1,
            "Should have one pane with one editor"
        );
        workspace.move_item_to_pane_in_direction(
            &MoveItemToPaneInDirection {
                direction: workspace::SplitDirection::Right,
                focus: false,
                clone: true,
            },
            window,
            cx,
        );
    });
    cx.run_until_parked();
    workspace.update_in(cx, |workspace, _, cx| {
        let panes = workspace.panes();
        assert_eq!(panes.len(), 2, "Should have two panes after splitting");
        for pane in panes {
            let editor = pane
                .read(cx)
                .active_item()
                .and_then(|item| item.downcast::<Editor>())
                .expect("Should have opened an editor in each split");
            let editor_file = editor
                .read(cx)
                .buffer()
                .read(cx)
                .as_singleton()
                .expect("test deals with singleton buffers")
                .read(cx)
                .file()
                .expect("test buffese should have a file")
                .path();
            assert_eq!(
                editor_file.as_ref(),
                rel_path("first.rs"),
                "Both editors should be opened for the same file"
            )
        }
    });

    cx.executor().advance_clock(Duration::from_millis(500));
    let save = editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.handle_input("dirty", window, cx);
        editor.save(
            SaveOptions {
                format: true,
                force_format: false,
                autosave: true,
            },
            project.clone(),
            window,
            cx,
        )
    });
    save.await.unwrap();

    color_request_handle.next().await.unwrap();
    cx.run_until_parked();
    assert_eq!(
        2,
        requests_made.load(atomic::Ordering::Acquire),
        "Should query for colors once per save (deduplicated) and once per formatting after save"
    );

    drop(editor);
    let close = workspace.update_in(cx, |workspace, window, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            pane.close_active_item(&CloseActiveItem::default(), window, cx)
        })
    });
    close.await.unwrap();
    let close = workspace.update_in(cx, |workspace, window, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            pane.close_active_item(&CloseActiveItem::default(), window, cx)
        })
    });
    close.await.unwrap();
    assert_eq!(
        2,
        requests_made.load(atomic::Ordering::Acquire),
        "After saving and closing all editors, no extra requests should be made"
    );
    workspace.update_in(cx, |workspace, _, cx| {
        assert!(
            workspace.active_item(cx).is_none(),
            "Should close all editors"
        )
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            pane.navigate_backward(&workspace::GoBack, window, cx);
        })
    });
    cx.executor().advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT);
    cx.run_until_parked();
    let editor = workspace.update_in(cx, |workspace, _, cx| {
        workspace
            .active_item(cx)
            .expect("Should have reopened the editor again after navigating back")
            .downcast::<Editor>()
            .expect("Should be an editor")
    });

    assert_eq!(
        2,
        requests_made.load(atomic::Ordering::Acquire),
        "Cache should be reused on buffer close and reopen"
    );
    editor.update(cx, |editor, cx| {
        assert_eq!(
            vec![expected_color],
            extract_color_inlays(editor, cx),
            "Should have an initial inlay"
        );
    });

    drop(color_request_handle);
    let closure_requests_made = Arc::clone(&requests_made);
    let mut empty_color_request_handle = fake_language_server
        .set_request_handler::<lsp::request::DocumentColor, _, _>(move |params, _| {
            let requests_made = Arc::clone(&closure_requests_made);
            async move {
                assert_eq!(
                    params.text_document.uri,
                    lsp::Uri::from_file_path(path!("/a/first.rs")).unwrap()
                );
                requests_made.fetch_add(1, atomic::Ordering::Release);
                Ok(Vec::new())
            }
        });
    let save = editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.handle_input("dirty_again", window, cx);
        editor.save(
            SaveOptions {
                format: false,
                force_format: false,
                autosave: true,
            },
            project.clone(),
            window,
            cx,
        )
    });
    save.await.unwrap();

    cx.executor().advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT);
    empty_color_request_handle.next().await.unwrap();
    cx.run_until_parked();
    assert_eq!(
        3,
        requests_made.load(atomic::Ordering::Acquire),
        "Should query for colors once per save only, as formatting was not requested"
    );
    editor.update(cx, |editor, cx| {
        assert_eq!(
            Vec::<Rgba>::new(),
            extract_color_inlays(editor, cx),
            "Should clear all colors when the server returns an empty response"
        );
    });
}
