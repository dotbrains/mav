use super::*;

impl Project {
    pub fn init(client: &Arc<Client>, cx: &mut App) {
        connection_manager::init(client.clone(), cx);

        let client: AnyProtoClient = client.clone().into();
        client.add_entity_message_handler(Self::handle_add_collaborator);
        client.add_entity_message_handler(Self::handle_update_project_collaborator);
        client.add_entity_message_handler(Self::handle_remove_collaborator);
        client.add_entity_message_handler(Self::handle_update_project);
        client.add_entity_message_handler(Self::handle_unshare_project);
        client.add_entity_request_handler(Self::handle_update_buffer);
        client.add_entity_message_handler(Self::handle_update_worktree);
        client.add_entity_request_handler(Self::handle_synchronize_buffers);

        client.add_entity_request_handler(Self::handle_search_candidate_buffers);
        client.add_entity_request_handler(Self::handle_open_buffer_by_id);
        client.add_entity_request_handler(Self::handle_open_buffer_by_path);
        client.add_entity_request_handler(Self::handle_open_new_buffer);
        client.add_entity_message_handler(Self::handle_create_buffer_for_peer);
        client.add_entity_message_handler(Self::handle_toggle_lsp_logs);
        client.add_entity_message_handler(Self::handle_create_image_for_peer);
        client.add_entity_request_handler(Self::handle_find_search_candidates_chunk);
        client.add_entity_message_handler(Self::handle_find_search_candidates_cancel);
        client.add_entity_message_handler(Self::handle_create_file_for_peer);

        WorktreeStore::init(&client);
        BufferStore::init(&client);
        LspStore::init(&client);
        GitStore::init(&client);
        SettingsObserver::init(&client);
        TaskStore::init(Some(&client));
        ToolchainStore::init(&client);
        DapStore::init(&client, cx);
        BreakpointStore::init(&client);
        context_server_store::init(cx);
    }

    pub fn local(
        client: Arc<Client>,
        node: NodeRuntime,
        user_store: Entity<UserStore>,
        languages: Arc<LanguageRegistry>,
        fs: Arc<dyn Fs>,
        env: Option<HashMap<String, String>>,
        flags: LocalProjectFlags,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let (tx, rx) = mpsc::unbounded();
            cx.spawn(async move |this, cx| Self::send_buffer_ordered_messages(this, rx, cx).await)
                .detach();
            let snippets = SnippetProvider::new(fs.clone(), BTreeSet::from_iter([]), cx);
            let worktree_store =
                cx.new(|cx| WorktreeStore::local(false, fs.clone(), WorktreeIdCounter::get(cx)));
            if flags.init_worktree_trust {
                trusted_worktrees::track_worktree_trust(
                    worktree_store.clone(),
                    None,
                    None,
                    None,
                    cx,
                );
            }
            cx.subscribe(&worktree_store, Self::on_worktree_store_event)
                .detach();

            let weak_self = cx.weak_entity();
            let context_server_store = cx.new(|cx| {
                ContextServerStore::local(
                    worktree_store.clone(),
                    Some(weak_self.clone()),
                    false,
                    cx,
                )
            });

            let environment = cx.new(|cx| {
                ProjectEnvironment::new(env, worktree_store.downgrade(), None, false, cx)
            });
            let manifest_tree = ManifestTree::new(worktree_store.clone(), cx);
            let toolchain_store = cx.new(|cx| {
                ToolchainStore::local(
                    languages.clone(),
                    worktree_store.clone(),
                    environment.clone(),
                    manifest_tree.clone(),
                    cx,
                )
            });

            let buffer_store = cx.new(|cx| BufferStore::local(worktree_store.clone(), cx));
            cx.subscribe(&buffer_store, Self::on_buffer_store_event)
                .detach();

            let bookmark_store =
                cx.new(|_| BookmarkStore::new(worktree_store.clone(), buffer_store.clone()));

            let breakpoint_store =
                cx.new(|_| BreakpointStore::local(worktree_store.clone(), buffer_store.clone()));

            let dap_store = cx.new(|cx| {
                DapStore::new_local(
                    client.http_client(),
                    node.clone(),
                    fs.clone(),
                    environment.clone(),
                    toolchain_store.read(cx).as_language_toolchain_store(),
                    worktree_store.clone(),
                    breakpoint_store.clone(),
                    false,
                    cx,
                )
            });
            cx.subscribe(&dap_store, Self::on_dap_store_event).detach();

            let image_store = cx.new(|cx| ImageStore::local(worktree_store.clone(), cx));
            cx.subscribe(&image_store, Self::on_image_store_event)
                .detach();

            let prettier_store = cx.new(|cx| {
                PrettierStore::new(
                    node.clone(),
                    fs.clone(),
                    languages.clone(),
                    worktree_store.clone(),
                    cx,
                )
            });

            let git_store = cx.new(|cx| {
                GitStore::local(
                    &worktree_store,
                    buffer_store.clone(),
                    environment.clone(),
                    fs.clone(),
                    cx,
                )
            });

            let task_store = cx.new(|cx| {
                TaskStore::local(
                    buffer_store.downgrade(),
                    worktree_store.clone(),
                    toolchain_store.read(cx).as_language_toolchain_store(),
                    environment.clone(),
                    git_store.clone(),
                    cx,
                )
            });

            let settings_observer = cx.new(|cx| {
                SettingsObserver::new_local(
                    fs.clone(),
                    worktree_store.clone(),
                    task_store.clone(),
                    flags.watch_global_configs,
                    cx,
                )
            });
            cx.subscribe(&settings_observer, Self::on_settings_observer_event)
                .detach();

            let lsp_store = cx.new(|cx| {
                LspStore::new_local(
                    buffer_store.clone(),
                    worktree_store.clone(),
                    prettier_store.clone(),
                    toolchain_store
                        .read(cx)
                        .as_local_store()
                        .expect("Toolchain store to be local")
                        .clone(),
                    environment.clone(),
                    manifest_tree,
                    languages.clone(),
                    client.http_client(),
                    fs.clone(),
                    cx,
                )
            });

            let agent_server_store = cx.new(|cx| {
                AgentServerStore::local(
                    node.clone(),
                    fs.clone(),
                    environment.clone(),
                    client.http_client(),
                    cx,
                )
            });

            cx.subscribe(&lsp_store, Self::on_lsp_store_event).detach();

            Self {
                buffer_ordered_messages_tx: tx,
                collaborators: Default::default(),
                worktree_store,
                buffer_store,
                image_store,
                lsp_store,
                context_server_store,
                join_project_response_message_id: 0,
                client_state: ProjectClientState::Local,
                git_store,
                client_subscriptions: Vec::new(),
                _subscriptions: vec![cx.on_release(Self::release)],
                active_entry: None,
                snippets,
                languages,
                collab_client: client,
                task_store,
                user_store,
                settings_observer,
                fs,
                remote_client: None,
                bookmark_store,
                breakpoint_store,
                dap_store,
                agent_server_store,

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
            }
        })
    }
}
