use super::*;

impl LspStore {
    pub fn buffer_store(&self) -> Entity<BufferStore> {
        self.buffer_store.clone()
    }

    pub fn set_active_entry(&mut self, active_entry: Option<ProjectEntryId>) {
        self.active_entry = active_entry;
    }

    pub(crate) fn send_diagnostic_summaries(&self, worktree: &mut Worktree) {
        if let Some((client, downstream_project_id)) = self.downstream_client.clone()
            && let Some(diangostic_summaries) = self.diagnostic_summaries.get(&worktree.id())
        {
            let mut summaries = diangostic_summaries.iter().flat_map(|(path, summaries)| {
                summaries
                    .iter()
                    .map(|(server_id, summary)| summary.to_proto(*server_id, path.as_ref()))
            });
            if let Some(summary) = summaries.next() {
                client
                    .send(proto::UpdateDiagnosticSummary {
                        project_id: downstream_project_id,
                        worktree_id: worktree.id().to_proto(),
                        summary: Some(summary),
                        more_summaries: summaries.collect(),
                    })
                    .log_err();
            }
        }
    }

    pub fn language_server_statuses(
        &self,
    ) -> impl DoubleEndedIterator<Item = (LanguageServerId, &LanguageServerStatus)> {
        self.language_server_statuses
            .iter()
            .map(|(key, value)| (*key, value))
    }

    #[cfg(feature = "test-support")]
    pub fn has_language_server_seed_for_worktree(&self, worktree_id: WorktreeId) -> bool {
        self.as_local().is_some_and(|local| {
            local
                .language_server_ids
                .keys()
                .any(|seed| seed.worktree_id == worktree_id)
        })
    }

    pub fn language_server_for_id(&self, id: LanguageServerId) -> Option<Arc<LanguageServer>> {
        self.as_local()?.language_server_for_id(id)
    }

    pub fn wait_for_remote_buffer(
        &mut self,
        id: BufferId,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        self.buffer_store.update(cx, |buffer_store, cx| {
            buffer_store.wait_for_remote_buffer(id, cx)
        })
    }

    pub fn downstream_client(&self) -> Option<(AnyProtoClient, u64)> {
        self.downstream_client.clone()
    }

    pub fn worktree_store(&self) -> Entity<WorktreeStore> {
        self.worktree_store.clone()
    }

    /// Gets what's stored in the LSP data for the given buffer.
    pub fn current_lsp_data(&mut self, buffer_id: BufferId) -> Option<&mut BufferLspData> {
        self.lsp_data.get_mut(&buffer_id)
    }

    /// Gets the most recent LSP data for the given buffer: if the data is absent or out of date,
    /// new [`BufferLspData`] will be created to replace the previous state.
    pub fn latest_lsp_data(&mut self, buffer: &Entity<Buffer>, cx: &mut App) -> &mut BufferLspData {
        let (buffer_id, buffer_version) =
            buffer.read_with(cx, |buffer, _| (buffer.remote_id(), buffer.version()));
        let lsp_data = self
            .lsp_data
            .entry(buffer_id)
            .or_insert_with(|| BufferLspData::new(buffer, cx));
        if buffer_version.changed_since(&lsp_data.buffer_version) {
            // To send delta requests for semantic tokens, the previous tokens
            // need to be kept between buffer changes.
            let semantic_tokens = lsp_data.semantic_tokens.take();
            *lsp_data = BufferLspData::new(buffer, cx);
            lsp_data.semantic_tokens = semantic_tokens;
        }
        lsp_data
    }
}
