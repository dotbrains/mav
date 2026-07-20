use crate::{RoomParticipants, TestServer, following_tests::join_channel, room_participants};
use call::ActiveCall;
use collections::HashSet;
use futures::StreamExt as _;
use gpui::{
    App, BackgroundExecutor, Modifiers, MouseButton, MouseDownEvent, TestAppContext, px, size,
};
use pretty_assertions::assert_eq;
use project::ProjectPath;
use serde_json::json;
use std::{path::Path, time::Duration};
use util::{path, rel_path::rel_path};
use workspace::Pane;

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

#[gpui::test(iterations = 10)]
async fn test_join_call_after_screen_was_shared(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;

    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    // Call users B and C from client A.
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();

    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: Default::default(),
            pending: vec!["user_b".to_string()]
        }
    );

    // User B receives the call.

    let mut incoming_call_b = active_call_b.read_with(cx_b, |call, _| call.incoming());
    let call_b = incoming_call_b.next().await.unwrap().unwrap();
    assert_eq!(call_b.calling_user.username, "user_a");

    // User A shares their screen
    let display = gpui::TestScreenCaptureSource::new();
    cx_a.set_screen_capture_sources(vec![display]);
    let screen_a = cx_a
        .update(|cx| cx.screen_capture_sources())
        .await
        .unwrap()
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    active_call_a
        .update(cx_a, |call, cx| {
            call.room()
                .unwrap()
                .update(cx, |room, cx| room.share_screen(screen_a, cx))
        })
        .await
        .unwrap();

    client_b.user_store().update(cx_b, |user_store, _| {
        user_store.clear_cache();
    });

    // User B joins the room
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();

    let room_b = active_call_b.read_with(cx_b, |call, _| call.room().unwrap().clone());
    assert!(incoming_call_b.next().await.unwrap().is_none());

    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string()],
            pending: vec![],
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string()],
            pending: vec![],
        }
    );

    // Ensure User B sees User A's screenshare.

    room_b.read_with(cx_b, |room, _| {
        assert_eq!(
            room.remote_participants()
                .get(&client_a.user_id().unwrap())
                .unwrap()
                .video_tracks
                .len(),
            1
        );
    });
}

