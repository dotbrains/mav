use super::*;

impl LocalLspStore {
    pub(super) fn remove_worktree(
        &mut self,
        id_to_remove: WorktreeId,
        cx: &mut Context<LspStore>,
    ) -> Vec<LanguageServerId> {
        self.restricted_worktrees_tasks.remove(&id_to_remove);
        self.diagnostics.remove(&id_to_remove);
        self.prettier_store.update(cx, |prettier_store, cx| {
            prettier_store.remove_worktree(id_to_remove, cx);
        });

        let mut servers_to_remove = BTreeSet::default();
        let mut servers_to_preserve = HashSet::default();
        for (seed, state) in &self.language_server_ids {
            if seed.worktree_id == id_to_remove {
                servers_to_remove.insert(state.id);
            } else {
                servers_to_preserve.insert(state.id);
            }
        }
        servers_to_remove.retain(|server_id| !servers_to_preserve.contains(server_id));
        self.language_server_ids.retain(|seed, state| {
            seed.worktree_id != id_to_remove && !servers_to_remove.contains(&state.id)
        });
        self.lsp_tree.instances.remove(&id_to_remove);
        for server_id_to_remove in &servers_to_remove {
            self.language_server_watched_paths
                .remove(server_id_to_remove);
            self.language_server_paths_watched_for_rename
                .remove(server_id_to_remove);
            self.last_workspace_edits_by_language_server
                .remove(server_id_to_remove);
            self.language_servers.remove(server_id_to_remove);
            self.buffer_pull_diagnostics_result_ids
                .remove(server_id_to_remove);
            self.workspace_pull_diagnostics_result_ids
                .remove(server_id_to_remove);
            for buffer_servers in self.buffers_opened_in_servers.values_mut() {
                buffer_servers.remove(server_id_to_remove);
            }
            cx.emit(LspStoreEvent::LanguageServerRemoved(*server_id_to_remove));
        }
        servers_to_remove.into_iter().collect()
    }

    pub(super) async fn initialization_options_for_adapter(
        adapter: Arc<dyn LspAdapter>,
        delegate: &Arc<dyn LspAdapterDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<Option<serde_json::Value>> {
        let Some(mut initialization_config) =
            adapter.clone().initialization_options(delegate, cx).await?
        else {
            return Ok(None);
        };

        for other_adapter in delegate.registered_lsp_adapters() {
            if other_adapter.name() == adapter.name() {
                continue;
            }
            if let Ok(Some(target_config)) = other_adapter
                .clone()
                .additional_initialization_options(adapter.name(), delegate)
                .await
            {
                merge_json_value_into(target_config.clone(), &mut initialization_config);
            }
        }

        Ok(Some(initialization_config))
    }

    pub(super) async fn workspace_configuration_for_adapter(
        adapter: Arc<dyn LspAdapter>,
        delegate: &Arc<dyn LspAdapterDelegate>,
        toolchain: Option<Toolchain>,
        requested_uri: Option<Uri>,
        cx: &mut AsyncApp,
    ) -> Result<serde_json::Value> {
        let mut workspace_config = adapter
            .clone()
            .workspace_configuration(delegate, toolchain, requested_uri, cx)
            .await?;

        for other_adapter in delegate.registered_lsp_adapters() {
            if other_adapter.name() == adapter.name() {
                continue;
            }
            if let Ok(Some(target_config)) = other_adapter
                .clone()
                .additional_workspace_configuration(adapter.name(), delegate, cx)
                .await
            {
                merge_json_value_into(target_config.clone(), &mut workspace_config);
            }
        }

        Ok(workspace_config)
    }

    pub(super) fn language_server_for_id(
        &self,
        id: LanguageServerId,
    ) -> Option<Arc<LanguageServer>> {
        if let Some(LanguageServerState::Running { server, .. }) = self.language_servers.get(&id) {
            Some(server.clone())
        } else if let Some((_, server)) = self.supplementary_language_servers.get(&id) {
            Some(Arc::clone(server))
        } else {
            None
        }
    }
}
