use super::*;

impl LspStore {
    pub fn shared(
        &mut self,
        project_id: u64,
        downstream_client: AnyProtoClient,
        _: &mut Context<Self>,
    ) {
        self.downstream_client = Some((downstream_client.clone(), project_id));

        for (server_id, status) in &self.language_server_statuses {
            if let Some(server) = self.language_server_for_id(*server_id) {
                downstream_client
                    .send(proto::StartLanguageServer {
                        project_id,
                        server: Some(proto::LanguageServer {
                            id: server_id.to_proto(),
                            name: status.name.to_string(),
                            worktree_id: status.worktree.map(|id| id.to_proto()),
                            language_name: status
                                .language_name
                                .as_ref()
                                .map(|name| name.to_proto()),
                        }),
                        capabilities: serde_json::to_string(&server.capabilities())
                            .expect("serializing server LSP capabilities"),
                    })
                    .log_err();
            }
        }
    }

    pub fn disconnected_from_host(&mut self) {
        self.downstream_client.take();
    }

    pub fn disconnected_from_ssh_remote(&mut self) {
        if let LspStoreMode::Remote(RemoteLspStore {
            upstream_client, ..
        }) = &mut self.mode
        {
            upstream_client.take();
        }
    }

    pub(crate) fn set_language_server_statuses_from_proto(
        &mut self,
        project: WeakEntity<Project>,
        language_servers: Vec<proto::LanguageServer>,
        server_capabilities: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let lsp_logs = cx
            .try_global::<GlobalLogStore>()
            .map(|lsp_store| lsp_store.0.clone());

        self.language_server_statuses = language_servers
            .into_iter()
            .zip(server_capabilities)
            .map(|(server, server_capabilities)| {
                let server_id = LanguageServerId(server.id as usize);
                if let Ok(server_capabilities) = serde_json::from_str(&server_capabilities) {
                    self.lsp_server_capabilities
                        .insert(server_id, server_capabilities);
                }

                let name = LanguageServerName::from_proto(server.name);
                let worktree = server.worktree_id.map(WorktreeId::from_proto);
                let language_name = server.language_name.map(LanguageName::from_proto);

                if let Some(lsp_logs) = &lsp_logs {
                    lsp_logs.update(cx, |lsp_logs, cx| {
                        lsp_logs.add_language_server(
                            // Only remote clients get their language servers set from proto
                            LanguageServerKind::Remote {
                                project: project.clone(),
                            },
                            server_id,
                            Some(name.clone()),
                            worktree,
                            None,
                            cx,
                        );
                    });
                }

                if let Some(ref lang_name) = language_name {
                    self.try_register_remote_adapter_locally(&name, lang_name);
                }

                (
                    server_id,
                    LanguageServerStatus {
                        name,
                        language_name: language_name,
                        server_version: None,
                        server_readable_version: None,
                        pending_work: Default::default(),
                        has_pending_diagnostic_updates: false,
                        progress_tokens: Default::default(),
                        worktree,
                        binary: None,
                        configuration: None,
                        workspace_folders: BTreeSet::new(),
                        process_id: None,
                    },
                )
            })
            .collect();
    }

    pub(super) fn try_register_remote_adapter_locally(
        &self,
        server_name: &LanguageServerName,
        language_name: &LanguageName,
    ) {
        let already_registered = self
            .languages
            .lsp_adapters(language_name)
            .iter()
            .any(|adapter| adapter.name() == *server_name);

        if already_registered {
            return;
        }

        if let Some(adapter) = self.languages.load_available_lsp_adapter(server_name) {
            log::info!(
                "Registering LSP adapter '{}' for language '{}' on local client",
                server_name.0,
                language_name.0
            );
            self.languages
                .register_lsp_adapter(language_name.clone(), adapter.adapter.clone());
        } else {
            log::warn!(
                "LSP adapter '{}' for language '{}' not available locally",
                server_name.0,
                language_name.0
            );
        }
    }
}
