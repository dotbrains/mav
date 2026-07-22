use super::*;

impl Project {
    pub async fn in_room(
        remote_id: u64,
        client: Arc<Client>,
        user_store: Entity<UserStore>,
        languages: Arc<LanguageRegistry>,
        fs: Arc<dyn Fs>,
        cx: AsyncApp,
    ) -> Result<Entity<Self>> {
        client.connect(true, &cx).await.into_response()?;

        let subscriptions = [
            EntitySubscription::Project(client.subscribe_to_entity::<Self>(remote_id)?),
            EntitySubscription::BufferStore(client.subscribe_to_entity::<BufferStore>(remote_id)?),
            EntitySubscription::GitStore(client.subscribe_to_entity::<GitStore>(remote_id)?),
            EntitySubscription::WorktreeStore(
                client.subscribe_to_entity::<WorktreeStore>(remote_id)?,
            ),
            EntitySubscription::LspStore(client.subscribe_to_entity::<LspStore>(remote_id)?),
            EntitySubscription::SettingsObserver(
                client.subscribe_to_entity::<SettingsObserver>(remote_id)?,
            ),
            EntitySubscription::DapStore(client.subscribe_to_entity::<DapStore>(remote_id)?),
            EntitySubscription::BreakpointStore(
                client.subscribe_to_entity::<BreakpointStore>(remote_id)?,
            ),
        ];
        let committer = get_git_committer(&cx).await;
        let response = client
            .request_envelope(proto::JoinProject {
                project_id: remote_id,
                committer_email: committer.email,
                committer_name: committer.name,
                features: CURRENT_PROJECT_FEATURES
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            })
            .await?;
        Self::from_join_project_response(
            response,
            subscriptions,
            client,
            false,
            user_store,
            languages,
            fs,
            cx,
        )
        .await
    }

