use crate::TestServer;
use call::ActiveCall;
use editor::{Editor, MultiBufferOffset, SelectionEffects, actions::Undo};
use futures::StreamExt;
use gpui::{TestAppContext, VisualContext};
use language::{FakeLspAdapter, rust_lang};
use pretty_assertions::assert_eq;
use serde_json::json;
use util::{path, rel_path::rel_path};

#[gpui::test(iterations = 10)]
async fn test_share_project(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let executor = cx_a.executor();
    let cx_b = cx_b.add_empty_window();
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b), (&client_c, cx_c)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);
    let active_call_c = cx_c.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                ".gitignore": "ignored-dir",
                "a.txt": "a-contents",
                "b.txt": "b-contents",
                "ignored-dir": {
                    "c.txt": "",
                    "d.txt": "",
                }
            }),
        )
        .await;

    // Invite client B to collaborate on a project
    let (project_a, worktree_id) = client_a.build_local_project(path!("/a"), cx_a).await;
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), Some(project_a.clone()), cx)
        })
        .await
        .unwrap();

    // Join that project as client B

    let incoming_call_b = active_call_b.read_with(cx_b, |call, _| call.incoming());
    executor.run_until_parked();
    let call = incoming_call_b.borrow().clone().unwrap();
    assert_eq!(call.calling_user.username, "user_a");
    let initial_project = call.initial_project.unwrap();
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    let client_b_peer_id = client_b.peer_id().unwrap();
    let project_b = client_b.join_remote_project(initial_project.id, cx_b).await;

    let replica_id_b = project_b.read_with(cx_b, |project, _| project.replica_id());

    executor.run_until_parked();

    project_a.read_with(cx_a, |project, _| {
        let client_b_collaborator = project.collaborators().get(&client_b_peer_id).unwrap();
        assert_eq!(client_b_collaborator.replica_id, replica_id_b);
    });

    project_b.read_with(cx_b, |project, cx| {
        let worktree = project.worktrees(cx).next().unwrap().read(cx);
        assert_eq!(
            worktree.paths().collect::<Vec<_>>(),
            [
                rel_path(".gitignore"),
                rel_path("a.txt"),
                rel_path("b.txt"),
                rel_path("ignored-dir"),
            ]
        );
    });

    project_b
        .update(cx_b, |project, cx| {
            let worktree = project.worktrees(cx).next().unwrap();
            let entry = worktree
                .read(cx)
                .entry_for_path(rel_path("ignored-dir"))
                .unwrap();
            project.expand_entry(worktree_id, entry.id, cx).unwrap()
        })
        .await
        .unwrap();

    project_b.read_with(cx_b, |project, cx| {
        let worktree = project.worktrees(cx).next().unwrap().read(cx);
        assert_eq!(
            worktree.paths().collect::<Vec<_>>(),
            [
                rel_path(".gitignore"),
                rel_path("a.txt"),
                rel_path("b.txt"),
                rel_path("ignored-dir"),
                rel_path("ignored-dir/c.txt"),
                rel_path("ignored-dir/d.txt"),
            ]
        );
    });

    // Open the same file as client B and client A.
    let buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("b.txt")), cx)
        })
        .await
        .unwrap();

    buffer_b.read_with(cx_b, |buf, _| assert_eq!(buf.text(), "b-contents"));

    project_a.read_with(cx_a, |project, cx| {
        assert!(project.has_open_buffer((worktree_id, rel_path("b.txt")), cx))
    });
    let buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("b.txt")), cx)
        })
        .await
        .unwrap();

    let editor_b =
        cx_b.new_window_entity(|window, cx| Editor::for_buffer(buffer_b, None, window, cx));

    // Client A sees client B's selection
    executor.run_until_parked();

    buffer_a.read_with(cx_a, |buffer, _| {
        buffer
            .snapshot()
            .selections_in_range(
                text::Anchor::min_max_range_for_buffer(buffer.remote_id()),
                false,
            )
            .count()
            == 1
    });

    // Edit the buffer as client B and see that edit as client A.
    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.handle_input("ok, ", window, cx)
    });
    executor.run_until_parked();

    buffer_a.read_with(cx_a, |buffer, _| {
        assert_eq!(buffer.text(), "ok, b-contents")
    });

    // Client B can invite client C on a project shared by client A.
    active_call_b
        .update(cx_b, |call, cx| {
            call.invite(client_c.user_id().unwrap(), Some(project_b.clone()), cx)
        })
        .await
        .unwrap();

    let incoming_call_c = active_call_c.read_with(cx_c, |call, _| call.incoming());
    executor.run_until_parked();
    let call = incoming_call_c.borrow().clone().unwrap();
    assert_eq!(call.calling_user.username, "user_b");
    let initial_project = call.initial_project.unwrap();
    active_call_c
        .update(cx_c, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    let _project_c = client_c.join_remote_project(initial_project.id, cx_c).await;

    // Client B closes the editor, and client A sees client B's selections removed.
    cx_b.update(move |_, _| drop(editor_b));
    executor.run_until_parked();

    buffer_a.read_with(cx_a, |buffer, _| {
        buffer
            .snapshot()
            .selections_in_range(
                text::Anchor::min_max_range_for_buffer(buffer.remote_id()),
                false,
            )
            .count()
            == 0
    });
}

