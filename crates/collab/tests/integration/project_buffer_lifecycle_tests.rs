use call::ActiveCall;
use client::RECEIVE_TIMEOUT;
use collab::rpc::RECONNECT_TIMEOUT;
use fs::{FakeFs, Fs as _};
use gpui::{BackgroundExecutor, TestAppContext};
use language::{LineEnding, Rope};
use pretty_assertions::assert_eq;
use project::Project;
use serde_json::json;
use util::{path, rel_path::rel_path};

use crate::TestServer;

#[gpui::test(iterations = 10)]
async fn test_buffer_conflict_after_save(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(
            path!("/dir"),
            json!({
                "a.txt": "a-contents",
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/dir"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Open a buffer as client B
    let buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();

    buffer_b.update(cx_b, |buf, cx| buf.edit([(0..0, "world ")], None, cx));

    buffer_b.read_with(cx_b, |buf, _| {
        assert!(buf.is_dirty());
        assert!(!buf.has_conflict());
    });

    project_b
        .update(cx_b, |project, cx| {
            project.save_buffer(buffer_b.clone(), cx)
        })
        .await
        .unwrap();

    buffer_b.read_with(cx_b, |buffer_b, _| assert!(!buffer_b.is_dirty()));

    buffer_b.read_with(cx_b, |buf, _| {
        assert!(!buf.has_conflict());
    });

    buffer_b.update(cx_b, |buf, cx| buf.edit([(0..0, "hello ")], None, cx));

    buffer_b.read_with(cx_b, |buf, _| {
        assert!(buf.is_dirty());
        assert!(!buf.has_conflict());
    });
}

#[gpui::test(iterations = 10)]
async fn test_buffer_reloading(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(
            path!("/dir"),
            json!({
                "a.txt": "a\nb\nc",
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/dir"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Open a buffer as client B
    let buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();

    buffer_b.read_with(cx_b, |buf, _| {
        assert!(!buf.is_dirty());
        assert!(!buf.has_conflict());
        assert_eq!(buf.line_ending(), LineEnding::Unix);
    });

    let new_contents = Rope::from("d\ne\nf");
    client_a
        .fs()
        .save(
            path!("/dir/a.txt").as_ref(),
            &new_contents,
            LineEnding::Windows,
        )
        .await
        .unwrap();

    executor.run_until_parked();

    buffer_b.read_with(cx_b, |buf, _| {
        assert_eq!(buf.text(), new_contents.to_string());
        assert!(!buf.is_dirty());
        assert!(!buf.has_conflict());
        assert_eq!(buf.line_ending(), LineEnding::Windows);
    });
}

#[gpui::test(iterations = 10)]
async fn test_editing_while_guest_opens_buffer(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(path!("/dir"), json!({ "a.txt": "a-contents" }))
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

    // Start opening the same buffer as client B
    let open_buffer = project_b.update(cx_b, |p, cx| {
        p.open_buffer((worktree_id, rel_path("a.txt")), cx)
    });
    let buffer_b = cx_b.executor().spawn(open_buffer);

    // Edit the buffer as client A while client B is still opening it.
    cx_b.executor().simulate_random_delay().await;
    buffer_a.update(cx_a, |buf, cx| buf.edit([(0..0, "X")], None, cx));
    cx_b.executor().simulate_random_delay().await;
    buffer_a.update(cx_a, |buf, cx| buf.edit([(1..1, "Y")], None, cx));

    let text = buffer_a.read_with(cx_a, |buf, _| buf.text());
    let buffer_b = buffer_b.await.unwrap();
    executor.run_until_parked();

    buffer_b.read_with(cx_b, |buf, _| assert_eq!(buf.text(), text));
}

#[gpui::test(iterations = 10)]
async fn test_leaving_worktree_while_opening_buffer(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree("/dir", json!({ "a.txt": "a-contents" }))
        .await;
    let (project_a, worktree_id) = client_a.build_local_project("/dir", cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // See that a guest has joined as client A.
    executor.run_until_parked();

    project_a.read_with(cx_a, |p, _| assert_eq!(p.collaborators().len(), 1));

    // Begin opening a buffer as client B, but leave the project before the open completes.
    let open_buffer = project_b.update(cx_b, |p, cx| {
        p.open_buffer((worktree_id, rel_path("a.txt")), cx)
    });
    let buffer_b = cx_b.executor().spawn(open_buffer);
    cx_b.update(|_| drop(project_b));
    drop(buffer_b);

    // See that the guest has left.
    executor.run_until_parked();

    project_a.read_with(cx_a, |p, _| assert!(p.collaborators().is_empty()));
}

#[gpui::test(iterations = 10)]
async fn test_canceling_buffer_opening(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(
            "/dir",
            json!({
                "a.txt": "abc",
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project("/dir", cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();

    // Open a buffer as client B but cancel after a random amount of time.
    let buffer_b = project_b.update(cx_b, |p, cx| {
        p.open_buffer_by_id(buffer_a.read_with(cx_a, |a, _| a.remote_id()), cx)
    });
    executor.simulate_random_delay().await;
    drop(buffer_b);

    // Try opening the same buffer again as client B, and ensure we can
    // still do it despite the cancellation above.
    let buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer_by_id(buffer_a.read_with(cx_a, |a, _| a.remote_id()), cx)
        })
        .await
        .unwrap();

    buffer_b.read_with(cx_b, |buf, _| assert_eq!(buf.text(), "abc"));
}

#[gpui::test(iterations = 10)]
async fn test_leaving_project(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b), (&client_c, cx_c)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(
            "/a",
            json!({
                "a.txt": "a-contents",
                "b.txt": "b-contents",
            }),
        )
        .await;
    let (project_a, _) = client_a.build_local_project("/a", cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b1 = client_b.join_remote_project(project_id, cx_b).await;
    let project_c = client_c.join_remote_project(project_id, cx_c).await;

    // Client A sees that a guest has joined.
    executor.run_until_parked();

    project_a.read_with(cx_a, |project, _| {
        assert_eq!(project.collaborators().len(), 2);
    });

    project_b1.read_with(cx_b, |project, _| {
        assert_eq!(project.collaborators().len(), 2);
    });

    project_c.read_with(cx_c, |project, _| {
        assert_eq!(project.collaborators().len(), 2);
    });

    // Client B opens a buffer.
    let buffer_b1 = project_b1
        .update(cx_b, |project, cx| {
            let worktree_id = project.worktrees(cx).next().unwrap().read(cx).id();
            project.open_buffer((worktree_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();

    buffer_b1.read_with(cx_b, |buffer, _| assert_eq!(buffer.text(), "a-contents"));

    // Drop client B's project and ensure client A and client C observe client B leaving.
    cx_b.update(|_| drop(project_b1));
    executor.run_until_parked();

    project_a.read_with(cx_a, |project, _| {
        assert_eq!(project.collaborators().len(), 1);
    });

    project_c.read_with(cx_c, |project, _| {
        assert_eq!(project.collaborators().len(), 1);
    });

    // Client B re-joins the project and can open buffers as before.
    let project_b2 = client_b.join_remote_project(project_id, cx_b).await;
    executor.run_until_parked();

    project_a.read_with(cx_a, |project, _| {
        assert_eq!(project.collaborators().len(), 2);
    });

    project_b2.read_with(cx_b, |project, _| {
        assert_eq!(project.collaborators().len(), 2);
    });

    project_c.read_with(cx_c, |project, _| {
        assert_eq!(project.collaborators().len(), 2);
    });

    let buffer_b2 = project_b2
        .update(cx_b, |project, cx| {
            let worktree_id = project.worktrees(cx).next().unwrap().read(cx).id();
            project.open_buffer((worktree_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();

    buffer_b2.read_with(cx_b, |buffer, _| assert_eq!(buffer.text(), "a-contents"));

    project_a.read_with(cx_a, |project, _| {
        assert_eq!(project.collaborators().len(), 2);
    });

    // Drop client B's connection and ensure client A and client C observe client B leaving.
    client_b.disconnect(&cx_b.to_async());
    executor.advance_clock(RECONNECT_TIMEOUT);

    project_a.read_with(cx_a, |project, _| {
        assert_eq!(project.collaborators().len(), 1);
    });

    project_b2.read_with(cx_b, |project, cx| {
        assert!(project.is_disconnected(cx));
    });

    project_c.read_with(cx_c, |project, _| {
        assert_eq!(project.collaborators().len(), 1);
    });

    // Client B can't join the project, unless they re-join the room.
    cx_b.spawn(|cx| {
        Project::in_room(
            project_id,
            client_b.app_state.client.clone(),
            client_b.user_store().clone(),
            client_b.language_registry().clone(),
            FakeFs::new(cx.background_executor().clone()),
            cx,
        )
    })
    .await
    .unwrap_err();

    // Simulate connection loss for client C and ensure client A observes client C leaving the project.
    client_c.wait_for_current_user(cx_c).await;
    server.forbid_connections();
    server.disconnect_client(client_c.peer_id().unwrap());
    executor.advance_clock(RECEIVE_TIMEOUT + RECONNECT_TIMEOUT);
    executor.run_until_parked();

    project_a.read_with(cx_a, |project, _| {
        assert_eq!(project.collaborators().len(), 0);
    });

    project_b2.read_with(cx_b, |project, cx| {
        assert!(project.is_disconnected(cx));
    });

    project_c.read_with(cx_c, |project, cx| {
        assert!(project.is_disconnected(cx));
    });
}
