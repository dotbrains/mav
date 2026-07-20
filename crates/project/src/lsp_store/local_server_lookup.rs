use super::*;

impl LocalLspStore {
    pub(super) fn shutdown_language_servers_on_quit(&mut self) -> impl Future<Output = ()> + use<> {
        let shutdown_futures = self
            .language_servers
            .drain()
            .map(|(_, server_state)| Self::shutdown_server(server_state))
            .collect::<Vec<_>>();

        async move {
            join_all(shutdown_futures).await;
        }
    }

    pub(super) async fn shutdown_server(server_state: LanguageServerState) -> anyhow::Result<()> {
        match server_state {
            LanguageServerState::Running { server, .. } => {
                if let Some(shutdown) = server.shutdown() {
                    shutdown.await;
                }
            }
            LanguageServerState::Starting { startup, .. } => {
                if let Some(server) = startup.await
                    && let Some(shutdown) = server.shutdown()
                {
                    shutdown.await;
                }
            }
        }
        Ok(())
    }

    pub(super) fn language_servers_for_worktree(
        &self,
        worktree_id: WorktreeId,
    ) -> impl Iterator<Item = &Arc<LanguageServer>> {
        self.language_server_ids
            .iter()
            .filter_map(move |(seed, state)| {
                if seed.worktree_id != worktree_id {
                    return None;
                }

                if let Some(LanguageServerState::Running { server, .. }) =
                    self.language_servers.get(&state.id)
                {
                    Some(server)
                } else {
                    None
                }
            })
    }

    pub(super) fn language_server_ids_for_project_path(
        &self,
        project_path: ProjectPath,
        language: &Language,
        cx: &mut App,
    ) -> Vec<LanguageServerId> {
        let Some(worktree) = self
            .worktree_store
            .read(cx)
            .worktree_for_id(project_path.worktree_id, cx)
        else {
            return Vec::new();
        };
        let delegate: Arc<dyn ManifestDelegate> =
            Arc::new(ManifestQueryDelegate::new(worktree.read(cx).snapshot()));

        self.lsp_tree
            .get(
                project_path,
                language.name(),
                language.manifest(),
                &delegate,
                cx,
            )
            .collect::<Vec<_>>()
    }

    pub(super) fn language_server_ids_for_buffer(
        &self,
        buffer: &Buffer,
        cx: &mut App,
    ) -> Vec<LanguageServerId> {
        if let Some((file, language)) = File::from_dyn(buffer.file()).zip(buffer.language()) {
            let worktree_id = file.worktree_id(cx);

            let path: Arc<RelPath> = file
                .path()
                .parent()
                .map(Arc::from)
                .unwrap_or_else(|| file.path().clone());
            let worktree_path = ProjectPath { worktree_id, path };
            self.language_server_ids_for_project_path(worktree_path, language, cx)
        } else {
            Vec::new()
        }
    }

    pub(super) fn language_servers_for_buffer<'a>(
        &'a self,
        buffer: &'a Buffer,
        cx: &'a mut App,
    ) -> impl Iterator<Item = (&'a Arc<CachedLspAdapter>, &'a Arc<LanguageServer>)> {
        self.language_server_ids_for_buffer(buffer, cx)
            .into_iter()
            .filter_map(|server_id| match self.language_servers.get(&server_id)? {
                LanguageServerState::Running {
                    adapter, server, ..
                } => Some((adapter, server)),
                _ => None,
            })
    }

    pub(super) async fn execute_code_action_kind_locally(
        lsp_store: WeakEntity<LspStore>,
        mut buffers: Vec<Entity<Buffer>>,
        kind: CodeActionKind,
        push_to_history: bool,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<ProjectTransaction> {
        // Do not allow multiple concurrent code actions requests for the
        // same buffer.
        lsp_store.update(cx, |this, cx| {
            let this = this.as_local_mut().unwrap();
            buffers.retain(|buffer| {
                this.buffers_being_formatted
                    .insert(buffer.read(cx).remote_id())
            });
        })?;
        let _cleanup = defer({
            let this = lsp_store.clone();
            let mut cx = cx.clone();
            let buffers = &buffers;
            move || {
                this.update(&mut cx, |this, cx| {
                    let this = this.as_local_mut().unwrap();
                    for buffer in buffers {
                        this.buffers_being_formatted
                            .remove(&buffer.read(cx).remote_id());
                    }
                })
                .ok();
            }
        });
        let mut project_transaction = ProjectTransaction::default();

        for buffer in &buffers {
            let adapters_and_servers = lsp_store.update(cx, |lsp_store, cx| {
                buffer.update(cx, |buffer, cx| {
                    lsp_store
                        .as_local()
                        .unwrap()
                        .language_servers_for_buffer(buffer, cx)
                        .map(|(adapter, lsp)| (adapter.clone(), lsp.clone()))
                        .collect::<Vec<_>>()
                })
            })?;
            for (_, language_server) in adapters_and_servers.iter() {
                let actions = Self::get_server_code_actions_from_action_kinds(
                    &lsp_store,
                    language_server.server_id(),
                    vec![kind.clone()],
                    buffer,
                    cx,
                )
                .await?;
                Self::execute_code_actions_on_server(
                    &lsp_store,
                    language_server,
                    actions,
                    push_to_history,
                    &mut project_transaction,
                    cx,
                )
                .await?;
            }
        }
        Ok(project_transaction)
    }
}
