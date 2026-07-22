use super::*;

pub(super) fn added_to_pane<T: Item>(
    item: &Entity<T>,
    workspace: &mut Workspace,
    pane: Entity<Pane>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let weak_item = item.downgrade();
    let history = pane.read(cx).nav_history_for_item(item);
    item.update(cx, |this, cx| {
        this.set_nav_history(history, window, cx);
        this.added_to_workspace(workspace, window, cx);
    });

    if let Some(serializable_item) = item.to_serializable_item_handle(cx) {
        workspace
            .enqueue_item_serialization(serializable_item)
            .log_err();
    }

    let new_pane_id = pane.entity_id();
    let old_item_pane = workspace
        .panes_by_item
        .insert(item.item_id(), pane.downgrade());

    if old_item_pane.as_ref().is_none_or(|old_pane| {
        old_pane
            .upgrade()
            .is_some_and(|old_pane| old_pane.entity_id() != new_pane_id)
    }) {
        item.update(cx, |this, cx| {
            this.pane_changed(new_pane_id, cx);
        });
    }

    if old_item_pane.is_none() {
        let mut pending_autosave = DelayedDebouncedEditAction::new();
        let (pending_update_tx, mut pending_update_rx) = mpsc::unbounded();
        let pending_update = Rc::new(RefCell::new(None));

        let mut send_follower_updates = None;
        if let Some(item) = item.to_followable_item_handle(cx) {
            let is_project_item = item.is_project_item(window, cx);
            let item = item.downgrade();

            send_follower_updates = Some(cx.spawn_in(window, {
                let pending_update = pending_update.clone();
                async move |workspace, cx| {
                    while let Ok(mut leader_id) = pending_update_rx.recv().await {
                        while let Ok(id) = pending_update_rx.try_recv() {
                            leader_id = id;
                        }

                        workspace.update_in(cx, |workspace, window, cx| {
                            let Some(item) = item.upgrade() else { return };
                            workspace.update_followers(
                                is_project_item,
                                proto::update_followers::Variant::UpdateView(proto::UpdateView {
                                    id: item
                                        .remote_id(workspace.client(), window, cx)
                                        .and_then(|id| id.to_proto()),
                                    variant: pending_update.borrow_mut().take(),
                                    leader_id,
                                }),
                                window,
                                cx,
                            );
                        })?;
                        cx.background_executor().timer(LEADER_UPDATE_THROTTLE).await;
                    }
                    anyhow::Ok(())
                }
            }));
        }

        let mut event_subscription = Some(cx.subscribe_in(
            item,
            window,
            move |workspace, item: &Entity<T>, event, window, cx| {
                let pane = if let Some(pane) = workspace
                    .panes_by_item
                    .get(&item.item_id())
                    .and_then(|pane| pane.upgrade())
                {
                    pane
                } else {
                    return;
                };

                if let Some(item) = item.to_followable_item_handle(cx) {
                    let leader_id = workspace.leader_for_pane(&pane);

                    if let Some(leader_id) = leader_id
                        && let Some(FollowEvent::Unfollow) = item.to_follow_event(event)
                    {
                        workspace.unfollow(leader_id, window, cx);
                    }

                    if item.item_focus_handle(cx).contains_focused(window, cx) {
                        match leader_id {
                            Some(CollaboratorId::Agent) => {}
                            Some(CollaboratorId::PeerId(leader_peer_id)) => {
                                item.add_event_to_update_proto(
                                    event,
                                    &mut pending_update.borrow_mut(),
                                    window,
                                    cx,
                                );
                                pending_update_tx.unbounded_send(Some(leader_peer_id)).ok();
                            }
                            None => {
                                item.add_event_to_update_proto(
                                    event,
                                    &mut pending_update.borrow_mut(),
                                    window,
                                    cx,
                                );
                                pending_update_tx.unbounded_send(None).ok();
                            }
                        }
                    }
                }

                if let Some(item) = item.to_serializable_item_handle(cx)
                    && item.should_serialize(event, cx)
                {
                    workspace.enqueue_item_serialization(item).ok();
                }

                T::to_item_events(event, &mut |event| match event {
                    ItemEvent::CloseItem => {
                        pane.update(cx, |pane, cx| {
                            pane.close_item_by_id(
                                item.item_id(),
                                crate::SaveIntent::Close,
                                window,
                                cx,
                            )
                        })
                        .detach_and_log_err(cx);
                    }

                    ItemEvent::UpdateTab => {
                        workspace.update_item_dirty_state(item, window, cx);

                        if item.has_deleted_file(cx)
                            && !item.is_dirty(cx)
                            && item.workspace_settings(cx).close_on_file_delete
                        {
                            let item_id = item.item_id();
                            let close_item_task = pane.update(cx, |pane, cx| {
                                pane.close_item_by_id(item_id, crate::SaveIntent::Close, window, cx)
                            });
                            cx.spawn_in(window, {
                                let pane = pane.clone();
                                async move |_workspace, cx| {
                                    close_item_task.await?;
                                    pane.update(cx, |pane, _cx| {
                                        pane.nav_history_mut().remove_item(item_id);
                                    });
                                    anyhow::Ok(())
                                }
                            })
                            .detach_and_log_err(cx);
                        } else {
                            pane.update(cx, |_, cx| {
                                cx.emit(pane::Event::ChangeItemTitle);
                                cx.notify();
                            });
                        }
                    }

                    ItemEvent::UpdateBreadcrumbs => {
                        if &pane == workspace.active_pane()
                            && pane
                                .read(cx)
                                .active_item()
                                .is_some_and(|active_item| active_item.item_id() == item.item_id())
                        {
                            workspace.active_item_path_changed(false, window, cx);
                        }
                    }

                    ItemEvent::Edit => {
                        let autosave = item.workspace_settings(cx).autosave;

                        if let AutosaveSetting::AfterDelay { milliseconds } = autosave {
                            let delay = Duration::from_millis(milliseconds.0);
                            let item = item.clone();
                            pending_autosave.fire_new(
                                delay,
                                window,
                                cx,
                                move |workspace, window, cx| {
                                    Pane::autosave_item(
                                        &item,
                                        workspace.project().clone(),
                                        window,
                                        cx,
                                    )
                                },
                            );
                        }
                        pane.update(cx, |pane, cx| pane.handle_item_edit(item.item_id(), cx));
                    }
                });
            },
        ));

        cx.on_focus_out(
            &item.read(cx).focus_handle(cx),
            window,
            move |workspace, _event, window, cx| {
                if let Some(item) = weak_item.upgrade()
                    && item.workspace_settings(cx).autosave == AutosaveSetting::OnFocusChange
                {
                    // Only trigger autosave if focus has truly left the item.
                    // If focus is still within the item's hierarchy (e.g., moved to a context menu),
                    // don't trigger autosave to avoid unwanted formatting and cursor jumps.
                    let focus_handle = item.item_focus_handle(cx);
                    if focus_handle.contains_focused(window, cx) {
                        return;
                    }

                    // Add the item to a deferred save list. The actual save will happen when
                    // focus lands on a pane or panel (via handle_pane_focused or
                    // handle_panel_focused), or when the window deactivates.
                    // This avoids saving when opening modals and skips saving if focus
                    // returns to the same item.
                    workspace.deferred_save_items.push(item.downgrade_item());

                    // Defer the flush to ensure all focus events are processed first.
                    // This is needed because on_focus_out fires before handle_pane_focused
                    // when switching items.
                    cx.defer_in(window, |workspace, window, cx| {
                        // Don't flush if a modal is active - the user might return
                        // to the original item when the modal is dismissed.
                        if !workspace.has_active_modal(window, cx) {
                            workspace.flush_deferred_saves(window, cx);
                        }
                    });
                }
            },
        )
        .detach();

        let item_id = item.item_id();
        workspace.update_item_dirty_state(item, window, cx);
        cx.observe_release_in(item, window, move |workspace, _, _, _| {
            workspace.panes_by_item.remove(&item_id);
            event_subscription.take();
            send_follower_updates.take();
        })
        .detach();
    }

    cx.defer_in(window, |workspace, window, cx| {
        workspace.serialize_workspace(window, cx);
    });
}