#[gpui::test(iterations = 10)]
async fn test_on_input_format_from_host_to_guest(
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

    client_a.language_registry().add(rust_lang());
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                document_on_type_formatting_provider: Some(lsp::DocumentOnTypeFormattingOptions {
                    first_trigger_character: ":".to_string(),
                    more_trigger_character: Some(vec![">".to_string()]),
                }),
                ..Default::default()
            },
            ..Default::default()
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
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Open a file in an editor as the host.
    let buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();
    let cx_a = cx_a.add_empty_window();
    let editor_a = cx_a.new_window_entity(|window, cx| {
        Editor::for_buffer(buffer_a, Some(project_a.clone()), window, cx)
    });

    let fake_language_server = fake_language_servers.next().await.unwrap();
    executor.run_until_parked();

    // Receive an OnTypeFormatting request as the host's language server.
    // Return some formatting from the host's language server.
    fake_language_server.set_request_handler::<lsp::request::OnTypeFormatting, _, _>(
        |params, _| async move {
            assert_eq!(
                params.text_document_position.text_document.uri,
                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
            );
            assert_eq!(
                params.text_document_position.position,
                lsp::Position::new(0, 14),
            );

            Ok(Some(vec![lsp::TextEdit {
                new_text: "~<".to_string(),
                range: lsp::Range::new(lsp::Position::new(0, 14), lsp::Position::new(0, 14)),
            }]))
        },
    );

    // Open the buffer on the guest and see that the formatting worked
    let buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    // Type a on type formatting trigger character as the guest.
    cx_a.focus(&editor_a);
    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(13)..MultiBufferOffset(13)])
        });
        editor.handle_input(">", window, cx);
    });

    executor.run_until_parked();

    buffer_b.read_with(cx_b, |buffer, _| {
        assert_eq!(buffer.text(), "fn main() { a>~< }")
    });

    // Undo should remove LSP edits first
    editor_a.update_in(cx_a, |editor, window, cx| {
        assert_eq!(editor.text(cx), "fn main() { a>~< }");
        editor.undo(&Undo, window, cx);
        assert_eq!(editor.text(cx), "fn main() { a> }");
    });
    executor.run_until_parked();

    buffer_b.read_with(cx_b, |buffer, _| {
        assert_eq!(buffer.text(), "fn main() { a> }")
    });

    editor_a.update_in(cx_a, |editor, window, cx| {
        assert_eq!(editor.text(cx), "fn main() { a> }");
        editor.undo(&Undo, window, cx);
        assert_eq!(editor.text(cx), "fn main() { a }");
    });
    executor.run_until_parked();

    buffer_b.read_with(cx_b, |buffer, _| {
        assert_eq!(buffer.text(), "fn main() { a }")
    });
}

#[gpui::test(iterations = 10)]
async fn test_on_input_format_from_guest_to_host(
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

    let capabilities = lsp::ServerCapabilities {
        document_on_type_formatting_provider: Some(lsp::DocumentOnTypeFormattingOptions {
            first_trigger_character: ":".to_string(),
            more_trigger_character: Some(vec![">".to_string()]),
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
            path!("/a"),
            json!({
                "main.rs": "fn main() { a }",
                "other.rs": "// Test file",
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/a"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Open a file in an editor as the guest.
    let buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();
    let cx_b = cx_b.add_empty_window();
    let editor_b = cx_b.new_window_entity(|window, cx| {
        Editor::for_buffer(buffer_b, Some(project_b.clone()), window, cx)
    });

    let fake_language_server = fake_language_servers.next().await.unwrap();
    executor.run_until_parked();

    // Type a on type formatting trigger character as the guest.
    cx_b.focus(&editor_b);
    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(13)..MultiBufferOffset(13)])
        });
        editor.handle_input(":", window, cx);
    });

    // Receive an OnTypeFormatting request as the host's language server.
    // Return some formatting from the host's language server.
    fake_language_server
        .set_request_handler::<lsp::request::OnTypeFormatting, _, _>(|params, _| async move {
            assert_eq!(
                params.text_document_position.text_document.uri,
                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
            );
            assert_eq!(
                params.text_document_position.position,
                lsp::Position::new(0, 14),
            );

            Ok(Some(vec![lsp::TextEdit {
                new_text: "~:".to_string(),
                range: lsp::Range::new(lsp::Position::new(0, 14), lsp::Position::new(0, 14)),
            }]))
        })
        .next()
        .await
        .unwrap();

    // Open the buffer on the host and see that the formatting worked
    let buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();

    buffer_a.read_with(cx_a, |buffer, _| {
        assert_eq!(buffer.text(), "fn main() { a:~: }")
    });

    // Undo should remove LSP edits first
    editor_b.update_in(cx_b, |editor, window, cx| {
        assert_eq!(editor.text(cx), "fn main() { a:~: }");
        editor.undo(&Undo, window, cx);
        assert_eq!(editor.text(cx), "fn main() { a: }");
    });
    executor.run_until_parked();

    buffer_a.read_with(cx_a, |buffer, _| {
        assert_eq!(buffer.text(), "fn main() { a: }")
    });

    editor_b.update_in(cx_b, |editor, window, cx| {
        assert_eq!(editor.text(cx), "fn main() { a: }");
        editor.undo(&Undo, window, cx);
        assert_eq!(editor.text(cx), "fn main() { a }");
    });
    executor.run_until_parked();

    buffer_a.read_with(cx_a, |buffer, _| {
        assert_eq!(buffer.text(), "fn main() { a }")
    });
}
