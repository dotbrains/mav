use super::*;

impl LspStore {
    pub(super) fn on_settings_changed(&mut self, cx: &mut Context<Self>) {
        let mut language_formatters_to_check = Vec::new();
        for buffer in self.buffer_store.read(cx).buffers() {
            let buffer = buffer.read(cx);
            let settings = LanguageSettings::for_buffer(buffer, cx);
            if buffer.language().is_some() {
                let buffer_file = File::from_dyn(buffer.file());
                language_formatters_to_check.push((
                    buffer_file.map(|f| f.worktree_id(cx)),
                    settings.into_owned(),
                ));
            }
        }

        self.request_workspace_config_refresh();

        if let Some(prettier_store) = self.as_local().map(|s| s.prettier_store.clone()) {
            prettier_store.update(cx, |prettier_store, cx| {
                prettier_store.on_settings_changed(language_formatters_to_check, cx)
            })
        }

        let new_semantic_token_rules = crate::project_settings::ProjectSettings::get_global(cx)
            .global_lsp_settings
            .semantic_token_rules
            .clone();
        self.semantic_token_config
            .update_rules(new_semantic_token_rules);
        // Always clear cached stylizers so that changes to language-specific
        // semantic token rules (e.g. from extension install/uninstall) are
        // picked up. Stylizers are recreated lazily, so this is cheap.
        self.semantic_token_config.clear_stylizers();

        let new_global_semantic_tokens_mode =
            all_language_settings(None, cx).defaults.semantic_tokens;
        if self
            .semantic_token_config
            .update_global_mode(new_global_semantic_tokens_mode)
        {
            let all_stopped = self
                .as_local()
                .is_some_and(|local| local.all_language_servers_stopped);
            if !all_stopped {
                // Restart servers without clearing per-server stopped status.
                // Individually-stopped servers will be skipped by the guard in
                // register_buffer_with_language_servers.
                let buffers = self.buffer_store.read(cx).buffers().collect();
                self.restart_language_servers_for_buffers(buffers, HashSet::default(), false, cx);
            }
        }

        cx.notify();
    }

    pub(super) fn refresh_server_tree(&mut self, cx: &mut Context<Self>) {
        let buffer_store = self.buffer_store.clone();
        let Some(local) = self.as_local_mut() else {
            return;
        };
        if local.all_language_servers_stopped {
            return;
        }
        let stopped_language_servers = local.stopped_language_servers.clone();
        let mut adapters = BTreeMap::default();
        let get_adapter = {
            let languages = local.languages.clone();
            let environment = local.environment.clone();
            let weak = local.weak.clone();
            let worktree_store = local.worktree_store.clone();
            let http_client = local.http_client.clone();
            let fs = local.fs.clone();
            move |worktree_id, cx: &mut App| {
                let worktree = worktree_store.read(cx).worktree_for_id(worktree_id, cx)?;
                Some(LocalLspAdapterDelegate::new(
                    languages.clone(),
                    &environment,
                    weak.clone(),
                    &worktree,
                    http_client.clone(),
                    fs.clone(),
                    cx,
                ))
            }
        };

        let mut messages_to_report = Vec::new();
        let (new_tree, to_stop) = {
            let mut rebase = local.lsp_tree.rebase();
            let buffers = buffer_store
                .read(cx)
                .buffers()
                .filter_map(|buffer| {
                    let raw_buffer = buffer.read(cx);
                    if !local
                        .registered_buffers
                        .contains_key(&raw_buffer.remote_id())
                    {
                        return None;
                    }
                    let file = File::from_dyn(raw_buffer.file()).cloned()?;
                    let language = raw_buffer.language().cloned()?;
                    Some((file, language, raw_buffer.remote_id()))
                })
                .sorted_by_key(|(file, _, _)| Reverse(file.worktree.read(cx).is_visible()));
            for (file, language, buffer_id) in buffers {
                let worktree_id = file.worktree_id(cx);
                let Some(worktree) = local
                    .worktree_store
                    .read(cx)
                    .worktree_for_id(worktree_id, cx)
                else {
                    continue;
                };

                if let Some((_, apply)) = local.reuse_existing_language_server(
                    rebase.server_tree(),
                    &worktree,
                    &language.name(),
                    cx,
                ) {
                    (apply)(rebase.server_tree());
                } else if let Some(lsp_delegate) = adapters
                    .entry(worktree_id)
                    .or_insert_with(|| get_adapter(worktree_id, cx))
                    .clone()
                {
                    let delegate =
                        Arc::new(ManifestQueryDelegate::new(worktree.read(cx).snapshot()));
                    let path = file
                        .path()
                        .parent()
                        .map(Arc::from)
                        .unwrap_or_else(|| file.path().clone());
                    let worktree_path = ProjectPath { worktree_id, path };
                    let abs_path = file.abs_path(cx);
                    let nodes = rebase
                        .walk(
                            worktree_path,
                            language.name(),
                            language.manifest(),
                            delegate.clone(),
                            cx,
                        )
                        .collect::<Vec<_>>();
                    for node in nodes {
                        if let Some(name) = node.name()
                            && stopped_language_servers.contains(&name)
                        {
                            continue;
                        }
                        let server_id = node.server_id_or_init(|disposition| {
                            let path = &disposition.path;
                            let uri = Uri::from_file_path(worktree.read(cx).absolutize(&path.path));
                            let key = LanguageServerSeed {
                                worktree_id,
                                name: disposition.server_name.clone(),
                                settings: LanguageServerSeedSettings {
                                    binary: disposition.settings.binary.clone(),
                                    initialization_options: disposition
                                        .settings
                                        .initialization_options
                                        .clone(),
                                },
                                toolchain: local.toolchain_store.read(cx).active_toolchain(
                                    path.worktree_id,
                                    &path.path,
                                    language.name(),
                                ),
                            };
                            local.language_server_ids.remove(&key);

                            let server_id = local.get_or_insert_language_server(
                                &worktree,
                                lsp_delegate.clone(),
                                disposition,
                                &language.name(),
                                cx,
                            );
                            if let Some(state) = local.language_servers.get(&server_id)
                                && let Ok(uri) = uri
                            {
                                state.add_workspace_folder(uri);
                            };
                            server_id
                        });

                        if let Some(language_server_id) = server_id {
                            messages_to_report.push(LspStoreEvent::LanguageServerUpdate {
                                language_server_id,
                                name: node.name(),
                                message:
                                    proto::update_language_server::Variant::RegisteredForBuffer(
                                        proto::RegisteredForBuffer {
                                            buffer_abs_path: abs_path
                                                .to_string_lossy()
                                                .into_owned(),
                                            buffer_id: buffer_id.to_proto(),
                                        },
                                    ),
                            });
                        }
                    }
                } else {
                    continue;
                }
            }
            rebase.finish()
        };
        for message in messages_to_report {
            cx.emit(message);
        }
        local.lsp_tree = new_tree;
        for (id, _) in to_stop {
            self.stop_local_language_server(id, cx).detach();
        }
    }
}
