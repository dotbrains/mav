use call::ActiveCall;
use client::RECEIVE_TIMEOUT;
use fs::{Fs as _, RemoveOptions};
use gpui::{BackgroundExecutor, TestAppContext};
use pretty_assertions::assert_eq;
use serde_json::json;
use util::{path, rel_path::rel_path};

use crate::TestServer;

#[gpui::test(iterations = 10)]
async fn test_unshare_project(
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
    let active_call_b = cx_b.read(ActiveCall::global);

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

    let (project_a, worktree_id) = client_a.build_local_project("/a", cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let worktree_a = project_a.read_with(cx_a, |project, cx| project.worktrees(cx).next().unwrap());
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    executor.run_until_parked();

    assert!(worktree_a.read_with(cx_a, |tree, _| tree.has_update_observer()));

    project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();

    // When client B leaves the room, the project becomes read-only.
    active_call_b
        .update(cx_b, |call, cx| call.hang_up(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    assert!(project_b.read_with(cx_b, |project, cx| project.is_disconnected(cx)));

    // Client C opens the project.
    let project_c = client_c.join_remote_project(project_id, cx_c).await;

    // When client A unshares the project, client C's project becomes read-only.
    project_a
        .update(cx_a, |project, cx| project.unshare(cx))
        .unwrap();
    executor.run_until_parked();

    assert!(worktree_a.read_with(cx_a, |tree, _| !tree.has_update_observer()));

    assert!(project_c.read_with(cx_c, |project, cx| project.is_disconnected(cx)));

    // Client C can open the project again after client A re-shares.
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_c2 = client_c.join_remote_project(project_id, cx_c).await;
    executor.run_until_parked();

    assert!(worktree_a.read_with(cx_a, |tree, _| tree.has_update_observer()));
    project_c2
        .update(cx_c, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();

    // When client A (the host) leaves the room, the project gets unshared and guests are notified.
    active_call_a
        .update(cx_a, |call, cx| call.hang_up(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    project_a.read_with(cx_a, |project, _| assert!(!project.is_shared()));

    project_c2.read_with(cx_c, |project, cx| {
        assert!(project.is_disconnected(cx));
        assert!(project.collaborators().is_empty());
    });
}

#[gpui::test(iterations = 10)]
async fn test_project_reconnect(
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

    cx_b.update(editor::init);

    client_a
        .fs()
        .insert_tree(
            path!("/root-1"),
            json!({
                "dir1": {
                    "a.txt": "a",
                    "b.txt": "b",
                    "subdir1": {
                        "c.txt": "c",
                        "d.txt": "d",
                        "e.txt": "e",
                    }
                },
                "dir2": {
                    "v.txt": "v",
                },
                "dir3": {
                    "w.txt": "w",
                    "x.txt": "x",
                    "y.txt": "y",
                },
                "dir4": {
                    "z.txt": "z",
                },
            }),
        )
        .await;
    client_a
        .fs()
        .insert_tree(
            path!("/root-2"),
            json!({
                "2.txt": "2",
            }),
        )
        .await;
    client_a
        .fs()
        .insert_tree(
            path!("/root-3"),
            json!({
                "3.txt": "3",
            }),
        )
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let (project_a1, _) = client_a
        .build_local_project(path!("/root-1/dir1"), cx_a)
        .await;
    let (project_a2, _) = client_a.build_local_project(path!("/root-2"), cx_a).await;
    let (project_a3, _) = client_a.build_local_project(path!("/root-3"), cx_a).await;
    let worktree_a1 =
        project_a1.read_with(cx_a, |project, cx| project.worktrees(cx).next().unwrap());
    let project1_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a1.clone(), cx))
        .await
        .unwrap();
    let project2_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a2.clone(), cx))
        .await
        .unwrap();
    let project3_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a3.clone(), cx))
        .await
        .unwrap();

    let project_b1 = client_b.join_remote_project(project1_id, cx_b).await;
    let project_b2 = client_b.join_remote_project(project2_id, cx_b).await;
    let project_b3 = client_b.join_remote_project(project3_id, cx_b).await;
    executor.run_until_parked();

    let worktree1_id = worktree_a1.read_with(cx_a, |worktree, _| {
        assert!(worktree.has_update_observer());
        worktree.id()
    });
    let (worktree_a2, _) = project_a1
        .update(cx_a, |p, cx| {
            p.find_or_create_worktree(path!("/root-1/dir2"), true, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();

    let worktree2_id = worktree_a2.read_with(cx_a, |tree, _| {
        assert!(tree.has_update_observer());
        tree.id()
    });
    executor.run_until_parked();

    project_b1.read_with(cx_b, |project, cx| {
        assert!(project.worktree_for_id(worktree2_id, cx).is_some())
    });

    let buffer_a1 = project_a1
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree1_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();
    let buffer_b1 = project_b1
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree1_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();

    // Drop client A's connection.
    server.forbid_connections();
    server.disconnect_client(client_a.peer_id().unwrap());
    executor.advance_clock(RECEIVE_TIMEOUT);

    project_a1.read_with(cx_a, |project, _| {
        assert!(project.is_shared());
        assert_eq!(project.collaborators().len(), 1);
    });

    project_b1.read_with(cx_b, |project, cx| {
        assert!(!project.is_disconnected(cx));
        assert_eq!(project.collaborators().len(), 1);
    });

    worktree_a1.read_with(cx_a, |tree, _| assert!(tree.has_update_observer()));

    // While client A is disconnected, add and remove files from client A's project.
    client_a
        .fs()
        .insert_tree(
            path!("/root-1/dir1/subdir2"),
            json!({
                "f.txt": "f-contents",
                "g.txt": "g-contents",
                "h.txt": "h-contents",
                "i.txt": "i-contents",
            }),
        )
        .await;
    client_a
        .fs()
        .remove_dir(
            path!("/root-1/dir1/subdir1").as_ref(),
            RemoveOptions {
                recursive: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // While client A is disconnected, add and remove worktrees from client A's project.
    project_a1.update(cx_a, |project, cx| {
        project.remove_worktree(worktree2_id, cx)
    });
    let (worktree_a3, _) = project_a1
        .update(cx_a, |p, cx| {
            p.find_or_create_worktree(path!("/root-1/dir3"), true, cx)
        })
        .await
        .unwrap();
    worktree_a3
        .read_with(cx_a, |tree, _| tree.as_local().unwrap().scan_complete())
        .await;

    let worktree3_id = worktree_a3.read_with(cx_a, |tree, _| {
        assert!(!tree.has_update_observer());
        tree.id()
    });
    executor.run_until_parked();

    // While client A is disconnected, close project 2
    cx_a.update(|_| drop(project_a2));

    // While client A is disconnected, mutate a buffer on both the host and the guest.
    buffer_a1.update(cx_a, |buf, cx| buf.edit([(0..0, "W")], None, cx));
    buffer_b1.update(cx_b, |buf, cx| buf.edit([(1..1, "Z")], None, cx));
    executor.run_until_parked();

    // Client A reconnects. Their project is re-shared, and client B re-joins it.
    server.allow_connections();
    client_a
        .connect(false, &cx_a.to_async())
        .await
        .into_response()
        .unwrap();
    executor.run_until_parked();

    project_a1.read_with(cx_a, |project, cx| {
        assert!(project.is_shared());
        assert!(worktree_a1.read(cx).has_update_observer());
        assert_eq!(
            worktree_a1.read(cx).snapshot().paths().collect::<Vec<_>>(),
            vec![
                rel_path("a.txt"),
                rel_path("b.txt"),
                rel_path("subdir2"),
                rel_path("subdir2/f.txt"),
                rel_path("subdir2/g.txt"),
                rel_path("subdir2/h.txt"),
                rel_path("subdir2/i.txt")
            ]
        );
        assert!(worktree_a3.read(cx).has_update_observer());
        assert_eq!(
            worktree_a3.read(cx).snapshot().paths().collect::<Vec<_>>(),
            vec![rel_path("w.txt"), rel_path("x.txt"), rel_path("y.txt")]
        );
    });

    project_b1.read_with(cx_b, |project, cx| {
        assert!(!project.is_disconnected(cx));
        assert_eq!(
            project
                .worktree_for_id(worktree1_id, cx)
                .unwrap()
                .read(cx)
                .snapshot()
                .paths()
                .collect::<Vec<_>>(),
            vec![
                rel_path("a.txt"),
                rel_path("b.txt"),
                rel_path("subdir2"),
                rel_path("subdir2/f.txt"),
                rel_path("subdir2/g.txt"),
                rel_path("subdir2/h.txt"),
                rel_path("subdir2/i.txt")
            ]
        );
        assert!(project.worktree_for_id(worktree2_id, cx).is_none());
        assert_eq!(
            project
                .worktree_for_id(worktree3_id, cx)
                .unwrap()
                .read(cx)
                .snapshot()
                .paths()
                .collect::<Vec<_>>(),
            vec![rel_path("w.txt"), rel_path("x.txt"), rel_path("y.txt")]
        );
    });

    project_b2.read_with(cx_b, |project, cx| assert!(project.is_disconnected(cx)));

    project_b3.read_with(cx_b, |project, cx| assert!(!project.is_disconnected(cx)));

    buffer_a1.read_with(cx_a, |buffer, _| assert_eq!(buffer.text(), "WaZ"));

    buffer_b1.read_with(cx_b, |buffer, _| assert_eq!(buffer.text(), "WaZ"));

    // Drop client B's connection.
    server.forbid_connections();
    server.disconnect_client(client_b.peer_id().unwrap());
    executor.advance_clock(RECEIVE_TIMEOUT);

    // While client B is disconnected, add and remove files from client A's project
    client_a
        .fs()
        .insert_file(path!("/root-1/dir1/subdir2/j.txt"), "j-contents".into())
        .await;
    client_a
        .fs()
        .remove_file(
            path!("/root-1/dir1/subdir2/i.txt").as_ref(),
            Default::default(),
        )
        .await
        .unwrap();

    // While client B is disconnected, add and remove worktrees from client A's project.
    let (worktree_a4, _) = project_a1
        .update(cx_a, |p, cx| {
            p.find_or_create_worktree(path!("/root-1/dir4"), true, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();

    let worktree4_id = worktree_a4.read_with(cx_a, |tree, _| {
        assert!(tree.has_update_observer());
        tree.id()
    });
    project_a1.update(cx_a, |project, cx| {
        project.remove_worktree(worktree3_id, cx)
    });
    executor.run_until_parked();

    // While client B is disconnected, mutate a buffer on both the host and the guest.
    buffer_a1.update(cx_a, |buf, cx| buf.edit([(1..1, "X")], None, cx));
    buffer_b1.update(cx_b, |buf, cx| buf.edit([(2..2, "Y")], None, cx));
    executor.run_until_parked();

    // While disconnected, close project 3
    cx_a.update(|_| drop(project_a3));
    executor.run_until_parked();

    // Client B reconnects. They re-join the room and the remaining shared project.
    server.allow_connections();
    client_b
        .connect(false, &cx_b.to_async())
        .await
        .into_response()
        .unwrap();
    executor.run_until_parked();

    project_b1.read_with(cx_b, |project, cx| {
        assert!(!project.is_disconnected(cx));
        assert_eq!(
            project
                .worktree_for_id(worktree1_id, cx)
                .unwrap()
                .read(cx)
                .snapshot()
                .paths()
                .collect::<Vec<_>>(),
            vec![
                rel_path("a.txt"),
                rel_path("b.txt"),
                rel_path("subdir2"),
                rel_path("subdir2/f.txt"),
                rel_path("subdir2/g.txt"),
                rel_path("subdir2/h.txt"),
                rel_path("subdir2/j.txt")
            ]
        );
        assert!(project.worktree_for_id(worktree2_id, cx).is_none());
        assert_eq!(
            project
                .worktree_for_id(worktree4_id, cx)
                .unwrap()
                .read(cx)
                .snapshot()
                .paths()
                .map(|p| p.as_unix_str())
                .collect::<Vec<_>>(),
            vec!["z.txt"]
        );
    });

    project_b3.read_with(cx_b, |project, cx| assert!(project.is_disconnected(cx)));

    buffer_a1.read_with(cx_a, |buffer, _| assert_eq!(buffer.text(), "WXaYZ"));

    buffer_b1.read_with(cx_b, |buffer, _| assert_eq!(buffer.text(), "WXaYZ"));
}
