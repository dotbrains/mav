use super::*;

impl Project {
    pub fn set_language_for_buffer(
        &mut self,
        buffer: &Entity<Buffer>,
        new_language: Arc<Language>,
        cx: &mut Context<Self>,
    ) {
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.set_language_for_buffer(buffer, new_language, cx)
        })
    }

    pub fn restart_language_servers_for_buffers(
        &mut self,
        buffers: Vec<Entity<Buffer>>,
        only_restart_servers: HashSet<LanguageServerSelector>,
        clear_stopped: bool,
        cx: &mut Context<Self>,
    ) {
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.restart_language_servers_for_buffers(
                buffers,
                only_restart_servers,
                clear_stopped,
                cx,
            )
        })
    }

    pub fn stop_language_servers_for_buffers(
        &mut self,
        buffers: Vec<Entity<Buffer>>,
        also_restart_servers: HashSet<LanguageServerSelector>,
        cx: &mut Context<Self>,
    ) {
        self.lsp_store
            .update(cx, |lsp_store, cx| {
                lsp_store.stop_language_servers_for_buffers(buffers, also_restart_servers, cx)
            })
            .detach_and_log_err(cx);
    }

    pub fn cancel_language_server_work_for_buffers(
        &mut self,
        buffers: impl IntoIterator<Item = Entity<Buffer>>,
        cx: &mut Context<Self>,
    ) {
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.cancel_language_server_work_for_buffers(buffers, cx)
        })
    }

    pub fn cancel_language_server_work(
        &mut self,
        server_id: LanguageServerId,
        token_to_cancel: Option<ProgressToken>,
        cx: &mut Context<Self>,
    ) {
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.cancel_language_server_work(server_id, token_to_cancel, cx)
        })
    }

    fn enqueue_buffer_ordered_message(&mut self, message: BufferOrderedMessage) -> Result<()> {
        self.buffer_ordered_messages_tx
            .unbounded_send(message)
            .map_err(|e| anyhow!(e))
    }

    pub fn available_toolchains(
        &self,
        path: ProjectPath,
        language_name: LanguageName,
        cx: &App,
    ) -> Task<Option<Toolchains>> {
        if let Some(toolchain_store) = self.toolchain_store.as_ref().map(Entity::downgrade) {
            cx.spawn(async move |cx| {
                toolchain_store
                    .update(cx, |this, cx| this.list_toolchains(path, language_name, cx))
                    .ok()?
                    .await
            })
        } else {
            Task::ready(None)
        }
    }

    pub async fn toolchain_metadata(
        languages: Arc<LanguageRegistry>,
        language_name: LanguageName,
    ) -> Option<ToolchainMetadata> {
        languages
            .language_for_name(language_name.as_ref())
            .await
            .ok()?
            .toolchain_lister()
            .map(|lister| lister.meta())
    }

    pub fn add_toolchain(
        &self,
        toolchain: Toolchain,
        scope: ToolchainScope,
        cx: &mut Context<Self>,
    ) {
        maybe!({
            self.toolchain_store.as_ref()?.update(cx, |this, cx| {
                this.add_toolchain(toolchain, scope, cx);
            });
            Some(())
        });
    }

    pub fn remove_toolchain(
        &self,
        toolchain: Toolchain,
        scope: ToolchainScope,
        cx: &mut Context<Self>,
    ) {
        maybe!({
            self.toolchain_store.as_ref()?.update(cx, |this, cx| {
                this.remove_toolchain(toolchain, scope, cx);
            });
            Some(())
        });
    }

    pub fn user_toolchains(
        &self,
        cx: &App,
    ) -> Option<BTreeMap<ToolchainScope, IndexSet<Toolchain>>> {
        Some(self.toolchain_store.as_ref()?.read(cx).user_toolchains())
    }

    pub fn resolve_toolchain(
        &self,
        path: PathBuf,
        language_name: LanguageName,
        cx: &App,
    ) -> Task<Result<Toolchain>> {
        if let Some(toolchain_store) = self.toolchain_store.as_ref().map(Entity::downgrade) {
            cx.spawn(async move |cx| {
                toolchain_store
                    .update(cx, |this, cx| {
                        this.resolve_toolchain(path, language_name, cx)
                    })?
                    .await
            })
        } else {
            Task::ready(Err(anyhow!("This project does not support toolchains")))
        }
    }

    pub fn toolchain_store(&self) -> Option<Entity<ToolchainStore>> {
        self.toolchain_store.clone()
    }
    pub fn activate_toolchain(
        &self,
        path: ProjectPath,
        toolchain: Toolchain,
        cx: &mut App,
    ) -> Task<Option<()>> {
        let Some(toolchain_store) = self.toolchain_store.clone() else {
            return Task::ready(None);
        };
        toolchain_store.update(cx, |this, cx| this.activate_toolchain(path, toolchain, cx))
    }
    pub fn active_toolchain(
        &self,
        path: ProjectPath,
        language_name: LanguageName,
        cx: &App,
    ) -> Task<Option<Toolchain>> {
        let Some(toolchain_store) = self.toolchain_store.clone() else {
            return Task::ready(None);
        };
        toolchain_store
            .read(cx)
            .active_toolchain(path, language_name, cx)
    }
    pub fn language_server_statuses<'a>(
        &'a self,
        cx: &'a App,
    ) -> impl DoubleEndedIterator<Item = (LanguageServerId, &'a LanguageServerStatus)> {
        self.lsp_store.read(cx).language_server_statuses()
    }

    pub fn last_formatting_failure<'a>(&self, cx: &'a App) -> Option<&'a str> {
        self.lsp_store.read(cx).last_formatting_failure()
    }

    pub fn reset_last_formatting_failure(&self, cx: &mut App) {
        self.lsp_store
            .update(cx, |store, _| store.reset_last_formatting_failure());
    }
}
