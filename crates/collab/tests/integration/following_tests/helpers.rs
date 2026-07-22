use super::*;

fn visible_push_notifications(cx: &mut TestAppContext) -> Vec<Entity<ProjectSharedNotification>> {
    let mut ret = Vec::new();
    for window in cx.windows() {
        window
            .update(cx, |window, _, _| {
                if let Ok(handle) = window.downcast::<ProjectSharedNotification>() {
                    ret.push(handle)
                }
            })
            .unwrap();
    }
    ret
}

#[derive(Debug, PartialEq, Eq)]
struct PaneSummary {
    active: bool,
    leader: Option<PeerId>,
    items: Vec<(bool, String)>,
}

fn followers_by_leader(project_id: u64, cx: &TestAppContext) -> Vec<(PeerId, Vec<PeerId>)> {
    cx.read(|cx| {
        let active_call = ActiveCall::global(cx).read(cx);
        let peer_id = active_call.client().peer_id();
        let room = active_call.room().unwrap().read(cx);
        let mut result = room
            .remote_participants()
            .values()
            .map(|participant| participant.peer_id)
            .chain(peer_id)
            .filter_map(|peer_id| {
                let followers = room.followers_for(peer_id, project_id);
                if followers.is_empty() {
                    None
                } else {
                    Some((peer_id, followers.to_vec()))
                }
            })
            .collect::<Vec<_>>();
        result.sort_by_key(|e| e.0);
        result
    })
}

fn pane_summaries(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<PaneSummary> {
    workspace.update(cx, |workspace, cx| {
        let active_pane = workspace.active_pane();
        workspace
            .panes()
            .iter()
            .map(|pane| {
                let leader = match workspace.leader_for_pane(pane) {
                    Some(CollaboratorId::PeerId(peer_id)) => Some(peer_id),
                    Some(CollaboratorId::Agent) => unimplemented!(),
                    None => None,
                };
                let active = pane == active_pane;
                let pane = pane.read(cx);
                let active_ix = pane.active_item_index();
                PaneSummary {
                    active,
                    leader,
                    items: pane
                        .items()
                        .enumerate()
                        .map(|(ix, item)| (ix == active_ix, item.tab_content_text(0, cx).into()))
                        .collect(),
                }
            })
            .collect()
    })
}

pub(crate) async fn join_channel(
    channel_id: ChannelId,
    client: &TestClient,
    cx: &mut TestAppContext,
) -> anyhow::Result<()> {
    cx.update(|cx| workspace::join_channel(channel_id, client.app_state.clone(), None, None, cx))
        .await
}

async fn share_workspace(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> anyhow::Result<u64> {
    let project = workspace.read_with(cx, |workspace, _| workspace.project().clone());
    cx.read(ActiveCall::global)
        .update(cx, |call, cx| call.share_project(project, cx))
        .await
}

pub(super) async fn exercise_screen_share_following(
    executor: &BackgroundExecutor,
    active_call_b: &Entity<ActiveCall>,
    client_b: &TestClient,
    project_b: &Entity<Project>,
    workspace_a: &Entity<Workspace>,
    cx_a: &mut VisualTestContext,
    workspace_b: &Entity<Workspace>,
    cx_b: &mut VisualTestContext,
    editor_a1: &Entity<Editor>,
    multibuffer_editor_a: &Entity<Editor>,
    multibuffer_editor_b: &Entity<Editor>,
    pane_a: &Entity<Pane>,
) {
    use collab::rpc::RECONNECT_TIMEOUT;
    use gpui::TestScreenCaptureSource;
    use workspace::{
        dock::{DockPosition, test::TestPanel},
        item::test::TestItem,
        shared_screen::SharedScreen,
    };

    use collab::rpc::RECONNECT_TIMEOUT;
    use gpui::TestScreenCaptureSource;
    use workspace::{
        dock::{DockPosition, test::TestPanel},
        item::test::TestItem,
        shared_screen::SharedScreen,
    };

    // Client B activates an external window, which causes a new screen-sharing item to be added to the pane.
    let display = TestScreenCaptureSource::new();
    active_call_b
        .update(cx_b, |call, cx| call.set_location(None, cx))
        .await
        .unwrap();
    cx_b.set_screen_capture_sources(vec![display]);
    let source = cx_b
        .read(|cx| cx.screen_capture_sources())
        .await
        .unwrap()
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    active_call_b
        .update(cx_b, |call, cx| {
            call.room()
                .unwrap()
                .update(cx, |room, cx| room.share_screen(source, cx))
        })
        .await
        .unwrap();
    executor.run_until_parked();

    let shared_screen = workspace_a.update(cx_a, |workspace, cx| {
        workspace
            .active_item(cx)
            .expect("no active item")
            .downcast::<SharedScreen>()
            .expect("active item isn't a shared screen")
    });

    // Client B activates Mav again, which causes the previous editor to become focused again.
    active_call_b
        .update(cx_b, |call, cx| call.set_location(Some(&project_b), cx))
        .await
        .unwrap();
    executor.run_until_parked();
    workspace_a.update(cx_a, |workspace, cx| {
        assert_eq!(
            workspace.active_item(cx).unwrap().item_id(),
            editor_a1.item_id()
        )
    });

    // Client B activates a multibuffer that was created by following client A. Client A returns to that multibuffer.
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.activate_item(&multibuffer_editor_b, true, true, window, cx)
    });
    executor.run_until_parked();
    workspace_a.update(cx_a, |workspace, cx| {
        assert_eq!(
            workspace.active_item(cx).unwrap().item_id(),
            multibuffer_editor_a.item_id()
        )
    });

    // Client B activates a panel, and the previously-opened screen-sharing item gets activated.
    let panel = cx_b.new(|cx| TestPanel::new(DockPosition::Left, 100, cx));
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.add_panel(panel, window, cx);
        workspace.toggle_panel_focus::<TestPanel>(window, cx);
    });
    executor.run_until_parked();
    assert_eq!(
        workspace_a.update(cx_a, |workspace, cx| workspace
            .active_item(cx)
            .unwrap()
            .item_id()),
        shared_screen.item_id()
    );

    // Toggling the focus back to the pane causes client A to return to the multibuffer.
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.toggle_panel_focus::<TestPanel>(window, cx);
    });
    executor.run_until_parked();
    workspace_a.update(cx_a, |workspace, cx| {
        assert_eq!(
            workspace.active_item(cx).unwrap().item_id(),
            multibuffer_editor_a.item_id()
        )
    });

    // Client B activates an item that doesn't implement following,
    // so the previously-opened screen-sharing item gets activated.
    let unfollowable_item = cx_b.new(TestItem::new);
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            pane.add_item(Box::new(unfollowable_item), true, true, None, window, cx)
        })
    });
    executor.run_until_parked();
    assert_eq!(
        workspace_a.update(cx_a, |workspace, cx| workspace
            .active_item(cx)
            .unwrap()
            .item_id()),
        shared_screen.item_id()
    );

    // Following interrupts when client B disconnects.
    client_b.disconnect(&cx_b.to_async());
    executor.advance_clock(RECONNECT_TIMEOUT);
    assert_eq!(
        workspace_a.update(cx_a, |workspace, _| workspace.leader_for_pane(&pane_a)),
        None
    );
}

