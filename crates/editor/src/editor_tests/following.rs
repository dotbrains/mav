use super::*;

#[gpui::test]
async fn test_following(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, ["/file.rs".as_ref()], cx).await;

    let buffer = project.update(cx, |project, cx| {
        let buffer = project.create_local_buffer(&sample_text(16, 8, 'a'), None, false, cx);
        cx.new(|cx| MultiBuffer::singleton(buffer, cx))
    });
    let leader = cx.add_window(|window, cx| build_editor(buffer.clone(), window, cx));
    let follower = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::from_corners(
                    gpui::Point::new(px(0.), px(0.)),
                    gpui::Point::new(px(10.), px(80.)),
                ))),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| build_editor(buffer.clone(), window, cx)),
        )
        .unwrap()
    });

    let is_still_following = Rc::new(RefCell::new(true));
    let follower_edit_event_count = Rc::new(RefCell::new(0));
    let pending_update = Rc::new(RefCell::new(None));
    let leader_entity = leader.root(cx).unwrap();
    let follower_entity = follower.root(cx).unwrap();
    _ = follower.update(cx, {
        let update = pending_update.clone();
        let is_still_following = is_still_following.clone();
        let follower_edit_event_count = follower_edit_event_count.clone();
        |_, window, cx| {
            cx.subscribe_in(
                &leader_entity,
                window,
                move |_, leader, event, window, cx| {
                    leader.update(cx, |leader, cx| {
                        leader.add_event_to_update_proto(
                            event,
                            &mut update.borrow_mut(),
                            window,
                            cx,
                        );
                    });
                },
            )
            .detach();

            cx.subscribe_in(
                &follower_entity,
                window,
                move |_, _, event: &EditorEvent, _window, _cx| {
                    if matches!(Editor::to_follow_event(event), Some(FollowEvent::Unfollow)) {
                        *is_still_following.borrow_mut() = false;
                    }

                    if let EditorEvent::BufferEdited = event {
                        *follower_edit_event_count.borrow_mut() += 1;
                    }
                },
            )
            .detach();
        }
    });

    // Update the selections only
    _ = leader.update(cx, |leader, window, cx| {
        leader.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(1)..MultiBufferOffset(1)])
        });
    });
    follower
        .update(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                pending_update.borrow_mut().take().unwrap(),
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .unwrap();
    _ = follower.update(cx, |follower, _, cx| {
        assert_eq!(
            follower.selections.ranges(&follower.display_snapshot(cx)),
            vec![MultiBufferOffset(1)..MultiBufferOffset(1)]
        );
    });
    assert!(*is_still_following.borrow());
    assert_eq!(*follower_edit_event_count.borrow(), 0);

    // Update the scroll position only
    _ = leader.update(cx, |leader, window, cx| {
        leader.set_scroll_position(gpui::Point::new(1.5, 3.5), window, cx);
    });
    follower
        .update(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                pending_update.borrow_mut().take().unwrap(),
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .unwrap();
    assert_eq!(
        follower
            .update(cx, |follower, _, cx| follower.scroll_position(cx))
            .unwrap(),
        gpui::Point::new(1.5, 3.5)
    );
    assert!(*is_still_following.borrow());
    assert_eq!(*follower_edit_event_count.borrow(), 0);

    // Update the selections and scroll position. The follower's scroll position is updated
    // via autoscroll, not via the leader's exact scroll position.
    _ = leader.update(cx, |leader, window, cx| {
        leader.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(0)..MultiBufferOffset(0)])
        });
        leader.request_autoscroll(Autoscroll::newest(), cx);
        leader.set_scroll_position(gpui::Point::new(1.5, 3.5), window, cx);
    });
    follower
        .update(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                pending_update.borrow_mut().take().unwrap(),
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .unwrap();
    _ = follower.update(cx, |follower, _, cx| {
        assert_eq!(follower.scroll_position(cx), gpui::Point::new(1.5, 0.0));
        assert_eq!(
            follower.selections.ranges(&follower.display_snapshot(cx)),
            vec![MultiBufferOffset(0)..MultiBufferOffset(0)]
        );
    });
    assert!(*is_still_following.borrow());

    // Creating a pending selection that precedes another selection
    _ = leader.update(cx, |leader, window, cx| {
        leader.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(1)..MultiBufferOffset(1)])
        });
        leader.begin_selection(DisplayPoint::new(DisplayRow(0), 0), true, 1, window, cx);
    });
    follower
        .update(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                pending_update.borrow_mut().take().unwrap(),
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .unwrap();
    _ = follower.update(cx, |follower, _, cx| {
        assert_eq!(
            follower.selections.ranges(&follower.display_snapshot(cx)),
            vec![
                MultiBufferOffset(0)..MultiBufferOffset(0),
                MultiBufferOffset(1)..MultiBufferOffset(1)
            ]
        );
    });
    assert!(*is_still_following.borrow());

    // Extend the pending selection so that it surrounds another selection
    _ = leader.update(cx, |leader, window, cx| {
        leader.extend_selection(DisplayPoint::new(DisplayRow(0), 2), 1, window, cx);
    });
    follower
        .update(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                pending_update.borrow_mut().take().unwrap(),
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .unwrap();
    _ = follower.update(cx, |follower, _, cx| {
        assert_eq!(
            follower.selections.ranges(&follower.display_snapshot(cx)),
            vec![MultiBufferOffset(0)..MultiBufferOffset(2)]
        );
    });

    // Scrolling locally breaks the follow
    _ = follower.update(cx, |follower, window, cx| {
        let top_anchor = follower
            .buffer()
            .read(cx)
            .read(cx)
            .anchor_after(MultiBufferOffset(0));
        follower.set_scroll_anchor(
            ScrollAnchor {
                anchor: top_anchor,
                offset: gpui::Point::new(0.0, 0.5),
            },
            window,
            cx,
        );
    });
    assert!(!(*is_still_following.borrow()));
}

#[gpui::test]
async fn test_following_with_multiple_excerpts(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, ["/file.rs".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let cx = &mut VisualTestContext::from_window(*window, cx);

    let leader = pane.update_in(cx, |_, window, cx| {
        let multibuffer = cx.new(|_| MultiBuffer::new(ReadWrite));
        cx.new(|cx| build_editor(multibuffer.clone(), window, cx))
    });

    // Start following the editor when it has no excerpts.
    let mut state_message =
        leader.update_in(cx, |leader, window, cx| leader.to_state_proto(window, cx));
    let workspace_entity = workspace.clone();
    let follower_1 = cx
        .update_window(*window, |_, window, cx| {
            Editor::from_state_proto(
                workspace_entity,
                ViewId {
                    creator: CollaboratorId::PeerId(PeerId::default()),
                    id: 0,
                },
                &mut state_message,
                window,
                cx,
            )
        })
        .unwrap()
        .unwrap()
        .await
        .unwrap();

    let update_message = Rc::new(RefCell::new(None));
    follower_1.update_in(cx, {
        let update = update_message.clone();
        |_, window, cx| {
            cx.subscribe_in(&leader, window, move |_, leader, event, window, cx| {
                leader.update(cx, |leader, cx| {
                    leader.add_event_to_update_proto(event, &mut update.borrow_mut(), window, cx);
                });
            })
            .detach();
        }
    });

    let (buffer_1, buffer_2) = project.update(cx, |project, cx| {
        (
            project.create_local_buffer("abc\ndef\nghi\njkl\nmno\npqr\nstu\nvwx\nyza\nbcd\nefg\nhij\nklm\nnop\nqrs\ntuv\nwxy\nzab\ncde\nfgh\n", None, false, cx),
            project.create_local_buffer("aaa\nbbb\nccc\nddd\neee\nfff\nggg\nhhh\niii\njjj\nkkk\nlll\nmmm\nnnn\nooo\nppp\nqqq\nrrr\nsss\nttt\n", None, false, cx),
        )
    });

    // Insert some excerpts.
    leader.update(cx, |leader, cx| {
        leader.buffer.update(cx, |multibuffer, cx| {
            multibuffer.set_excerpts_for_path(
                PathKey::with_sort_prefix(1, rel_path("b.txt").into_arc()),
                buffer_1.clone(),
                vec![
                    Point::row_range(0..3),
                    Point::row_range(1..6),
                    Point::row_range(12..15),
                ],
                0,
                cx,
            );
            multibuffer.set_excerpts_for_path(
                PathKey::with_sort_prefix(1, rel_path("a.txt").into_arc()),
                buffer_2.clone(),
                vec![Point::row_range(0..6), Point::row_range(8..12)],
                0,
                cx,
            );
        });
    });

    // Apply the update of adding the excerpts.
    follower_1
        .update_in(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                update_message.borrow().clone().unwrap(),
                window,
                cx,
            )
        })
        .await
        .unwrap();
    assert_eq!(
        follower_1.update(cx, |editor, cx| editor.text(cx)),
        leader.update(cx, |editor, cx| editor.text(cx))
    );
    update_message.borrow_mut().take();

    // Start following separately after it already has excerpts.
    let mut state_message =
        leader.update_in(cx, |leader, window, cx| leader.to_state_proto(window, cx));
    let workspace_entity = workspace.clone();
    let follower_2 = cx
        .update_window(*window, |_, window, cx| {
            Editor::from_state_proto(
                workspace_entity,
                ViewId {
                    creator: CollaboratorId::PeerId(PeerId::default()),
                    id: 0,
                },
                &mut state_message,
                window,
                cx,
            )
        })
        .unwrap()
        .unwrap()
        .await
        .unwrap();
    assert_eq!(
        follower_2.update(cx, |editor, cx| editor.text(cx)),
        leader.update(cx, |editor, cx| editor.text(cx))
    );

    // Remove some excerpts.
    leader.update(cx, |leader, cx| {
        leader.buffer.update(cx, |multibuffer, cx| {
            multibuffer.remove_excerpts(
                PathKey::with_sort_prefix(1, rel_path("b.txt").into_arc()),
                cx,
            );
        });
    });

    // Apply the update of removing the excerpts.
    follower_1
        .update_in(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                update_message.borrow().clone().unwrap(),
                window,
                cx,
            )
        })
        .await
        .unwrap();
    follower_2
        .update_in(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                update_message.borrow().clone().unwrap(),
                window,
                cx,
            )
        })
        .await
        .unwrap();
    update_message.borrow_mut().take();
    assert_eq!(
        follower_1.update(cx, |editor, cx| editor.text(cx)),
        leader.update(cx, |editor, cx| editor.text(cx))
    );
}