#[cfg(target_os = "linux")]
#[gpui::test(iterations = 10)]
async fn test_share_screen_wayland(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;

    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    // User A calls user B.
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();

    // User B accepts.
    let mut incoming_call_b = active_call_b.read_with(cx_b, |call, _| call.incoming());
    executor.run_until_parked();
    incoming_call_b.next().await.unwrap().unwrap();
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();

    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    let room_b = active_call_b.read_with(cx_b, |call, _| call.room().unwrap().clone());
    executor.run_until_parked();

    // User A shares their screen via the Wayland path.
    let events_b = active_call_events(cx_b);
    active_call_a
        .update(cx_a, |call, cx| {
            call.room()
                .unwrap()
                .update(cx, |room, cx| room.share_screen_wayland(cx))
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // Room A is sharing and has a nonzero synthetic screen ID.
    room_a.read_with(cx_a, |room, _| {
        assert!(room.is_sharing_screen());
        let screen_id = room.shared_screen_id();
        assert!(screen_id.is_some(), "shared_screen_id should be Some");
        assert_ne!(screen_id.unwrap(), 0, "synthetic ID must be nonzero");
    });

    // User B observes the remote screen sharing track.
    assert_eq!(events_b.borrow().len(), 1);
    if let call::room::Event::RemoteVideoTracksChanged { participant_id } =
        events_b.borrow().first().unwrap()
    {
        assert_eq!(*participant_id, client_a.peer_id().unwrap());
        room_b.read_with(cx_b, |room, _| {
            assert_eq!(
                room.remote_participants()[&client_a.user_id().unwrap()]
                    .video_tracks
                    .len(),
                1
            );
        });
    } else {
        panic!("expected RemoteVideoTracksChanged event");
    }
}

#[cfg(target_os = "linux")]
#[gpui::test(iterations = 10)]
async fn test_unshare_screen_wayland(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;

    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    // User A calls user B.
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();

    // User B accepts.
    let mut incoming_call_b = active_call_b.read_with(cx_b, |call, _| call.incoming());
    executor.run_until_parked();
    incoming_call_b.next().await.unwrap().unwrap();
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();

    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    executor.run_until_parked();

    // User A shares their screen via the Wayland path.
    active_call_a
        .update(cx_a, |call, cx| {
            call.room()
                .unwrap()
                .update(cx, |room, cx| room.share_screen_wayland(cx))
        })
        .await
        .unwrap();
    executor.run_until_parked();

    room_a.read_with(cx_a, |room, _| {
        assert!(room.is_sharing_screen());
    });

    // User A stops sharing.
    room_a
        .update(cx_a, |room, cx| room.unshare_screen(true, cx))
        .unwrap();
    executor.run_until_parked();

    // Room A is no longer sharing, screen ID is gone.
    room_a.read_with(cx_a, |room, _| {
        assert!(!room.is_sharing_screen());
        assert!(room.shared_screen_id().is_none());
    });
}

#[gpui::test]
async fn test_right_click_menu_behind_collab_panel(cx: &mut TestAppContext) {
    let mut server = TestServer::start(cx.executor().clone()).await;
    let client_a = server.create_client(cx, "user_a").await;
    let (_workspace_a, cx) = client_a.build_test_workspace(cx).await;

    cx.simulate_resize(size(px(300.), px(300.)));

    cx.simulate_keystrokes("cmd-n cmd-n cmd-n");
    cx.update(|window, _cx| window.refresh());

    let new_tab_button_bounds = cx.debug_bounds("ICON-Plus").unwrap();

    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position: new_tab_button_bounds.center(),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });

    // regression test that the right click menu for tabs does not open.
    assert!(cx.debug_bounds("MENU_ITEM-Close").is_none());

    let tab_bounds = cx.debug_bounds("TAB-1").unwrap();
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position: tab_bounds.center(),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    assert!(cx.debug_bounds("MENU_ITEM-Close").is_some());
}

#[gpui::test]
async fn test_pane_split_left(cx: &mut TestAppContext) {
    let (_, client) = TestServer::start1(cx).await;
    let (workspace, cx) = client.build_test_workspace(cx).await;

    cx.simulate_keystrokes("cmd-n");
    workspace.update(cx, |workspace, cx| {
        assert!(workspace.items(cx).collect::<Vec<_>>().len() == 1);
    });
    cx.simulate_keystrokes("cmd-k left");
    workspace.update(cx, |workspace, cx| {
        assert!(workspace.items(cx).collect::<Vec<_>>().len() == 2);
    });
    cx.simulate_keystrokes("cmd-k");
    // Sleep past the historical timeout to ensure the multi-stroke binding
    // still fires now that unambiguous prefixes no longer auto-expire.
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.simulate_keystrokes("left");
    workspace.update(cx, |workspace, cx| {
        assert!(workspace.items(cx).collect::<Vec<_>>().len() == 3);
    });
}

#[gpui::test]
async fn test_join_after_restart(cx1: &mut TestAppContext, cx2: &mut TestAppContext) {
    let (mut server, client) = TestServer::start1(cx1).await;
    let channel1 = server.make_public_channel("channel1", &client, cx1).await;
    let channel2 = server.make_public_channel("channel2", &client, cx1).await;

    join_channel(channel1, &client, cx1).await.unwrap();
    drop(client);

    let client2 = server.create_client(cx2, "user_a").await;
    join_channel(channel2, &client2, cx2).await.unwrap();
}

