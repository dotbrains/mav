use super::*;

impl LspStore {
    pub(super) fn register_supplementary_language_server(
        &mut self,
        id: LanguageServerId,
        name: LanguageServerName,
        server: Arc<LanguageServer>,
        cx: &mut Context<Self>,
    ) {
        if let Some(local) = self.as_local_mut() {
            local
                .supplementary_language_servers
                .insert(id, (name.clone(), server));
            cx.emit(LspStoreEvent::LanguageServerAdded(id, name, None));
        }
    }

    pub(super) fn unregister_supplementary_language_server(
        &mut self,
        id: LanguageServerId,
        cx: &mut Context<Self>,
    ) {
        if let Some(local) = self.as_local_mut() {
            local.supplementary_language_servers.remove(&id);
            cx.emit(LspStoreEvent::LanguageServerRemoved(id));
        }
    }

    pub(crate) fn supplementary_language_servers(
        &self,
    ) -> impl '_ + Iterator<Item = (LanguageServerId, LanguageServerName)> {
        self.as_local().into_iter().flat_map(|local| {
            local
                .supplementary_language_servers
                .iter()
                .map(|(id, (name, _))| (*id, name.clone()))
        })
    }

    pub fn language_server_adapter_for_id(
        &self,
        id: LanguageServerId,
    ) -> Option<Arc<CachedLspAdapter>> {
        if let Some(local) = self.as_local()
            && let Some(LanguageServerState::Running { adapter, .. }) =
                local.language_servers.get(&id)
        {
            return Some(adapter.clone());
        }
        // In remote (SSH/collab) mode there are no local `language_servers`, but
        // `language_server_statuses` is kept in sync with the upstream and carries each
        // server's registered name, which is enough to look the adapter up in the registry.
        let name = &self.language_server_statuses.get(&id)?.name;
        self.languages.adapter_for_name(name)
    }
}
