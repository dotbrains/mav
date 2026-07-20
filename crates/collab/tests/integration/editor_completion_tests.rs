use call::ActiveCall;
use editor::{
    Editor, MultiBufferOffset, SelectionEffects,
    actions::{ConfirmCompletion, ContextMenuFirst},
};
use futures::StreamExt as _;
use gpui::{TestAppContext, VisualContext};
use language::{FakeLspAdapter, rust_lang};
use pretty_assertions::assert_eq;
use serde_json::json;
use util::{path, rel_path::rel_path};

use crate::TestServer;

#[gpui::test]
async fn test_collaborating_with_completion(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    let capabilities = lsp::ServerCapabilities {
        completion_provider: Some(lsp::CompletionOptions {
            trigger_characters: Some(vec![".".to_string()]),
            resolve_provider: Some(true),
            ..lsp::CompletionOptions::default()
        }),
        ..lsp::ServerCapabilities::default()
    };
    client_a.language_registry().add(rust_lang());
    let mut fake_language_servers = [
        client_a.language_registry().register_fake_lsp(
            "Rust",
            FakeLspAdapter {
                capabilities: capabilities.clone(),
                initializer: Some(Box::new(|fake_server| {
                    fake_server.set_request_handler::<lsp::request::Completion, _, _>(
                        |params, _| async move {
                            assert_eq!(
                                params.text_document_position.text_document.uri,
                                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                            );
                            assert_eq!(
                                params.text_document_position.position,
                                lsp::Position::new(0, 14),
                            );

                            Ok(Some(lsp::CompletionResponse::Array(vec![
                                lsp::CompletionItem {
                                    label: "first_method(…)".into(),
                                    detail: Some("fn(&mut self, B) -> C".into()),
                                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                                        new_text: "first_method($1)".to_string(),
                                        range: lsp::Range::new(
                                            lsp::Position::new(0, 14),
                                            lsp::Position::new(0, 14),
                                        ),
                                    })),
                                    insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                                    ..Default::default()
                                },
                                lsp::CompletionItem {
                                    label: "second_method(…)".into(),
                                    detail: Some("fn(&mut self, C) -> D<E>".into()),
                                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                                        new_text: "second_method()".to_string(),
                                        range: lsp::Range::new(
                                            lsp::Position::new(0, 14),
                                            lsp::Position::new(0, 14),
                                        ),
                                    })),
                                    insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                                    ..Default::default()
                                },
                            ])))
                        },
                    );
                })),
                ..FakeLspAdapter::default()
            },
        ),
        client_a.language_registry().register_fake_lsp(
            "Rust",
            FakeLspAdapter {
                name: "fake-analyzer",
                capabilities: capabilities.clone(),
                initializer: Some(Box::new(|fake_server| {
                    fake_server.set_request_handler::<lsp::request::Completion, _, _>(
                        |_, _| async move { Ok(None) },
                    );
                })),
                ..FakeLspAdapter::default()
            },
        ),
    ];
    client_b.language_registry().add(rust_lang());
    client_b.language_registry().register_fake_lsp_adapter(
        "Rust",
        FakeLspAdapter {
            capabilities: capabilities.clone(),
            ..FakeLspAdapter::default()
        },
    );
    client_b.language_registry().register_fake_lsp_adapter(
        "Rust",
        FakeLspAdapter {
            name: "fake-analyzer",
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
                "other.rs": "",
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
        Editor::for_buffer(buffer_b.clone(), Some(project_b.clone()), window, cx)
    });

    let fake_language_server = fake_language_servers[0].next().await.unwrap();
    let second_fake_language_server = fake_language_servers[1].next().await.unwrap();
    cx_a.background_executor.run_until_parked();
    cx_b.background_executor.run_until_parked();

    buffer_b.read_with(cx_b, |buffer, _| {
        assert!(!buffer.completion_triggers().is_empty())
    });

    // Set up the completion request handlers BEFORE typing the trigger character.
    // This is critical - the handlers must be in place when the request arrives,
    // otherwise the requests will time out waiting for a response.
    let mut first_completion_request = fake_language_server
        .set_request_handler::<lsp::request::Completion, _, _>(|params, _| async move {
            assert_eq!(
                params.text_document_position.text_document.uri,
                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
            );
            assert_eq!(
                params.text_document_position.position,
                lsp::Position::new(0, 14),
            );

            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "first_method(…)".into(),
                    detail: Some("fn(&mut self, B) -> C".into()),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        new_text: "first_method($1)".to_string(),
                        range: lsp::Range::new(
                            lsp::Position::new(0, 14),
                            lsp::Position::new(0, 14),
                        ),
                    })),
                    insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                    ..Default::default()
                },
                lsp::CompletionItem {
                    label: "second_method(…)".into(),
                    detail: Some("fn(&mut self, C) -> D<E>".into()),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        new_text: "second_method()".to_string(),
                        range: lsp::Range::new(
                            lsp::Position::new(0, 14),
                            lsp::Position::new(0, 14),
                        ),
                    })),
                    insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                    ..Default::default()
                },
            ])))
        });
    let mut second_completion_request = second_fake_language_server
        .set_request_handler::<lsp::request::Completion, _, _>(|_, _| async move { Ok(None) });
    // Type a completion trigger character as the guest.
    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(13)..MultiBufferOffset(13)])
        });
        editor.handle_input(".", window, cx);
    });
    cx_b.focus(&editor_b);

    // Allow the completion request to propagate from guest to host to LSP.
    cx_b.background_executor.run_until_parked();
    cx_a.background_executor.run_until_parked();

    // Wait for the completion requests to be received by the fake language servers.
    first_completion_request.next().await.unwrap();
    second_completion_request.next().await.unwrap();

    // Open the buffer on the host.
    let buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();
    cx_a.executor().run_until_parked();

    buffer_a.read_with(cx_a, |buffer, _| {
        assert_eq!(buffer.text(), "fn main() { a. }")
    });

    // Confirm a completion on the guest.
    editor_b.update_in(cx_b, |editor, window, cx| {
        assert!(editor.context_menu_visible());
        editor.confirm_completion(&ConfirmCompletion { item_ix: Some(0) }, window, cx);
        assert_eq!(editor.text(cx), "fn main() { a.first_method() }");
    });

    // Return a resolved completion from the host's language server.
    // The resolved completion has an additional text edit.
    fake_language_server.set_request_handler::<lsp::request::ResolveCompletionItem, _, _>(
        |params, _| async move {
            assert_eq!(params.label, "first_method(…)");
            Ok(lsp::CompletionItem {
                label: "first_method(…)".into(),
                detail: Some("fn(&mut self, B) -> C".into()),
                text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                    new_text: "first_method($1)".to_string(),
                    range: lsp::Range::new(lsp::Position::new(0, 14), lsp::Position::new(0, 14)),
                })),
                additional_text_edits: Some(vec![lsp::TextEdit {
                    new_text: "use d::SomeTrait;\n".to_string(),
                    range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 0)),
                }]),
                insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                ..Default::default()
            })
        },
    );

    // The additional edit is applied.
    cx_a.executor().run_until_parked();
    cx_b.executor().run_until_parked();

    buffer_a.read_with(cx_a, |buffer, _| {
        assert_eq!(
            buffer.text(),
            "use d::SomeTrait;\nfn main() { a.first_method() }"
        );
    });

    buffer_b.read_with(cx_b, |buffer, _| {
        assert_eq!(
            buffer.text(),
            "use d::SomeTrait;\nfn main() { a.first_method() }"
        );
    });

    // Now we do a second completion, this time to ensure that documentation/snippets are
    // resolved
    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(46)..MultiBufferOffset(46)])
        });
        editor.handle_input("; a", window, cx);
        editor.handle_input(".", window, cx);
    });

    buffer_b.read_with(cx_b, |buffer, _| {
        assert_eq!(
            buffer.text(),
            "use d::SomeTrait;\nfn main() { a.first_method(); a. }"
        );
    });

    let mut completion_response = fake_language_server
        .set_request_handler::<lsp::request::Completion, _, _>(|params, _| async move {
            assert_eq!(
                params.text_document_position.text_document.uri,
                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
            );
            assert_eq!(
                params.text_document_position.position,
                lsp::Position::new(1, 32),
            );

            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "third_method(…)".into(),
                    detail: Some("fn(&mut self, B, C, D) -> E".into()),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        // no snippet placeholders
                        new_text: "third_method".to_string(),
                        range: lsp::Range::new(
                            lsp::Position::new(1, 32),
                            lsp::Position::new(1, 32),
                        ),
                    })),
                    insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                    documentation: None,
                    ..Default::default()
                },
            ])))
        });

    // Second language server also needs to handle the request (returns None)
    let mut second_completion_response = second_fake_language_server
        .set_request_handler::<lsp::request::Completion, _, _>(|_, _| async move { Ok(None) });

    // The completion now gets a new `text_edit.new_text` when resolving the completion item
    let mut resolve_completion_response = fake_language_server
        .set_request_handler::<lsp::request::ResolveCompletionItem, _, _>(|params, _| async move {
            assert_eq!(params.label, "third_method(…)");
            Ok(lsp::CompletionItem {
                label: "third_method(…)".into(),
                detail: Some("fn(&mut self, B, C, D) -> E".into()),
                text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                    // Now it's a snippet
                    new_text: "third_method($1, $2, $3)".to_string(),
                    range: lsp::Range::new(lsp::Position::new(1, 32), lsp::Position::new(1, 32)),
                })),
                insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                documentation: Some(lsp::Documentation::String(
                    "this is the documentation".into(),
                )),
                ..Default::default()
            })
        });

    cx_b.executor().run_until_parked();

    completion_response.next().await.unwrap();
    second_completion_response.next().await.unwrap();

    editor_b.update_in(cx_b, |editor, window, cx| {
        assert!(editor.context_menu_visible());
        editor.context_menu_first(&ContextMenuFirst {}, window, cx);
    });

    resolve_completion_response.next().await.unwrap();
    cx_b.executor().run_until_parked();

    // When accepting the completion, the snippet is insert.
    editor_b.update_in(cx_b, |editor, window, cx| {
        assert!(editor.context_menu_visible());
        editor.confirm_completion(&ConfirmCompletion { item_ix: Some(0) }, window, cx);
        assert_eq!(
            editor.text(cx),
            "use d::SomeTrait;\nfn main() { a.first_method(); a.third_method(, , ) }"
        );
    });

    // Ensure buffer is synced before proceeding with the next test
    cx_a.executor().run_until_parked();
    cx_b.executor().run_until_parked();

    // Test completions from the second fake language server
    // Add another completion trigger to test the second language server
    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(68)..MultiBufferOffset(68)])
        });
        editor.handle_input("; b", window, cx);
        editor.handle_input(".", window, cx);
    });

    buffer_b.read_with(cx_b, |buffer, _| {
        assert_eq!(
            buffer.text(),
            "use d::SomeTrait;\nfn main() { a.first_method(); a.third_method(, , ); b. }"
        );
    });

    // Set up completion handlers for both language servers
    let mut first_lsp_completion = fake_language_server
        .set_request_handler::<lsp::request::Completion, _, _>(|_, _| async move { Ok(None) });

    let mut second_lsp_completion = second_fake_language_server
        .set_request_handler::<lsp::request::Completion, _, _>(|params, _| async move {
            assert_eq!(
                params.text_document_position.text_document.uri,
                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
            );
            assert_eq!(
                params.text_document_position.position,
                lsp::Position::new(1, 54),
            );

            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "analyzer_method(…)".into(),
                    detail: Some("fn(&self) -> Result<T>".into()),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        new_text: "analyzer_method()".to_string(),
                        range: lsp::Range::new(
                            lsp::Position::new(1, 54),
                            lsp::Position::new(1, 54),
                        ),
                    })),
                    insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                    ..lsp::CompletionItem::default()
                },
            ])))
        });

    // Await both language server responses
    first_lsp_completion.next().await.unwrap();
    second_lsp_completion.next().await.unwrap();

    cx_b.executor().run_until_parked();

    // Confirm the completion from the second language server works
    editor_b.update_in(cx_b, |editor, window, cx| {
        assert!(editor.context_menu_visible());
        editor.confirm_completion(&ConfirmCompletion { item_ix: Some(0) }, window, cx);
        assert_eq!(
            editor.text(cx),
            "use d::SomeTrait;\nfn main() { a.first_method(); a.third_method(, , ); b.analyzer_method() }"
        );
    });
}