#[gpui::test]
async fn test_preview_tabs(cx: &mut TestAppContext) {
    let (_server, client) = TestServer::start1(cx).await;
    let (workspace, cx) = client.build_test_workspace(cx).await;
    let project = workspace.read_with(cx, |workspace, _| workspace.project().clone());

    let worktree_id = project.update(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });

    let path_1 = ProjectPath {
        worktree_id,
        path: rel_path("1.txt").into(),
    };
    let path_2 = ProjectPath {
        worktree_id,
        path: rel_path("2.js").into(),
    };
    let path_3 = ProjectPath {
        worktree_id,
        path: rel_path("3.rs").into(),
    };

    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let get_path = |pane: &Pane, idx: usize, cx: &App| {
        pane.item_for_index(idx).unwrap().project_path(cx).unwrap()
    };

    // Opening item 3 as a "permanent" tab
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(path_3.clone(), None, false, window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_3.clone());
        assert_eq!(pane.preview_item_id(), None);

        assert!(!pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    // Open item 1 as preview
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path_preview(path_1.clone(), None, true, true, true, window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 2);
        assert_eq!(get_path(pane, 0, cx), path_3.clone());
        assert_eq!(get_path(pane, 1, cx), path_1.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().nth(1).unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    // Open item 2 as preview
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path_preview(path_2.clone(), None, true, true, true, window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 2);
        assert_eq!(get_path(pane, 0, cx), path_3.clone());
        assert_eq!(get_path(pane, 1, cx), path_2.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().nth(1).unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    // Going back should show item 1 as preview
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.go_back(pane.downgrade(), window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 2);
        assert_eq!(get_path(pane, 0, cx), path_3.clone());
        assert_eq!(get_path(pane, 1, cx), path_1.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().nth(1).unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(pane.can_navigate_forward());
    });

    // Closing item 1
    pane.update_in(cx, |pane, window, cx| {
        pane.close_item_by_id(
            pane.active_item().unwrap().item_id(),
            workspace::SaveIntent::Skip,
            window,
            cx,
        )
    })
    .await
    .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_3.clone());
        assert_eq!(pane.preview_item_id(), None);

        assert!(pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    // Going back should show item 1 as preview
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.go_back(pane.downgrade(), window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 2);
        assert_eq!(get_path(pane, 0, cx), path_3.clone());
        assert_eq!(get_path(pane, 1, cx), path_1.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().nth(1).unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(pane.can_navigate_forward());
    });

    // Close permanent tab
    pane.update_in(cx, |pane, window, cx| {
        let id = pane.items().next().unwrap().item_id();
        pane.close_item_by_id(id, workspace::SaveIntent::Skip, window, cx)
    })
    .await
    .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_1.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().next().unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(pane.can_navigate_forward());
    });

    // Split pane to the right
    pane.update_in(cx, |pane, window, cx| {
        pane.split(
            workspace::SplitDirection::Right,
            workspace::SplitMode::default(),
            window,
            cx,
        );
    });
    cx.run_until_parked();
    let right_pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    right_pane.update(cx, |pane, cx| {
        // Nav history is now cloned in an pane split, but that's inconvenient
        // for this test, which uses the presence of a backwards history item as
        // an indication that a preview item was successfully opened
        pane.nav_history_mut().clear(cx);
    });

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_1.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().next().unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(pane.can_navigate_forward());
    });

    right_pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_1.clone());
        assert_eq!(pane.preview_item_id(), None);

        assert!(!pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    // Open item 2 as preview in right pane
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path_preview(path_2.clone(), None, true, true, true, window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_1.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().next().unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(pane.can_navigate_forward());
    });

    right_pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 2);
        assert_eq!(get_path(pane, 0, cx), path_1.clone());
        assert_eq!(get_path(pane, 1, cx), path_2.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().nth(1).unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    // Focus left pane
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.activate_pane_in_direction(workspace::SplitDirection::Left, window, cx)
    });

    // Open item 2 as preview in left pane
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path_preview(path_2.clone(), None, true, true, true, window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_2.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().next().unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    right_pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 2);
        assert_eq!(get_path(pane, 0, cx), path_1.clone());
        assert_eq!(get_path(pane, 1, cx), path_2.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().nth(1).unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });
}

