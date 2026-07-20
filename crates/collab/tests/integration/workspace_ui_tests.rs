use gpui::{App, Modifiers, MouseButton, MouseDownEvent, TestAppContext, px, size};
use pretty_assertions::assert_eq;
use project::ProjectPath;
use std::time::Duration;
use util::rel_path::rel_path;
use workspace::Pane;

use crate::{TestServer, following_tests::join_channel};

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