    async fn from_join_project_response(
        response: TypedEnvelope<proto::JoinProjectResponse>,
        subscriptions: [EntitySubscription; 8],
        client: Arc<Client>,
        run_tasks: bool,
        user_store: Entity<UserStore>,
        languages: Arc<LanguageRegistry>,
        fs: Arc<dyn Fs>,
        mut cx: AsyncApp,
    ) -> Result<Entity<Self>> {
        let remote_id = response.payload.project_id;
        let role = response.payload.role();

        let path_style = if response.payload.windows_paths {
            PathStyle::Windows
        } else {
            PathStyle::Posix
        };

        let worktree_store = cx.new(|cx| {
            WorktreeStore::remote(
                true,
                client.clone().into(),
                response.payload.project_id,
                path_style,
                WorktreeIdCounter::get(cx),
            )
        });
        let buffer_store = cx.new(|cx| {
            BufferStore::remote(worktree_store.clone(), client.clone().into(), remote_id, cx)
        });
        let image_store = cx.new(|cx| {
            ImageStore::remote(worktree_store.clone(), client.clone().into(), remote_id, cx)
        });

        let environment =
            cx.new(|cx| ProjectEnvironment::new(None, worktree_store.downgrade(), None, true, cx));

        let bookmark_store =
            cx.new(|_| BookmarkStore::new(worktree_store.clone(), buffer_store.clone()));

        let breakpoint_store = cx.new(|_| {
            BreakpointStore::remote(
                remote_id,
                client.clone().into(),
                buffer_store.clone(),
                worktree_store.clone(),
            )
        });
        let dap_store = cx.new(|cx| {
            DapStore::new_collab(
                remote_id,
                client.clone().into(),
                breakpoint_store.clone(),
                worktree_store.clone(),
                fs.clone(),
                cx,
            )
        });

        let lsp_store = cx.new(|cx| {
            LspStore::new_remote(
                buffer_store.clone(),
                worktree_store.clone(),
                languages.clone(),
                client.clone().into(),
                remote_id,
                cx,
            )
        });

        let git_store = cx.new(|cx| {
            GitStore::remote(
                // In this remote case we pass None for the environment
                &worktree_store,
                buffer_store.clone(),
                client.clone().into(),
                remote_id,
                cx,
            )
        });

        let task_store = cx.new(|cx| {
            if run_tasks {
                TaskStore::remote(
                    buffer_store.downgrade(),
                    worktree_store.clone(),
                    Arc::new(EmptyToolchainStore),
                    client.clone().into(),
                    remote_id,
                    git_store.clone(),
                    cx,
                )
            } else {
                TaskStore::Noop
            }
        });

        let settings_observer = cx.new(|cx| {
            SettingsObserver::new_remote(
                fs.clone(),
                worktree_store.clone(),
                task_store.clone(),
                None,
                true,
                cx,
            )
        });

        let agent_server_store = cx.new(|_cx| AgentServerStore::collab());
        let replica_id = ReplicaId::new(response.payload.replica_id as u16);

        let project = cx.new(|cx| {
            let snippets = SnippetProvider::new(fs.clone(), BTreeSet::from_iter([]), cx);

            let weak_self = cx.weak_entity();
            let context_server_store = cx.new(|cx| {
                ContextServerStore::local(worktree_store.clone(), Some(weak_self), false, cx)
            });

            let mut worktrees = Vec::new();
            for worktree in response.payload.worktrees {
                let worktree = Worktree::remote(
                    remote_id,
                    replica_id,
                    worktree,
                    client.clone().into(),
                    path_style,
                    cx,
                );
                worktrees.push(worktree);
            }

            let (tx, rx) = mpsc::unbounded();
            cx.spawn(async move |this, cx| Self::send_buffer_ordered_messages(this, rx, cx).await)
                .detach();

            cx.subscribe(&worktree_store, Self::on_worktree_store_event)
                .detach();

            cx.subscribe(&buffer_store, Self::on_buffer_store_event)
                .detach();
            cx.subscribe(&lsp_store, Self::on_lsp_store_event).detach();
            cx.subscribe(&settings_observer, Self::on_settings_observer_event)
                .detach();

            cx.subscribe(&dap_store, Self::on_dap_store_event).detach();

            let mut project = Self {
                buffer_ordered_messages_tx: tx,
                buffer_store: buffer_store.clone(),
                image_store,
                worktree_store: worktree_store.clone(),
                lsp_store: lsp_store.clone(),
                context_server_store,
                active_entry: None,
                collaborators: Default::default(),
                join_project_response_message_id: response.message_id,
                languages,
                user_store: user_store.clone(),
                task_store,
                snippets,
                fs,
                remote_client: None,
                settings_observer: settings_observer.clone(),
                client_subscriptions: Default::default(),
                _subscriptions: vec![cx.on_release(Self::release)],
                collab_client: client.clone(),
                client_state: ProjectClientState::Collab {
                    sharing_has_stopped: false,
                    capability: Capability::ReadWrite,
                    remote_id,
                    replica_id,
                },
                bookmark_store: bookmark_store.clone(),
                breakpoint_store: breakpoint_store.clone(),
                dap_store: dap_store.clone(),
                git_store: git_store.clone(),
                agent_server_store,
                buffers_needing_diff: Default::default(),
                git_diff_debouncer: DebouncedDelay::new(),
                terminals: Terminals {
                    local_handles: Vec::new(),
                },
                node: None,
                search_history: new_search_history(),
                search_included_history: new_search_history(),
                search_excluded_history: new_search_history(),
                environment,
                remotely_created_models: Arc::new(Mutex::new(RemotelyCreatedModels::default())),
                toolchain_store: None,
                agent_location: None,
                downloading_files: Default::default(),
                last_worktree_paths: WorktreePaths::default(),
            };
            project.set_role(role, cx);
            for worktree in worktrees {
                project.add_worktree(&worktree, cx);
            }
            project
        });

        let weak_project = project.downgrade();
        lsp_store.update(&mut cx, |lsp_store, cx| {
            lsp_store.set_language_server_statuses_from_proto(
                weak_project,
                response.payload.language_servers,
                response.payload.language_server_capabilities,
                cx,
            );
        });

        let subscriptions = subscriptions
            .into_iter()
            .map(|s| match s {
                EntitySubscription::BufferStore(subscription) => {
                    subscription.set_entity(&buffer_store, &cx)
                }
                EntitySubscription::WorktreeStore(subscription) => {
                    subscription.set_entity(&worktree_store, &cx)
                }
                EntitySubscription::GitStore(subscription) => {
                    subscription.set_entity(&git_store, &cx)
                }
                EntitySubscription::SettingsObserver(subscription) => {
                    subscription.set_entity(&settings_observer, &cx)
                }
                EntitySubscription::Project(subscription) => subscription.set_entity(&project, &cx),
                EntitySubscription::LspStore(subscription) => {
                    subscription.set_entity(&lsp_store, &cx)
                }
                EntitySubscription::DapStore(subscription) => {
                    subscription.set_entity(&dap_store, &cx)
                }
                EntitySubscription::BreakpointStore(subscription) => {
                    subscription.set_entity(&breakpoint_store, &cx)
                }
            })
            .collect::<Vec<_>>();

        let user_ids = response
            .payload
            .collaborators
            .iter()
            .map(|peer| peer.user_id)
            .collect();
        user_store
            .update(&mut cx, |user_store, cx| user_store.get_users(user_ids, cx))
            .await?;

        project.update(&mut cx, |this, cx| {
            this.set_collaborators_from_proto(response.payload.collaborators, cx)?;
            this.client_subscriptions.extend(subscriptions);
            anyhow::Ok(())
        })?;

        Ok(project)
    }
}
