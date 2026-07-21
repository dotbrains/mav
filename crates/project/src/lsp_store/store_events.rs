use super::*;

impl LspStore {
    pub(super) fn on_buffer_store_event(
        &mut self,
        _: Entity<BufferStore>,
        event: &BufferStoreEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            BufferStoreEvent::BufferAdded(buffer) => {
                self.on_buffer_added(buffer, cx).log_err();
            }
            BufferStoreEvent::BufferChangedFilePath { buffer, old_file } => {
                let buffer_id = buffer.read(cx).remote_id();
                if let Some(local) = self.as_local_mut()
                    && let Some(old_file) = File::from_dyn(old_file.as_ref())
                {
                    local.reset_buffer(buffer, old_file, cx);

                    if local.registered_buffers.contains_key(&buffer_id) {
                        local.unregister_old_buffer_from_language_servers(buffer, old_file, cx);
                    }
                }

                self.detect_language_for_buffer(buffer, cx);
                if let Some(local) = self.as_local_mut() {
                    local.initialize_buffer(buffer, cx);
                    if local.registered_buffers.contains_key(&buffer_id) {
                        local.register_buffer_with_language_servers(buffer, HashSet::default(), cx);
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn on_worktree_store_event(
        &mut self,
        _: Entity<WorktreeStore>,
        event: &WorktreeStoreEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            WorktreeStoreEvent::WorktreeAdded(worktree) => {
                if !worktree.read(cx).is_local() {
                    return;
                }
                cx.subscribe(worktree, |this, worktree, event, cx| match event {
                    worktree::Event::UpdatedEntries(changes) => {
                        this.update_local_worktree_language_servers(&worktree, changes, cx);
                    }
                    worktree::Event::UpdatedGitRepositories(_)
                    | worktree::Event::DeletedEntry(_)
                    | worktree::Event::Deleted
                    | worktree::Event::UpdatedRootRepoCommonDir { .. } => {}
                })
                .detach()
            }
            WorktreeStoreEvent::WorktreeRemoved(_, id) => self.remove_worktree(*id, cx),
            WorktreeStoreEvent::WorktreeUpdateSent(worktree) => {
                worktree.update(cx, |worktree, _cx| self.send_diagnostic_summaries(worktree));
            }
            WorktreeStoreEvent::WorktreeUpdatedEntries(worktree_id, changes) => {
                self.invalidate_diagnostic_summaries_for_removed_entries(*worktree_id, changes, cx);
            }
            WorktreeStoreEvent::WorktreeReleased(..)
            | WorktreeStoreEvent::WorktreeOrderChanged
            | WorktreeStoreEvent::WorktreeUpdatedGitRepositories(..)
            | WorktreeStoreEvent::WorktreeDeletedEntry(..)
            | WorktreeStoreEvent::WorktreeUpdatedRootRepoCommonDir(..) => {}
        }
    }

    pub(super) fn on_prettier_store_event(
        &mut self,
        _: Entity<PrettierStore>,
        event: &PrettierStoreEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PrettierStoreEvent::LanguageServerRemoved(prettier_server_id) => {
                self.unregister_supplementary_language_server(*prettier_server_id, cx);
            }
            PrettierStoreEvent::LanguageServerAdded {
                new_server_id,
                name,
                prettier_server,
            } => {
                self.register_supplementary_language_server(
                    *new_server_id,
                    name.clone(),
                    prettier_server.clone(),
                    cx,
                );
            }
        }
    }

    pub(super) fn on_toolchain_store_event(
        &mut self,
        _: Entity<LocalToolchainStore>,
        event: &ToolchainStoreEvent,
        _: &mut Context<Self>,
    ) {
        if let ToolchainStoreEvent::ToolchainActivated = event {
            self.request_workspace_config_refresh()
        }
    }

    pub(super) fn request_workspace_config_refresh(&mut self) {
        *self._maintain_workspace_config.1.borrow_mut() = ();
    }

    pub fn prettier_store(&self) -> Option<Entity<PrettierStore>> {
        self.as_local().map(|local| local.prettier_store.clone())
    }

    fn on_buffer_event(
        &mut self,
        buffer: Entity<Buffer>,
        event: &language::BufferEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            language::BufferEvent::Edited { .. } => {
                self.on_buffer_edited(buffer, cx);
            }

            language::BufferEvent::Saved => {
                self.on_buffer_saved(buffer, cx);
            }

            language::BufferEvent::Reloaded => {
                self.on_buffer_reloaded(buffer, cx);
            }

            _ => {}
        }
    }

    pub(super) fn on_buffer_added(
        &mut self,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        buffer
            .read(cx)
            .set_language_registry(self.languages.clone());

        cx.subscribe(buffer, |this, buffer, event, cx| {
            this.on_buffer_event(buffer, event, cx);
        })
        .detach();

        self.parse_modeline(buffer, cx);
        self.detect_language_for_buffer(buffer, cx);
        if let Some(local) = self.as_local_mut() {
            local.initialize_buffer(buffer, cx);
        }

        Ok(())
    }

    pub fn refresh_background_diagnostics_for_buffers(
        &mut self,
        buffers: HashSet<BufferId>,
        cx: &mut Context<Self>,
    ) -> Shared<Task<()>> {
        let Some(local) = self.as_local_mut() else {
            return Task::ready(()).shared();
        };
        for buffer in buffers {
            if local.buffers_to_refresh_hash_set.insert(buffer) {
                local.buffers_to_refresh_queue.push_back(buffer);
                if local.buffers_to_refresh_queue.len() == 1 {
                    local._background_diagnostics_worker =
                        Self::background_diagnostics_worker(cx).shared();
                }
            }
        }

        local._background_diagnostics_worker.clone()
    }

    pub(super) fn refresh_next_buffer(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let buffer_store = self.buffer_store.clone();
        let local = self.as_local_mut()?;
        while let Some(buffer_id) = local.buffers_to_refresh_queue.pop_front() {
            local.buffers_to_refresh_hash_set.remove(&buffer_id);
            if let Some(buffer) = buffer_store.read(cx).get(buffer_id) {
                return Some(self.pull_diagnostics_for_buffer(buffer, cx));
            }
        }
        None
    }

    pub(super) fn background_diagnostics_worker(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            while let Ok(Some(task)) = this.update(cx, |this, cx| this.refresh_next_buffer(cx)) {
                task.await.log_err();
            }
        })
    }

    pub(super) fn on_buffer_reloaded(&mut self, buffer: Entity<Buffer>, cx: &mut Context<Self>) {
        if self.parse_modeline(&buffer, cx) {
            self.detect_language_for_buffer(&buffer, cx);
        }

        let buffer_id = buffer.read(cx).remote_id();
        let task = self.pull_diagnostics_for_buffer(buffer, cx);
        self.buffer_reload_tasks.insert(buffer_id, task);
    }
}
