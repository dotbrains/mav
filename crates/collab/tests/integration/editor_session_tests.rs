use call::ActiveCall;
use collab::rpc::RECONNECT_TIMEOUT;
use editor::{
    Editor,
    test::editor_test_context::{AssertionContextManager, EditorTestContext},
};
use gpui::{AppContext as _, Entity, TestAppContext, VisualContext, VisualTestContext};
use indoc::indoc;
use recent_projects::disconnected_overlay::DisconnectedOverlay;
use rpc::RECEIVE_TIMEOUT;
use serde_json::json;
use util::{path, rel_path::rel_path};
use workspace::{CloseIntent, MultiWorkspace, Workspace};

use crate::TestServer;

#[gpui::test(iterations = 10)]
async fn test_host_disconnect(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b), (&client_c, cx_c)])
        .await;

    cx_b.update(editor::init);
    cx_b.update(recent_projects::init);

    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "a.txt": "a-contents",
                "b.txt": "b-contents",
            }),
        )
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let (project_a, worktree_id) = client_a.build_local_project(path!("/a"), cx_a).await;

    let worktree_a = project_a.read_with(cx_a, |project, cx| project.worktrees(cx).next().unwrap());
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    cx_a.background_executor.run_until_parked();

    assert!(worktree_a.read_with(cx_a, |tree, _| tree.has_update_observer()));

    let window_b = cx_b.add_window(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                None,
                project_b.clone(),
                client_b.app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });
    let cx_b = &mut VisualTestContext::from_window(*window_b, cx_b);
    let workspace_b = window_b
        .root(cx_b)
        .unwrap()
        .read_with(cx_b, |multi_workspace, _| {
            multi_workspace.workspace().clone()
        });

    let editor_b: Entity<Editor> = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("b.txt")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    //TODO: focus
    assert!(
        cx_b.update_window_entity(&editor_b, |editor: &mut Editor, window, _| editor
            .is_focused(window))
    );
    editor_b.update_in(cx_b, |editor: &mut Editor, window, cx| {
        editor.insert("X", window, cx)
    });

    cx_b.update(|_, cx| {
        assert!(workspace_b.read(cx).is_edited());
    });

    // Drop client A's connection. Collaborators should disappear and the project should not be shown as shared.
    server.forbid_connections();
    server.disconnect_client(client_a.peer_id().unwrap());
    cx_a.background_executor
        .advance_clock(RECEIVE_TIMEOUT + RECONNECT_TIMEOUT);

    project_a.read_with(cx_a, |project, _| project.collaborators().is_empty());

    project_a.read_with(cx_a, |project, _| assert!(!project.is_shared()));

    project_b.read_with(cx_b, |project, cx| project.is_read_only(cx));

    assert!(worktree_a.read_with(cx_a, |tree, _| !tree.has_update_observer()));

    // Ensure client B's edited state is reset and that the whole window is blurred.
    workspace_b.update(cx_b, |workspace, cx| {
        assert!(workspace.active_modal::<DisconnectedOverlay>(cx).is_some());
        assert!(!workspace.is_edited());
    });

    // Ensure client B is not prompted to save edits when closing window after disconnecting.
    let can_close: bool = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.prepare_to_close(CloseIntent::Quit, window, cx)
        })
        .await
        .unwrap();
    assert!(can_close);

    // Allow client A to reconnect to the server.
    server.allow_connections();
    cx_a.background_executor.advance_clock(RECONNECT_TIMEOUT);

    // Client B calls client A again after they reconnected.
    let active_call_b = cx_b.read(ActiveCall::global);
    active_call_b
        .update(cx_b, |call, cx| {
            call.invite(client_a.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    cx_a.background_executor.run_until_parked();
    active_call_a
        .update(cx_a, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();

    active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    // Drop client A's connection again. We should still unshare it successfully.
    server.forbid_connections();
    server.disconnect_client(client_a.peer_id().unwrap());
    cx_a.background_executor
        .advance_clock(RECEIVE_TIMEOUT + RECONNECT_TIMEOUT);

    project_a.read_with(cx_a, |project, _| assert!(!project.is_shared()));
}

#[gpui::test]
async fn test_newline_above_or_below_does_not_move_guest_cursor(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let executor = cx_a.executor();
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(path!("/dir"), json!({ "a.txt": "Some text\n" }))
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/dir"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Open a buffer as client A
    let buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();
    let cx_a = cx_a.add_empty_window();
    let editor_a = cx_a
        .new_window_entity(|window, cx| Editor::for_buffer(buffer_a, Some(project_a), window, cx));

    let mut editor_cx_a = EditorTestContext {
        cx: cx_a.clone(),
        window: cx_a.window_handle(),
        editor: editor_a,
        assertion_cx: AssertionContextManager::new(),
    };

    let cx_b = cx_b.add_empty_window();
    // Open a buffer as client B
    let buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();
    let editor_b = cx_b
        .new_window_entity(|window, cx| Editor::for_buffer(buffer_b, Some(project_b), window, cx));

    let mut editor_cx_b = EditorTestContext {
        cx: cx_b.clone(),
        window: cx_b.window_handle(),
        editor: editor_b,
        assertion_cx: AssertionContextManager::new(),
    };

    // Test newline above
    editor_cx_a.set_selections_state(indoc! {"
        Some textˇ
    "});
    editor_cx_b.set_selections_state(indoc! {"
        Some textˇ
    "});
    editor_cx_a.update_editor(|editor, window, cx| {
        editor.newline_above(&editor::actions::NewlineAbove, window, cx)
    });
    executor.run_until_parked();
    editor_cx_a.assert_editor_state(indoc! {"
        ˇ
        Some text
    "});
    editor_cx_b.assert_editor_state(indoc! {"

        Some textˇ
    "});

    // Test newline below
    editor_cx_a.set_selections_state(indoc! {"

        Some textˇ
    "});
    editor_cx_b.set_selections_state(indoc! {"

        Some textˇ
    "});
    editor_cx_a.update_editor(|editor, window, cx| {
        editor.newline_below(&editor::actions::NewlineBelow, window, cx)
    });
    executor.run_until_parked();
    editor_cx_a.assert_editor_state(indoc! {"

        Some text
        ˇ
    "});
    editor_cx_b.assert_editor_state(indoc! {"

        Some textˇ

    "});
}
