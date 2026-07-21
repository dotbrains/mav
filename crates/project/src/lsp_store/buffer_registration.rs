use super::*;

impl LocalLspStore {
    pub(super) fn register_buffer_with_language_servers(
        &mut self,
        buffer_handle: &Entity<Buffer>,
        only_register_servers: HashSet<LanguageServerSelector>,
        cx: &mut Context<LspStore>,
    ) {
        if self.all_language_servers_stopped {
            return;
        }
        let buffer = buffer_handle.read(cx);
        let buffer_id = buffer.remote_id();

        let Some(file) = File::from_dyn(buffer.file()) else {
            return;
        };
        if !file.is_local() {
            return;
        }

        let abs_path = file.abs_path(cx);
        let Some(uri) = file_path_to_lsp_url(&abs_path).log_err() else {
            return;
        };
        let initial_snapshot = buffer.text_snapshot();
        let worktree_id = file.worktree_id(cx);

        let Some(language) = buffer.language().cloned() else {
            return;
        };
        let path: Arc<RelPath> = file
            .path()
            .parent()
            .map(Arc::from)
            .unwrap_or_else(|| file.path().clone());
        let Some(worktree) = self
            .worktree_store
            .read(cx)
            .worktree_for_id(worktree_id, cx)
        else {
            return;
        };
        let language_name = language.name();
        let (reused, delegate, servers) = self
            .reuse_existing_language_server(&self.lsp_tree, &worktree, &language_name, cx)
            .map(|(delegate, apply)| (true, delegate, apply(&mut self.lsp_tree)))
            .unwrap_or_else(|| {
                let lsp_delegate = LocalLspAdapterDelegate::from_local_lsp(self, &worktree, cx);
                let delegate: Arc<dyn ManifestDelegate> =
                    Arc::new(ManifestQueryDelegate::new(worktree.read(cx).snapshot()));

                let servers = self
                    .lsp_tree
                    .walk(
                        ProjectPath { worktree_id, path },
                        language.name(),
                        language.manifest(),
                        &delegate,
                        cx,
                    )
                    .collect::<Vec<_>>();
                (false, lsp_delegate, servers)
            });
        let servers_and_adapters = servers
            .into_iter()
            .filter_map(|server_node| {
                if reused && server_node.server_id().is_none() {
                    return None;
                }
                if let Some(name) = server_node.name()
                    && self.stopped_language_servers.contains(&name)
                {
                    return None;
                }
                if !only_register_servers.is_empty() {
                    if let Some(server_id) = server_node.server_id()
                        && !only_register_servers.contains(&LanguageServerSelector::Id(server_id))
                    {
                        return None;
                    }
                    if let Some(name) = server_node.name()
                        && !only_register_servers.contains(&LanguageServerSelector::Name(name))
                    {
                        return None;
                    }
                }

                let server_id = server_node.server_id_or_init(|disposition| {
                    let path = &disposition.path;

                    {
                        let uri = Uri::from_file_path(worktree.read(cx).absolutize(&path.path));

                        let server_id = self.get_or_insert_language_server(
                            &worktree,
                            delegate.clone(),
                            disposition,
                            &language_name,
                            cx,
                        );

                        if let Some(state) = self.language_servers.get(&server_id)
                            && let Ok(uri) = uri
                        {
                            state.add_workspace_folder(uri);
                        };
                        server_id
                    }
                })?;
                let server_state = self.language_servers.get(&server_id)?;
                if let LanguageServerState::Running {
                    server, adapter, ..
                } = server_state
                {
                    Some((server.clone(), adapter.clone()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for (server, adapter) in servers_and_adapters {
            buffer_handle.update(cx, |buffer, cx| {
                buffer.set_completion_triggers(
                    server.server_id(),
                    server
                        .capabilities()
                        .completion_provider
                        .as_ref()
                        .and_then(|provider| {
                            provider
                                .trigger_characters
                                .as_ref()
                                .map(|characters| characters.iter().cloned().collect())
                        })
                        .unwrap_or_default(),
                    cx,
                );
            });

            let snapshot = LspBufferSnapshot {
                version: 0,
                snapshot: initial_snapshot.clone(),
            };

            let mut registered = false;
            self.buffer_snapshots
                .entry(buffer_id)
                .or_default()
                .entry(server.server_id())
                .or_insert_with(|| {
                    registered = true;
                    server.register_buffer(
                        uri.clone(),
                        adapter.language_id(&language.name()),
                        0,
                        initial_snapshot.text(),
                    );

                    vec![snapshot]
                });

            self.buffers_opened_in_servers
                .entry(buffer_id)
                .or_default()
                .insert(server.server_id());
            if registered {
                cx.emit(LspStoreEvent::LanguageServerUpdate {
                    language_server_id: server.server_id(),
                    name: None,
                    message: proto::update_language_server::Variant::RegisteredForBuffer(
                        proto::RegisteredForBuffer {
                            buffer_abs_path: abs_path.to_string_lossy().into_owned(),
                            buffer_id: buffer_id.to_proto(),
                        },
                    ),
                });
            }
        }
    }

    pub(super) fn reuse_existing_language_server<'lang_name>(
        &self,
        server_tree: &LanguageServerTree,
        worktree: &Entity<Worktree>,
        language_name: &'lang_name LanguageName,
        cx: &mut App,
    ) -> Option<(
        Arc<LocalLspAdapterDelegate>,
        impl FnOnce(&mut LanguageServerTree) -> Vec<LanguageServerTreeNode> + use<'lang_name>,
    )> {
        if worktree.read(cx).is_visible() {
            return None;
        }

        let worktree_store = self.worktree_store.read(cx);
        let servers = server_tree
            .instances
            .iter()
            .filter(|(worktree_id, _)| {
                worktree_store
                    .worktree_for_id(**worktree_id, cx)
                    .is_some_and(|worktree| worktree.read(cx).is_visible())
            })
            .flat_map(|(worktree_id, servers)| {
                servers
                    .roots
                    .values()
                    .flatten()
                    .map(move |(_, (server_node, server_languages))| {
                        (worktree_id, server_node, server_languages)
                    })
                    .filter(|(_, _, server_languages)| server_languages.contains(language_name))
                    .map(|(worktree_id, server_node, _)| {
                        (
                            *worktree_id,
                            LanguageServerTreeNode::from(Arc::downgrade(server_node)),
                        )
                    })
            })
            .fold(HashMap::default(), |mut acc, (worktree_id, server_node)| {
                acc.entry(worktree_id)
                    .or_insert_with(Vec::new)
                    .push(server_node);
                acc
            })
            .into_values()
            .max_by_key(|servers| servers.len())?;

        let worktree_id = worktree.read(cx).id();
        let apply = move |tree: &mut LanguageServerTree| {
            for server_node in &servers {
                tree.register_reused(worktree_id, language_name.clone(), server_node.clone());
            }
            servers
        };

        let delegate = LocalLspAdapterDelegate::from_local_lsp(self, worktree, cx);
        Some((delegate, apply))
    }

    pub(crate) fn unregister_old_buffer_from_language_servers(
        &mut self,
        buffer: &Entity<Buffer>,
        old_file: &File,
        cx: &mut App,
    ) {
        let old_path = match old_file.as_local() {
            Some(local) => local.abs_path(cx),
            None => return,
        };

        let Ok(file_url) = lsp::Uri::from_file_path(old_path.as_path()) else {
            return;
        };
        self.unregister_buffer_from_language_servers(buffer, &file_url, cx);
    }

    pub(crate) fn unregister_buffer_from_language_servers(
        &mut self,
        buffer: &Entity<Buffer>,
        file_url: &lsp::Uri,
        cx: &mut App,
    ) {
        buffer.update(cx, |buffer, cx| {
            let mut snapshots = self.buffer_snapshots.remove(&buffer.remote_id());

            for (_, language_server) in self.language_servers_for_buffer(buffer, cx) {
                if snapshots
                    .as_mut()
                    .is_some_and(|map| map.remove(&language_server.server_id()).is_some())
                {
                    language_server.unregister_buffer(file_url.clone());
                }
            }
        });
    }

    pub(super) fn buffer_snapshot_for_lsp_version(
        &mut self,
        buffer: &Entity<Buffer>,
        server_id: LanguageServerId,
        version: Option<i32>,
        cx: &App,
    ) -> Result<TextBufferSnapshot> {
        const OLD_VERSIONS_TO_RETAIN: i32 = 10;

        if let Some(version) = version {
            let buffer_id = buffer.read(cx).remote_id();
            let snapshots = if let Some(snapshots) = self
                .buffer_snapshots
                .get_mut(&buffer_id)
                .and_then(|m| m.get_mut(&server_id))
            {
                snapshots
            } else if version == 0 {
                // Some language servers report version 0 even if the buffer hasn't been opened yet.
                // We detect this case and treat it as if the version was `None`.
                return Ok(buffer.read(cx).text_snapshot());
            } else {
                anyhow::bail!("no snapshots found for buffer {buffer_id} and server {server_id}");
            };

            let found_snapshot = snapshots
                    .binary_search_by_key(&version, |e| e.version)
                    .map(|ix| snapshots[ix].snapshot.clone())
                    .map_err(|_| {
                        anyhow!("snapshot not found for buffer {buffer_id} server {server_id} at version {version}")
                    })?;

            snapshots.retain(|snapshot| snapshot.version + OLD_VERSIONS_TO_RETAIN >= version);
            Ok(found_snapshot)
        } else {
            Ok((buffer.read(cx)).text_snapshot())
        }
    }
}

impl LspStore {
    pub(super) async fn handle_register_buffer_with_language_servers(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::RegisterBufferWithLanguageServers>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
        let peer_id = envelope.original_sender_id.unwrap_or(envelope.sender_id);
        this.update(&mut cx, |this, cx| {
            if let Some((upstream_client, upstream_project_id)) = this.upstream_client() {
                return upstream_client.send(proto::RegisterBufferWithLanguageServers {
                    project_id: upstream_project_id,
                    buffer_id: buffer_id.to_proto(),
                    only_servers: envelope.payload.only_servers,
                });
            }

            let Some(buffer) = this.buffer_store().read(cx).get(buffer_id) else {
                anyhow::bail!("buffer is not open");
            };

            let handle = this.register_buffer_with_language_servers(
                &buffer,
                envelope
                    .payload
                    .only_servers
                    .into_iter()
                    .filter_map(|selector| {
                        Some(match selector.selector? {
                            proto::language_server_selector::Selector::ServerId(server_id) => {
                                LanguageServerSelector::Id(LanguageServerId::from_proto(server_id))
                            }
                            proto::language_server_selector::Selector::Name(name) => {
                                LanguageServerSelector::Name(LanguageServerName(
                                    SharedString::from(name),
                                ))
                            }
                        })
                    })
                    .collect(),
                false,
                cx,
            );
            // Pull diagnostics for the buffer even if it was already registered.
            // This is needed to make test_streamed_lsp_pull_diagnostics pass,
            // but it's unclear if we need it.
            this.pull_diagnostics_for_buffer(buffer.clone(), cx)
                .detach();
            this.buffer_store().update(cx, |buffer_store, _| {
                buffer_store.register_shared_lsp_handle(peer_id, buffer_id, handle);
            });

            Ok(())
        })?;
        Ok(proto::Ack {})
    }
}
