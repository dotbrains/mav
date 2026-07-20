use crate::TestServer;
use call::ActiveCall;
use editor::{
    Editor, MultiBufferOffset, SelectionEffects,
    actions::{ConfirmRename, Redo, Rename, Undo},
};
use futures::StreamExt;
use gpui::TestAppContext;
use language::{FakeLspAdapter, rust_lang};
use pretty_assertions::assert_eq;
use serde_json::json;
use util::{path, rel_path::rel_path, uri};

#[gpui::test(iterations = 10)]
async fn test_collaborating_with_renames(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    cx_b.update(editor::init);

    let capabilities = lsp::ServerCapabilities {
        rename_provider: Some(lsp::OneOf::Right(lsp::RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
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
                "one.rs": "const ONE: usize = 1;",
                "two.rs": "const TWO: usize = one::ONE + one::ONE;"
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
    let fake_language_server = fake_language_servers.next().await.unwrap();
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let prepare_rename = editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(7)..MultiBufferOffset(7)])
        });
        editor.rename(&Rename, window, cx).unwrap()
    });

    fake_language_server
        .set_request_handler::<lsp::request::PrepareRenameRequest, _, _>(|params, _| async move {
            assert_eq!(
                params.text_document.uri.as_str(),
                uri!("file:///dir/one.rs")
            );
            assert_eq!(params.position, lsp::Position::new(0, 7));
            Ok(Some(lsp::PrepareRenameResponse::Range(lsp::Range::new(
                lsp::Position::new(0, 6),
                lsp::Position::new(0, 9),
            ))))
        })
        .next()
        .await
        .unwrap();
    prepare_rename.await.unwrap();
    editor_b.update(cx_b, |editor, cx| {
        use editor::ToOffset;
        let rename = editor.pending_rename().unwrap();
        let buffer = editor.buffer().read(cx).snapshot(cx);
        assert_eq!(
            rename.range.start.to_offset(&buffer)..rename.range.end.to_offset(&buffer),
            MultiBufferOffset(6)..MultiBufferOffset(9)
        );
        rename.editor.update(cx, |rename_editor, cx| {
            let rename_selection = rename_editor
                .selections
                .newest::<MultiBufferOffset>(&rename_editor.display_snapshot(cx));
            assert_eq!(
                rename_selection.range(),
                MultiBufferOffset(0)..MultiBufferOffset(3),
                "Rename that was triggered from zero selection caret, should propose the whole word."
            );
            rename_editor.buffer().update(cx, |rename_buffer, cx| {
                rename_buffer.edit([(MultiBufferOffset(0)..MultiBufferOffset(3), "THREE")], None, cx);
            });
        });
    });

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.cancel(&editor::actions::Cancel, window, cx);
    });
    let prepare_rename = editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(7)..MultiBufferOffset(8)])
        });
        editor.rename(&Rename, window, cx).unwrap()
    });

    fake_language_server
        .set_request_handler::<lsp::request::PrepareRenameRequest, _, _>(|params, _| async move {
            assert_eq!(
                params.text_document.uri.as_str(),
                uri!("file:///dir/one.rs")
            );
            assert_eq!(params.position, lsp::Position::new(0, 8));
            Ok(Some(lsp::PrepareRenameResponse::Range(lsp::Range::new(
                lsp::Position::new(0, 6),
                lsp::Position::new(0, 9),
            ))))
        })
        .next()
        .await
        .unwrap();
    prepare_rename.await.unwrap();
    editor_b.update(cx_b, |editor, cx| {
        use editor::ToOffset;
        let rename = editor.pending_rename().unwrap();
        let buffer = editor.buffer().read(cx).snapshot(cx);
        let lsp_rename_start = rename.range.start.to_offset(&buffer);
        let lsp_rename_end = rename.range.end.to_offset(&buffer);
        assert_eq!(
            lsp_rename_start..lsp_rename_end,
            MultiBufferOffset(6)..MultiBufferOffset(9)
        );
        rename.editor.update(cx, |rename_editor, cx| {
            let rename_selection = rename_editor
                .selections
                .newest::<MultiBufferOffset>(&rename_editor.display_snapshot(cx));
            assert_eq!(
                rename_selection.range(),
                MultiBufferOffset(1)..MultiBufferOffset(2),
                "Rename that was triggered from a selection, should have the same selection range in the rename proposal"
            );
            rename_editor.buffer().update(cx, |rename_buffer, cx| {
                rename_buffer.edit(
                    [(
                        MultiBufferOffset(0)..MultiBufferOffset(lsp_rename_end - lsp_rename_start),
                        "THREE",
                    )],
                    None,
                    cx,
                );
            });
        });
    });

    let confirm_rename = editor_b.update_in(cx_b, |editor, window, cx| {
        Editor::confirm_rename(editor, &ConfirmRename, window, cx).unwrap()
    });
    fake_language_server
        .set_request_handler::<lsp::request::Rename, _, _>(|params, _| async move {
            assert_eq!(
                params.text_document_position.text_document.uri.as_str(),
                uri!("file:///dir/one.rs")
            );
            assert_eq!(
                params.text_document_position.position,
                lsp::Position::new(0, 6)
            );
            assert_eq!(params.new_name, "THREE");
            Ok(Some(lsp::WorkspaceEdit {
                changes: Some(
                    [
                        (
                            lsp::Uri::from_file_path(path!("/dir/one.rs")).unwrap(),
                            vec![lsp::TextEdit::new(
                                lsp::Range::new(lsp::Position::new(0, 6), lsp::Position::new(0, 9)),
                                "THREE".to_string(),
                            )],
                        ),
                        (
                            lsp::Uri::from_file_path(path!("/dir/two.rs")).unwrap(),
                            vec![
                                lsp::TextEdit::new(
                                    lsp::Range::new(
                                        lsp::Position::new(0, 24),
                                        lsp::Position::new(0, 27),
                                    ),
                                    "THREE".to_string(),
                                ),
                                lsp::TextEdit::new(
                                    lsp::Range::new(
                                        lsp::Position::new(0, 35),
                                        lsp::Position::new(0, 38),
                                    ),
                                    "THREE".to_string(),
                                ),
                            ],
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..Default::default()
            }))
        })
        .next()
        .await
        .unwrap();
    confirm_rename.await.unwrap();

    let rename_editor = workspace_b.update(cx_b, |workspace, cx| {
        workspace.active_item_as::<Editor>(cx).unwrap()
    });

    rename_editor.update_in(cx_b, |editor, window, cx| {
        assert_eq!(
            editor.text(cx),
            "const THREE: usize = 1;\nconst TWO: usize = one::THREE + one::THREE;"
        );
        editor.undo(&Undo, window, cx);
        assert_eq!(
            editor.text(cx),
            "const ONE: usize = 1;\nconst TWO: usize = one::ONE + one::ONE;"
        );
        editor.redo(&Redo, window, cx);
        assert_eq!(
            editor.text(cx),
            "const THREE: usize = 1;\nconst TWO: usize = one::THREE + one::THREE;"
        );
    });

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.undo(&Undo, window, cx);
        assert_eq!(editor.text(cx), "const ONE: usize = 1;");
        editor.undo(&Undo, window, cx);
        assert_eq!(editor.text(cx), "const ONE: usize = 1;");
        editor.redo(&Redo, window, cx);
        assert_eq!(editor.text(cx), "const THREE: usize = 1;");
    })
}
