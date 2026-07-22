use super::*;

impl Project {
    pub fn remote(
        remote: Entity<RemoteClient>,
        client: Arc<Client>,
        node: NodeRuntime,
        user_store: Entity<UserStore>,
        languages: Arc<LanguageRegistry>,
        fs: Arc<dyn Fs>,
        init_worktree_trust: bool,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let (tx, rx) = mpsc::unbounded();
            cx.spawn(async move |this, cx| Self::send_buffer_ordered_messages(this, rx, cx).await)
                .detach();
            let snippets = SnippetProvider::new(fs.clone(), BTreeSet::from_iter([]), cx);

            let (remote_proto, path_style, connection_options) =
                remote.read_with(cx, |remote, _| {
                    (
                        remote.proto_client(),
                        remote.path_style(),
                        remote.connection_options(),
                    )
                });
            let worktree_store = cx.new(|cx| {
                WorktreeStore::remote(
                    false,
                    remote_proto.clone(),
                    REMOTE_SERVER_PROJECT_ID,
                    path_style,
                    WorktreeIdCounter::get(cx),
                )
            });

            cx.subscribe(&worktree_store, Self::on_worktree_store_event)
                .detach();
            if init_worktree_trust {
                trusted_worktrees::track_worktree_trust(
                    worktree_store.clone(),
                    Some(RemoteHostLocation::from(connection_options)),
                    None,
                    Some((remote_proto.clone(), ProjectId(REMOTE_SERVER_PROJECT_ID))),
                    cx,
                );
            }

            let weak_self = cx.weak_entity();

            let buffer_store = cx.new(|cx| {
                BufferStore::remote(
                    worktree_store.clone(),
                    remote.read(cx).proto_client(),
                    REMOTE_SERVER_PROJECT_ID,
                    cx,
                )
            });
            let image_store = cx.new(|cx| {
                ImageStore::remote(
                    worktree_store.clone(),
                    remote.read(cx).proto_client(),
                    REMOTE_SERVER_PROJECT_ID,
                    cx,
                )
            });
            cx.subscribe(&buffer_store, Self::on_buffer_store_event)
                .detach();
            let toolchain_store = cx.new(|cx| {
                ToolchainStore::remote(
                    REMOTE_SERVER_PROJECT_ID,
                    worktree_store.clone(),
                    remote.read(cx).proto_client(),
                    cx,
                )
            });

            let context_server_store = cx.new(|cx| {
                ContextServerStore::remote(
                    rpc::proto::REMOTE_SERVER_PROJECT_ID,
                    remote.clone(),
                    worktree_store.clone(),
                    Some(weak_self.clone()),
                    cx,
                )
            });

            let environment = cx.new(|cx| {
                ProjectEnvironment::new(
                    None,
                    worktree_store.downgrade(),
                    Some(remote.downgrade()),
                    false,
                    cx,
                )
            });

            let lsp_store = cx.new(|cx| {
                LspStore::new_remote(
                    buffer_store.clone(),
                    worktree_store.clone(),
                    languages.clone(),
                    remote_proto.clone(),
                    REMOTE_SERVER_PROJECT_ID,
                    cx,
                )
            });
            cx.subscribe(&lsp_store, Self::on_lsp_store_event).detach();

            let bookmark_store =
                cx.new(|_| BookmarkStore::new(worktree_store.clone(), buffer_store.clone()));

            let breakpoint_store = cx.new(|_| {
                BreakpointStore::remote(
                    REMOTE_SERVER_PROJECT_ID,
                    remote_proto.clone(),
                    buffer_store.clone(),
                    worktree_store.clone(),
                )
            });

            let dap_store = cx.new(|cx| {
                DapStore::new_remote(
                    REMOTE_SERVER_PROJECT_ID,
                    remote.clone(),
                    breakpoint_store.clone(),
                    worktree_store.clone(),
                    node.clone(),
                    client.http_client(),
                    fs.clone(),
                    cx,
                )
            });

            let git_store = cx.new(|cx| {
                GitStore::remote(
                    &worktree_store,
                    buffer_store.clone(),
                    remote_proto.clone(),
                    REMOTE_SERVER_PROJECT_ID,
                    cx,
                )
            });

            let task_store = cx.new(|cx| {
                TaskStore::remote(
                    buffer_store.downgrade(),
                    worktree_store.clone(),
                    toolchain_store.read(cx).as_language_toolchain_store(),
                    remote.read(cx).proto_client(),
                    REMOTE_SERVER_PROJECT_ID,
                    git_store.clone(),
                    cx,
                )
            });

            let settings_observer = cx.new(|cx| {
                SettingsObserver::new_remote(
                    fs.clone(),
                    worktree_store.clone(),
                    task_store.clone(),
                    Some(remote_proto.clone()),
                    false,
                    cx,
                )
            });
            cx.subscribe(&settings_observer, Self::on_settings_observer_event)
                .detach();

            let agent_server_store = cx.new(|_| {
                AgentServerStore::remote(
                    REMOTE_SERVER_PROJECT_ID,
                    remote.clone(),
                    worktree_store.clone(),
                )
            });

            cx.subscribe(&remote, Self::on_remote_client_event).detach();

            let this = Self {
                buffer_ordered_messages_tx: tx,
                collaborators: Default::default(),
                worktree_store,
                buffer_store,
                image_store,
                lsp_store,
                context_server_store,
                bookmark_store,
                breakpoint_store,
                dap_store,
                join_project_response_message_id: 0,
                client_state: ProjectClientState::Local,
                git_store,
                agent_server_store,
                client_subscriptions: Vec::new(),
                _subscriptions: vec![
                    cx.on_release(Self::release),
                    cx.on_app_quit(|this, cx| {
                        let shutdown = this.remote_client.take().and_then(|client| {
                            client.update(cx, |client, cx| {
                                client.shutdown_processes(
                                    Some(proto::ShutdownRemoteServer {}),
                                    cx.background_executor().clone(),
                                )
                            })
                        });

                        cx.background_executor().spawn(async move {
                            if let Some(shutdown) = shutdown {
                                shutdown.await;
                            }
                        })
                    }),
                ],
                active_entry: None,
                snippets,
                languages,
                collab_client: client,
                task_store,
                user_store,
                settings_observer,
                fs,
                remote_client: Some(remote.clone()),
                buffers_needing_diff: Default::default(),
                git_diff_debouncer: DebouncedDelay::new(),
                terminals: Terminals {
                    local_handles: Vec::new(),
                },
                node: Some(node),
                search_history: new_search_history(),
                environment,
                remotely_created_models: Default::default(),

                search_included_history: new_search_history(),
                search_excluded_history: new_search_history(),

                toolchain_store: Some(toolchain_store),
                agent_location: None,
                downloading_files: Default::default(),
                last_worktree_paths: WorktreePaths::default(),
            };

            // remote server -> local machine handlers
            remote_proto.subscribe_to_entity(REMOTE_SERVER_PROJECT_ID, &cx.entity());
            remote_proto.subscribe_to_entity(REMOTE_SERVER_PROJECT_ID, &this.buffer_store);
            remote_proto.subscribe_to_entity(REMOTE_SERVER_PROJECT_ID, &this.worktree_store);
            remote_proto.subscribe_to_entity(REMOTE_SERVER_PROJECT_ID, &this.lsp_store);
            remote_proto.subscribe_to_entity(REMOTE_SERVER_PROJECT_ID, &this.dap_store);
            remote_proto.subscribe_to_entity(REMOTE_SERVER_PROJECT_ID, &this.breakpoint_store);
            remote_proto.subscribe_to_entity(REMOTE_SERVER_PROJECT_ID, &this.settings_observer);
            remote_proto.subscribe_to_entity(REMOTE_SERVER_PROJECT_ID, &this.git_store);
            remote_proto.subscribe_to_entity(REMOTE_SERVER_PROJECT_ID, &this.agent_server_store);

            remote_proto.add_entity_message_handler(Self::handle_create_buffer_for_peer);
            remote_proto.add_entity_message_handler(Self::handle_create_image_for_peer);
            remote_proto.add_entity_message_handler(Self::handle_create_file_for_peer);
            remote_proto.add_entity_message_handler(Self::handle_update_worktree);
            remote_proto.add_entity_message_handler(Self::handle_update_project);
            remote_proto.add_entity_message_handler(Self::handle_toast);
            remote_proto.add_entity_message_handler(Self::handle_telemetry_event);
            remote_proto.add_entity_request_handler(Self::handle_language_server_prompt_request);
            remote_proto.add_entity_message_handler(Self::handle_hide_toast);
            remote_proto.add_entity_request_handler(Self::handle_update_buffer_from_remote_server);
            remote_proto.add_entity_request_handler(Self::handle_trust_worktrees);
            remote_proto.add_entity_request_handler(Self::handle_restrict_worktrees);
            remote_proto.add_entity_request_handler(Self::handle_find_search_candidates_chunk);

            remote_proto.add_entity_message_handler(Self::handle_find_search_candidates_cancel);
            BufferStore::init(&remote_proto);
            WorktreeStore::init_remote(&remote_proto);
            LspStore::init(&remote_proto);
            SettingsObserver::init(&remote_proto);
            TaskStore::init(Some(&remote_proto));
            ToolchainStore::init(&remote_proto);
            DapStore::init(&remote_proto, cx);
            BreakpointStore::init(&remote_proto);
            GitStore::init(&remote_proto);
            AgentServerStore::init_remote(&remote_proto);

            this
        })
    }
}
