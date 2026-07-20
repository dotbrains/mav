use super::*;

impl Workspace {
    pub fn new(
        workspace_id: Option<WorkspaceId>,
        project: Entity<Project>,
        app_state: Arc<AppState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if let Some(trusted_worktrees) = TrustedWorktrees::try_get_global(cx) {
            cx.subscribe(&trusted_worktrees, |_, worktrees_store, e, cx| {
                if let TrustedWorktreesEvent::Trusted(..) = e {
                    // Do not persist auto trusted worktrees
                    if !ProjectSettings::get_global(cx).session.trust_all_worktrees {
                        worktrees_store.update(cx, |worktrees_store, cx| {
                            worktrees_store.schedule_serialization(
                                cx,
                                |new_trusted_worktrees, cx| {
                                    let timeout =
                                        cx.background_executor().timer(SERIALIZATION_THROTTLE_TIME);
                                    let db = WorkspaceDb::global(cx);
                                    cx.background_spawn(async move {
                                        timeout.await;
                                        db.save_trusted_worktrees(new_trusted_worktrees)
                                            .await
                                            .log_err();
                                    })
                                },
                            )
                        });
                    }
                }
            })
            .detach();

            cx.observe_global::<SettingsStore>(|_, cx| {
                if ProjectSettings::get_global(cx).session.trust_all_worktrees {
                    if let Some(trusted_worktrees) = TrustedWorktrees::try_get_global(cx) {
                        trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                            trusted_worktrees.auto_trust_all(cx);
                        })
                    }
                }
            })
            .detach();
        }

        cx.subscribe_in(&project, window, move |this, _, event, window, cx| {
            match event {
                project::Event::RemoteIdChanged(_) => {
                    this.update_window_title(window, cx);
                }

                project::Event::CollaboratorLeft(peer_id) => {
                    this.collaborator_left(*peer_id, window, cx);
                }

                &project::Event::WorktreeRemoved(_) => {
                    this.update_window_title(window, cx);
                    this.serialize_workspace(window, cx);
                    this.update_history(cx);
                }

                &project::Event::WorktreeAdded(id) => {
                    this.update_window_title(window, cx);
                    if this
                        .project()
                        .read(cx)
                        .worktree_for_id(id, cx)
                        .is_some_and(|wt| wt.read(cx).is_visible())
                    {
                        this.serialize_workspace(window, cx);
                        this.update_history(cx);
                    }
                }
                project::Event::WorktreeUpdatedEntries(..) => {
                    this.update_window_title(window, cx);
                    this.serialize_workspace(window, cx);
                }

                project::Event::DisconnectedFromHost => {
                    this.update_window_edited(window, cx);
                    let leaders_to_unfollow =
                        this.follower_states.keys().copied().collect::<Vec<_>>();
                    for leader_id in leaders_to_unfollow {
                        this.unfollow(leader_id, window, cx);
                    }
                }

                project::Event::DisconnectedFromRemote {
                    server_not_running: _,
                } => {
                    this.update_window_edited(window, cx);
                }

                project::Event::Closed => {
                    window.remove_window();
                }

                project::Event::DeletedEntry(_, entry_id) => {
                    for pane in this.panes.iter() {
                        pane.update(cx, |pane, cx| {
                            pane.handle_deleted_project_item(*entry_id, window, cx)
                        });
                    }
                }

                project::Event::Toast {
                    notification_id,
                    message,
                    link,
                } => this.show_notification(
                    NotificationId::named(notification_id.clone()),
                    cx,
                    |cx| {
                        let mut notification = MessageNotification::new(message.clone(), cx);
                        if let Some(link) = link {
                            notification = notification
                                .more_info_message(link.label)
                                .more_info_url(link.url);
                        }

                        cx.new(|_| notification)
                    },
                ),

                project::Event::HideToast { notification_id } => {
                    this.dismiss_notification(&NotificationId::named(notification_id.clone()), cx)
                }

                project::Event::LanguageServerPrompt(request) => {
                    struct LanguageServerPrompt;

                    this.show_notification(
                        NotificationId::composite::<LanguageServerPrompt>(request.id),
                        cx,
                        |cx| {
                            cx.new(|cx| {
                                notifications::LanguageServerPrompt::new(request.clone(), cx)
                            })
                        },
                    );
                }

                project::Event::AgentLocationChanged => {
                    this.handle_agent_location_changed(window, cx)
                }

                _ => {}
            }
            cx.notify()
        })
        .detach();

        cx.subscribe_in(
            &project.read(cx).breakpoint_store(),
            window,
            |workspace, _, event, window, cx| match event {
                BreakpointStoreEvent::BreakpointsUpdated(_, _)
                | BreakpointStoreEvent::BreakpointsCleared(_) => {
                    workspace.serialize_workspace(window, cx);
                }
                BreakpointStoreEvent::SetDebugLine | BreakpointStoreEvent::ClearDebugLines => {}
            },
        )
        .detach();
        if let Some(toolchain_store) = project.read(cx).toolchain_store() {
            cx.subscribe_in(
                &toolchain_store,
                window,
                |workspace, _, event, window, cx| match event {
                    ToolchainStoreEvent::CustomToolchainsModified => {
                        workspace.serialize_workspace(window, cx);
                    }
                    _ => {}
                },
            )
            .detach();
        }

        cx.on_focus_lost(window, |this, window, cx| {
            let focus_handle = this.focus_handle(cx);
            window.focus(&focus_handle, cx);
        })
        .detach();

        let weak_handle = cx.entity().downgrade();
        let pane_history_timestamp = Arc::new(AtomicUsize::new(0));

        let center_pane = cx.new(|cx| {
            let mut center_pane = Pane::new(
                weak_handle.clone(),
                project.clone(),
                pane_history_timestamp.clone(),
                None,
                NewFile.boxed_clone(),
                true,
                window,
                cx,
            );
            center_pane.set_can_split(Some(Arc::new(|_, _, _, _| true)));
            center_pane.set_should_display_welcome_page(true);
            center_pane
        });
        cx.subscribe_in(&center_pane, window, Self::handle_pane_event)
            .detach();

        window.focus(&center_pane.focus_handle(cx), cx);

        cx.emit(Event::PaneAdded(center_pane.clone()));

        let any_window_handle = window.window_handle();
        app_state.workspace_store.update(cx, |store, _| {
            store
                .workspaces
                .insert((any_window_handle, weak_handle.clone()));
        });

        let mut current_user = app_state.user_store.read(cx).watch_current_user();
        let mut connection_status = app_state.client.status();
        let _observe_current_user = cx.spawn_in(window, async move |this, cx| {
            current_user.next().await;
            connection_status.next().await;
            let mut stream =
                Stream::map(current_user, drop).merge(Stream::map(connection_status, drop));

            while stream.recv().await.is_some() {
                this.update(cx, |_, cx| cx.notify())?;
            }
            anyhow::Ok(())
        });

        // All leader updates are enqueued and then processed in a single task, so
        // that each asynchronous operation can be run in order.
        let (leader_updates_tx, mut leader_updates_rx) =
            mpsc::unbounded::<(PeerId, proto::UpdateFollowers)>();
        let _apply_leader_updates = cx.spawn_in(window, async move |this, cx| {
            while let Some((leader_id, update)) = leader_updates_rx.next().await {
                Self::process_leader_update(&this, leader_id, update, cx)
                    .await
                    .log_err();
            }

            Ok(())
        });

        cx.emit(Event::WorkspaceCreated(weak_handle.clone()));
        let modal_layer = cx.new(|_| ModalLayer::new());
        let toast_layer = cx.new(|_| ToastLayer::new());
        cx.subscribe(
            &modal_layer,
            |_, _, _: &modal_layer::ModalOpenedEvent, cx| {
                cx.emit(Event::ModalOpened);
            },
        )
        .detach();

        let left_dock = Dock::new(DockPosition::Left, modal_layer.clone(), window, cx);
        let right_dock = Dock::new(DockPosition::Right, modal_layer.clone(), window, cx);
        let multi_workspace = window
            .root::<MultiWorkspace>()
            .flatten()
            .map(|mw| mw.downgrade());
        let status_bar =
            cx.new(|cx| StatusBar::new(&center_pane.clone(), multi_workspace.clone(), window, cx));

        let session_id = app_state.session.read(cx).id().to_owned();

        let mut active_call = None;
        if let Some(call) = GlobalAnyActiveCall::try_global(cx).cloned() {
            let subscriptions =
                vec![
                    call.0
                        .subscribe(window, cx, Box::new(Self::on_active_call_event)),
                ];
            active_call = Some((call, subscriptions));
        }

        let (serializable_items_tx, serializable_items_rx) =
            mpsc::unbounded::<Box<dyn SerializableItemHandle>>();
        let _items_serializer = cx.spawn_in(window, async move |this, cx| {
            Self::serialize_items(&this, serializable_items_rx, cx).await
        });

        let subscriptions = vec![
            cx.observe_window_activation(window, Self::on_window_activation_changed),
            cx.observe_window_bounds(window, move |this, window, cx| {
                if this.bounds_save_task_queued.is_some() {
                    return;
                }
                this.bounds_save_task_queued = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    this.update_in(cx, |this, window, cx| {
                        this.save_window_bounds(window, cx).detach();
                        this.bounds_save_task_queued.take();
                    })
                    .ok();
                }));
                cx.notify();
            }),
            cx.observe_window_appearance(window, |_, window, cx| {
                let window_appearance = window.appearance();

                *SystemAppearance::global_mut(cx) = SystemAppearance(window_appearance.into());

                theme_settings::reload_theme(cx);
                theme_settings::reload_icon_theme(cx);
            }),
            cx.on_release({
                let weak_handle = weak_handle.clone();
                move |this, cx| {
                    this.app_state.workspace_store.update(cx, move |store, _| {
                        store.workspaces.retain(|(_, weak)| weak != &weak_handle);
                    })
                }
            }),
        ];

        cx.defer_in(window, move |this, window, cx| {
            this.update_window_title(window, cx);
            this.show_initial_notifications(cx);
        });

        let mut center = PaneGroup::new(center_pane.clone());
        center.set_is_center(true);
        center.mark_positions(cx);

        Workspace {
            weak_self: weak_handle.clone(),
            zoomed: None,
            zoomed_position: None,
            previous_dock_drag_coordinates: None,
            center,
            panes: vec![center_pane.clone()],
            panes_by_item: Default::default(),
            active_pane: center_pane.clone(),
            last_active_center_pane: Some(center_pane.downgrade()),
            last_active_view_id: None,
            status_bar,
            modal_layer,
            toast_layer,
            notifications: Notifications::default(),
            suppressed_notifications: HashSet::default(),
            left_dock,
            right_dock,
            _panels_task: None,
            project: project.clone(),
            follower_states: Default::default(),
            last_leaders_by_pane: Default::default(),
            auto_watch: AutoWatch::Off,
            dispatching_keystrokes: Default::default(),
            window_edited: false,
            last_window_title: None,
            dirty_items: Default::default(),
            active_call,
            database_id: workspace_id,
            app_state,
            _observe_current_user,
            _apply_leader_updates,
            _schedule_serialize_workspace: None,
            _serialize_workspace_task: None,
            _schedule_serialize_ssh_paths: None,
            leader_updates_tx,
            _subscriptions: subscriptions,
            pane_history_timestamp,
            workspace_actions: Default::default(),
            // This data will be incorrect, but it will be overwritten by the time it needs to be used.
            bounds: Default::default(),
            centered_layout: false,
            bounds_save_task_queued: None,
            on_prompt_for_new_path: None,
            on_prompt_for_open_path: None,
            terminal_provider: None,
            debugger_provider: None,
            serializable_items_tx,
            _items_serializer,
            session_id: Some(session_id),

            scheduled_tasks: Vec::new(),
            removing: false,
            sidebar_focus_handle: None,
            multi_workspace,
            active_workspace_id: None,
            active_worktree_creation: ActiveWorktreeCreation::default(),
            open_in_dev_container: false,
            _dev_container_task: None,
            deferred_save_items: Vec::new(),
        }
    }
}
