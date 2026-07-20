use crate::TestServer;
use call::ActiveCall;
use editor::{
    Editor,
    actions::{ExpandMacroRecursively, SelectAll},
    test::expand_macro_recursively,
};
use futures::StreamExt;
use gpui::TestAppContext;
use language::{FakeLspAdapter, rust_lang};
use pretty_assertions::assert_eq;
use project::{
    ProjectPath,
    lsp_store::lsp_ext_command::{ExpandedMacro, LspExtExpandMacro},
};
use serde_json::json;
use std::sync::Arc;
use util::{path, rel_path::rel_path};

#[gpui::test]
async fn test_add_breakpoints(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let executor = cx_a.executor();
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);
    cx_a.update(editor::init);
    cx_b.update(editor::init);
    client_a
        .fs()
        .insert_tree(
            "/a",
            json!({
                "test.txt": "one\ntwo\nthree\nfour\nfive",
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project("/a", cx_a).await;
    let project_path = ProjectPath {
        worktree_id,
        path: rel_path(&"test.txt").into(),
    };
    let abs_path = project_a.read_with(cx_a, |project, cx| {
        project
            .absolute_path(&project_path, cx)
            .map(Arc::from)
            .unwrap()
    });

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

    // Client A opens an editor.
    let editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path(project_path.clone(), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    // Client B opens same editor as A.
    let editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path(project_path.clone(), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    // Client A adds breakpoint on line (1)
    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.toggle_breakpoint(&editor::actions::ToggleBreakpoint, window, cx);
    });

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let breakpoints_a = editor_a.update(cx_a, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });
    let breakpoints_b = editor_b.update(cx_b, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(1, breakpoints_a.len());
    assert_eq!(1, breakpoints_a.get(&abs_path).unwrap().len());
    assert_eq!(breakpoints_a, breakpoints_b);

    // Client B adds breakpoint on line(2)
    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.toggle_breakpoint(&editor::actions::ToggleBreakpoint, window, cx);
    });

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let breakpoints_a = editor_a.update(cx_a, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });
    let breakpoints_b = editor_b.update(cx_b, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(1, breakpoints_a.len());
    assert_eq!(breakpoints_a, breakpoints_b);
    assert_eq!(2, breakpoints_a.get(&abs_path).unwrap().len());

    // Client A removes last added breakpoint from client B
    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.toggle_breakpoint(&editor::actions::ToggleBreakpoint, window, cx);
    });

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let breakpoints_a = editor_a.update(cx_a, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });
    let breakpoints_b = editor_b.update(cx_b, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(1, breakpoints_a.len());
    assert_eq!(breakpoints_a, breakpoints_b);
    assert_eq!(1, breakpoints_a.get(&abs_path).unwrap().len());

    // Client B removes first added breakpoint by client A
    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.move_up(&mav_actions::editor::MoveUp, window, cx);
        editor.move_up(&mav_actions::editor::MoveUp, window, cx);
        editor.toggle_breakpoint(&editor::actions::ToggleBreakpoint, window, cx);
    });

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let breakpoints_a = editor_a.update(cx_a, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });
    let breakpoints_b = editor_b.update(cx_b, |editor, cx| {
        editor
            .breakpoint_store()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(0, breakpoints_a.len());
    assert_eq!(breakpoints_a, breakpoints_b);
}

#[gpui::test]
async fn test_client_can_query_lsp_ext(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    cx_a.update(editor::init);
    cx_b.update(editor::init);

    client_a.language_registry().add(rust_lang());
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "rust-analyzer",
            ..FakeLspAdapter::default()
        },
    );
    client_b.language_registry().add(rust_lang());
    client_b.language_registry().register_fake_lsp_adapter(
        "Rust",
        FakeLspAdapter {
            name: "rust-analyzer",
            ..FakeLspAdapter::default()
        },
    );

    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "main.rs": "fn main() {}",
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

    let fake_language_server = fake_language_servers.next().await.unwrap();

    // host
    let mut expand_request_a = fake_language_server.set_request_handler::<LspExtExpandMacro, _, _>(
        |params, _| async move {
            assert_eq!(
                params.text_document.uri,
                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
            );
            assert_eq!(params.position, lsp::Position::new(0, 0));
            Ok(Some(ExpandedMacro {
                name: "test_macro_name".to_string(),
                expansion: "test_macro_expansion on the host".to_string(),
            }))
        },
    );

    editor_a.update_in(cx_a, |editor, window, cx| {
        expand_macro_recursively(editor, &ExpandMacroRecursively, window, cx)
    });
    expand_request_a.next().await.unwrap();
    cx_a.run_until_parked();

    workspace_a.update(cx_a, |workspace, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            assert_eq!(
                pane.items_len(),
                2,
                "Should have added a macro expansion to the host's pane"
            );
            let new_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
            new_editor.update(cx, |editor, cx| {
                assert_eq!(editor.text(cx), "test_macro_expansion on the host");
            });
        })
    });

    // client
    let mut expand_request_b = fake_language_server.set_request_handler::<LspExtExpandMacro, _, _>(
        |params, _| async move {
            assert_eq!(
                params.text_document.uri,
                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
            );
            assert_eq!(
                params.position,
                lsp::Position::new(0, 12),
                "editor_b has selected the entire text and should query for a different position"
            );
            Ok(Some(ExpandedMacro {
                name: "test_macro_name".to_string(),
                expansion: "test_macro_expansion on the client".to_string(),
            }))
        },
    );

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.select_all(&SelectAll, window, cx);
        expand_macro_recursively(editor, &ExpandMacroRecursively, window, cx)
    });
    expand_request_b.next().await.unwrap();
    cx_b.run_until_parked();

    workspace_b.update(cx_b, |workspace, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            assert_eq!(
                pane.items_len(),
                2,
                "Should have added a macro expansion to the client's pane"
            );
            let new_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
            new_editor.update(cx, |editor, cx| {
                assert_eq!(editor.text(cx), "test_macro_expansion on the client");
            });
        })
    });
}