#[gpui::test]
async fn test_remote_git_branches(
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
        .insert_tree("/project", serde_json::json!({ ".git":{} }))
        .await;
    let branches = ["main", "dev", "feature-1"];
    client_a
        .fs()
        .insert_branches(Path::new("/project/.git"), &branches);
    let branches_set = branches
        .into_iter()
        .map(ToString::to_string)
        .collect::<HashSet<_>>();

    let (project_a, _) = client_a.build_local_project("/project", cx_a).await;

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Client A sees that a guest has joined and the repo has been populated
    executor.run_until_parked();

    let repo_b = cx_b.update(|cx| project_b.read(cx).active_repository(cx).unwrap());

    let branches_b = cx_b
        .update(|cx| repo_b.update(cx, |repository, _| repository.branches()))
        .await
        .unwrap()
        .unwrap();

    let new_branch = branches[2];

    let branches_b = branches_b
        .branches
        .into_iter()
        .map(|branch| branch.name().to_string())
        .collect::<HashSet<_>>();

    assert_eq!(branches_b, branches_set);

    cx_b.update(|cx| {
        repo_b.update(cx, |repository, _cx| {
            repository.change_branch(new_branch.to_string())
        })
    })
    .await
    .unwrap()
    .unwrap();

    executor.run_until_parked();

    let host_branch = cx_a.update(|cx| {
        project_a.update(cx, |project, cx| {
            project
                .repositories(cx)
                .values()
                .next()
                .unwrap()
                .read(cx)
                .branch
                .as_ref()
                .unwrap()
                .clone()
        })
    });

    assert_eq!(host_branch.name(), branches[2]);

    // Also try creating a new branch
    cx_b.update(|cx| {
        repo_b.update(cx, |repository, _cx| {
            repository.create_branch("totally-new-branch".to_string(), None)
        })
    })
    .await
    .unwrap()
    .unwrap();

    cx_b.update(|cx| {
        repo_b.update(cx, |repository, _cx| {
            repository.change_branch("totally-new-branch".to_string())
        })
    })
    .await
    .unwrap()
    .unwrap();

    executor.run_until_parked();

    let host_branch = cx_a.update(|cx| {
        project_a.update(cx, |project, cx| {
            project
                .repositories(cx)
                .values()
                .next()
                .unwrap()
                .read(cx)
                .branch
                .as_ref()
                .unwrap()
                .clone()
        })
    });

    assert_eq!(host_branch.name(), "totally-new-branch");
}

#[gpui::test]
async fn test_guest_can_rejoin_shared_project_after_leaving_call(
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

    client_a
        .fs()
        .insert_tree(
            path!("/project"),
            json!({
                "file.txt": "hello\n",
            }),
        )
        .await;

    let (project_a, _worktree_id) = client_a.build_local_project(path!("/project"), cx_a).await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let _project_b = client_b.join_remote_project(project_id, cx_b).await;
    executor.run_until_parked();

    // third client joins call to prevent room from being torn down
    let _project_c = client_c.join_remote_project(project_id, cx_c).await;
    executor.run_until_parked();

    let active_call_b = cx_b.read(ActiveCall::global);
    active_call_b
        .update(cx_b, |call, cx| call.hang_up(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    let user_id_b = client_b.current_user_id(cx_b).to_proto();
    let active_call_a = cx_a.read(ActiveCall::global);
    active_call_a
        .update(cx_a, |call, cx| call.invite(user_id_b, None, cx))
        .await
        .unwrap();
    executor.run_until_parked();
    let active_call_b = cx_b.read(ActiveCall::global);
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    let _project_b2 = client_b.join_remote_project(project_id, cx_b).await;
    executor.run_until_parked();

    project_a.read_with(cx_a, |project, _| {
        let guest_count = project
            .collaborators()
            .values()
            .filter(|c| !c.is_host)
            .count();

        assert_eq!(
            guest_count, 2,
            "host should have exactly one guest collaborator after rejoin"
        );
    });

    _project_b.read_with(cx_b, |project, _| {
        assert_eq!(
            project.client_subscriptions().len(),
            0,
            "We should clear all host subscriptions after leaving the project"
        );
    })
}
