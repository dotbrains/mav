use super::*;

impl LspStore {
    pub fn running_language_servers_for_local_buffer<'a>(
        &'a self,
        buffer: &Buffer,
        cx: &mut App,
    ) -> impl Iterator<Item = (&'a Arc<CachedLspAdapter>, &'a Arc<LanguageServer>)> {
        let local = self.as_local();
        let language_server_ids = local
            .map(|local| local.language_server_ids_for_buffer(buffer, cx))
            .unwrap_or_default();

        language_server_ids
            .into_iter()
            .filter_map(
                move |server_id| match local?.language_servers.get(&server_id)? {
                    LanguageServerState::Running {
                        adapter, server, ..
                    } => Some((adapter, server)),
                    _ => None,
                },
            )
    }

    pub fn language_servers_for_local_buffer(
        &self,
        buffer: &Buffer,
        cx: &mut App,
    ) -> Vec<LanguageServerId> {
        let local = self.as_local();
        local
            .map(|local| local.language_server_ids_for_buffer(buffer, cx))
            .unwrap_or_default()
    }

    pub fn language_server_for_local_buffer<'a>(
        &'a self,
        buffer: &'a Buffer,
        server_id: LanguageServerId,
        cx: &'a mut App,
    ) -> Option<(&'a Arc<CachedLspAdapter>, &'a Arc<LanguageServer>)> {
        self.as_local()?
            .language_servers_for_buffer(buffer, cx)
            .find(|(_, s)| s.server_id() == server_id)
    }

    pub(super) fn remove_worktree(&mut self, id_to_remove: WorktreeId, cx: &mut Context<Self>) {
        self.diagnostic_summaries.remove(&id_to_remove);
        if let Some(local) = self.as_local_mut() {
            let to_remove = local.remove_worktree(id_to_remove, cx);
            for server in to_remove {
                self.language_server_statuses.remove(&server);
            }
        }
    }

    pub(super) fn invalidate_diagnostic_summaries_for_removed_entries(
        &mut self,
        worktree_id: WorktreeId,
        changes: &UpdatedEntriesSet,
        cx: &mut Context<Self>,
    ) {
        let Some(summaries_for_tree) = self.diagnostic_summaries.get_mut(&worktree_id) else {
            return;
        };

        let mut cleared_paths: Vec<ProjectPath> = Vec::new();
        let mut cleared_server_ids: HashSet<LanguageServerId> = HashSet::default();
        let downstream = self.downstream_client.clone();

        for (path, _, _) in changes
            .iter()
            .filter(|(_, _, change)| *change == PathChange::Removed)
        {
            if let Some(summaries_by_server_id) = summaries_for_tree.remove(path) {
                for (server_id, _) in &summaries_by_server_id {
                    cleared_server_ids.insert(*server_id);
                    if let Some((client, project_id)) = &downstream {
                        client
                            .send(proto::UpdateDiagnosticSummary {
                                project_id: *project_id,
                                worktree_id: worktree_id.to_proto(),
                                summary: Some(proto::DiagnosticSummary {
                                    path: path.as_ref().to_proto(),
                                    language_server_id: server_id.0 as u64,
                                    error_count: 0,
                                    warning_count: 0,
                                }),
                                more_summaries: Vec::new(),
                            })
                            .ok();
                    }
                }
                cleared_paths.push(ProjectPath {
                    worktree_id,
                    path: path.clone(),
                });
            }
        }

        if !cleared_paths.is_empty() {
            for server_id in cleared_server_ids {
                cx.emit(LspStoreEvent::DiagnosticsUpdated {
                    server_id,
                    paths: cleared_paths.clone(),
                });
            }
        }
    }
}