pub(super) fn assert_followed_tab_rotation(
    executor: &BackgroundExecutor,
    client_a: &TestClient,
    workspace_a: &Entity<Workspace>,
    cx_a: &mut VisualTestContext,
    workspace_b: &Entity<Workspace>,
    cx_b: &mut VisualTestContext,
) {
    // Client B follows client A into those tabs.
    assert_eq!(
        pane_summaries(&workspace_a, cx_a),
        &[
            PaneSummary {
                active: false,
                leader: None,
                items: vec![(false, "1.txt".into()), (true, "3.txt".into())]
            },
            PaneSummary {
                active: true,
                leader: None,
                items: vec![
                    (false, "1.txt".into()),
                    (false, "2.txt".into()),
                    (true, "4.txt".into()),
                    (false, "3.txt".into()),
                ]
            },
        ]
    );
    assert_eq!(
        pane_summaries(&workspace_b, cx_b),
        &[
            PaneSummary {
                active: false,
                leader: None,
                items: vec![(false, "2.txt".into()), (true, "4.txt".into())]
            },
            PaneSummary {
                active: true,
                leader: client_a.peer_id(),
                items: vec![(false, "3.txt".into()), (true, "4.txt".into())]
            },
        ]
    );

    workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            pane.activate_previous_item(&Default::default(), window, cx);
        });
    });
    executor.run_until_parked();

    assert_eq!(
        pane_summaries(&workspace_a, cx_a),
        &[
            PaneSummary {
                active: false,
                leader: None,
                items: vec![(false, "1.txt".into()), (true, "3.txt".into())]
            },
            PaneSummary {
                active: true,
                leader: None,
                items: vec![
                    (false, "1.txt".into()),
                    (true, "2.txt".into()),
                    (false, "4.txt".into()),
                    (false, "3.txt".into()),
                ]
            },
        ]
    );
    assert_eq!(
        pane_summaries(&workspace_b, cx_b),
        &[
            PaneSummary {
                active: false,
                leader: None,
                items: vec![(false, "2.txt".into()), (true, "4.txt".into())]
            },
            PaneSummary {
                active: true,
                leader: client_a.peer_id(),
                items: vec![
                    (false, "3.txt".into()),
                    (false, "4.txt".into()),
                    (true, "2.txt".into())
                ]
            },
        ]
    );

    workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            pane.activate_previous_item(&Default::default(), window, cx);
        });
    });
    executor.run_until_parked();

    assert_eq!(
        pane_summaries(&workspace_a, cx_a),
        &[
            PaneSummary {
                active: false,
                leader: None,
                items: vec![(false, "1.txt".into()), (true, "3.txt".into())]
            },
            PaneSummary {
                active: true,
                leader: None,
                items: vec![
                    (true, "1.txt".into()),
                    (false, "2.txt".into()),
                    (false, "4.txt".into()),
                    (false, "3.txt".into()),
                ]
            },
        ]
    );
    assert_eq!(
        pane_summaries(&workspace_b, cx_b),
        &[
            PaneSummary {
                active: false,
                leader: None,
                items: vec![(false, "2.txt".into()), (true, "4.txt".into())]
            },
            PaneSummary {
                active: true,
                leader: client_a.peer_id(),
                items: vec![
                    (false, "3.txt".into()),
                    (false, "4.txt".into()),
                    (false, "2.txt".into()),
                    (true, "1.txt".into()),
                ]
            },
        ]
    );
}
