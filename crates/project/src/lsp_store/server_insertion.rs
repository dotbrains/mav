use client::proto;
use collections::{BTreeSet, HashMap};
use gpui::Context;
use language::{BinaryStatus, CachedLspAdapter, File as _, LanguageName};
use lsp::{LanguageServer, LanguageServerId, Uri};
use parking_lot::Mutex;
use std::sync::Arc;
use util::ResultExt as _;
use worktree::File;

use super::{
    LanguageServerSeed, LanguageServerStatus, LspBufferSnapshot, LspStore, LspStoreEvent,
    RenamePathsWatchedForServer, server_state::LanguageServerState,
    workspace_diagnostics::lsp_workspace_diagnostics_refresh,
};

impl LspStore {
    pub(super) fn insert_newly_running_language_server(
        &mut self,
        adapter: Arc<CachedLspAdapter>,
        language_server: Arc<LanguageServer>,
        server_id: LanguageServerId,
        key: LanguageServerSeed,
        language_name: LanguageName,
        workspace_folders: Arc<Mutex<BTreeSet<Uri>>>,
        cx: &mut Context<Self>,
    ) {
        let Some(local) = self.as_local_mut() else {
            return;
        };
        if local
            .language_server_ids
            .get(&key)
            .map(|state| state.id != server_id)
            .unwrap_or(false)
        {
            return;
        }

        let workspace_folders = workspace_folders.lock().clone();
        language_server.set_workspace_folders(workspace_folders);

        let workspace_diagnostics_refresh_tasks = language_server
            .capabilities()
            .diagnostic_provider
            .and_then(|provider| {
                local
                    .language_server_dynamic_registrations
                    .entry(server_id)
                    .or_default()
                    .diagnostics
                    .entry(None)
                    .or_insert(provider.clone());
                let workspace_refresher =
                    lsp_workspace_diagnostics_refresh(None, provider, language_server.clone(), cx)?;

                Some((None, workspace_refresher))
            })
            .into_iter()
            .collect();
        local.language_servers.insert(
            server_id,
            LanguageServerState::Running {
                workspace_diagnostics_refresh_tasks,
                adapter: adapter.clone(),
                server: language_server.clone(),
                simulate_disk_based_diagnostics_completion: None,
            },
        );
        local
            .languages
            .update_lsp_binary_status(adapter.name(), BinaryStatus::None);
        if let Some(file_ops_caps) = language_server
            .capabilities()
            .workspace
            .as_ref()
            .and_then(|ws| ws.file_operations.as_ref())
        {
            let did_rename_caps = file_ops_caps.did_rename.as_ref();
            let will_rename_caps = file_ops_caps.will_rename.as_ref();
            if did_rename_caps.or(will_rename_caps).is_some() {
                let watcher = RenamePathsWatchedForServer::default()
                    .with_did_rename_patterns(did_rename_caps)
                    .with_will_rename_patterns(will_rename_caps);
                local
                    .language_server_paths_watched_for_rename
                    .insert(server_id, watcher);
            }
        }

        self.language_server_statuses.insert(
            server_id,
            LanguageServerStatus {
                name: language_server.name(),
                language_name: Some(language_name.clone()),
                server_version: language_server.version(),
                server_readable_version: language_server.readable_version(),
                pending_work: Default::default(),
                has_pending_diagnostic_updates: false,
                progress_tokens: Default::default(),
                worktree: Some(key.worktree_id),
                binary: Some(language_server.binary().clone()),
                configuration: Some(language_server.configuration().clone()),
                workspace_folders: language_server.workspace_folders(),
                process_id: language_server.process_id(),
            },
        );

        cx.emit(LspStoreEvent::LanguageServerAdded(
            server_id,
            language_server.name(),
            Some(key.worktree_id),
        ));

        let server_capabilities = language_server.capabilities();
        if let Some((downstream_client, project_id)) = self.downstream_client.as_ref() {
            downstream_client
                .send(proto::StartLanguageServer {
                    project_id: *project_id,
                    server: Some(proto::LanguageServer {
                        id: server_id.to_proto(),
                        name: language_server.name().to_string(),
                        worktree_id: Some(key.worktree_id.to_proto()),
                        language_name: Some(language_name.to_proto()),
                    }),
                    capabilities: serde_json::to_string(&server_capabilities)
                        .expect("serializing server LSP capabilities"),
                })
                .log_err();
        }
        self.lsp_server_capabilities
            .insert(server_id, server_capabilities);

        let mut worktrees_using_server = vec![key.worktree_id];
        if let Some(local) = self.as_local() {
            for (worktree_id, servers) in &local.lsp_tree.instances {
                if *worktree_id != key.worktree_id {
                    for server_map in servers.roots.values() {
                        if server_map
                            .values()
                            .any(|(node, _)| node.id() == Some(server_id))
                        {
                            worktrees_using_server.push(*worktree_id);
                        }
                    }
                }
            }
        }

        let mut buffer_paths_registered = Vec::new();
        self.buffer_store
            .clone()
            .update(cx, |buffer_store, cx| {
                let mut lsp_adapters = HashMap::default();
                for buffer_handle in buffer_store.buffers() {
                    let buffer = buffer_handle.read(cx);
                    let file = match File::from_dyn(buffer.file()) {
                        Some(file) => file,
                        None => continue,
                    };
                    let language = match buffer.language() {
                        Some(language) => language,
                        None => continue,
                    };

                    if !worktrees_using_server.contains(&file.worktree.read(cx).id())
                        || !lsp_adapters
                            .entry(language.name())
                            .or_insert_with(|| self.languages.lsp_adapters(&language.name()))
                            .iter()
                            .any(|a| a.name == key.name)
                    {
                        continue;
                    }
                    let file = match file.as_local() {
                        Some(file) => file,
                        None => continue,
                    };

                    let local = self.as_local_mut().unwrap();

                    let buffer_id = buffer.remote_id();
                    if local.registered_buffers.contains_key(&buffer_id) {
                        let abs_path = file.abs_path(cx);
                        let uri = match lsp::Uri::from_file_path(&abs_path) {
                            Ok(uri) => uri,
                            Err(()) => {
                                log::error!("failed to convert path to URI: {:?}", abs_path);
                                continue;
                            }
                        };

                        let versions = local
                            .buffer_snapshots
                            .entry(buffer_id)
                            .or_default()
                            .entry(server_id)
                            .and_modify(|_| {
                                assert!(
                                    false,
                                    "There should not be an existing snapshot for a newly inserted buffer"
                                )
                            })
                            .or_insert_with(|| {
                                vec![LspBufferSnapshot {
                                    version: 0,
                                    snapshot: buffer.text_snapshot(),
                                }]
                            });

                        let snapshot = versions.last().unwrap();
                        let version = snapshot.version;
                        let initial_snapshot = &snapshot.snapshot;
                        language_server.register_buffer(
                            uri,
                            adapter.language_id(&language.name()),
                            version,
                            initial_snapshot.text(),
                        );
                        buffer_paths_registered.push((buffer_id, abs_path));
                        local
                            .buffers_opened_in_servers
                            .entry(buffer_id)
                            .or_default()
                            .insert(server_id);
                    }
                    buffer_handle.update(cx, |buffer, cx| {
                        buffer.set_completion_triggers(
                            server_id,
                            language_server
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
                        )
                    });
                }
            });

        for (buffer_id, abs_path) in buffer_paths_registered {
            cx.emit(LspStoreEvent::LanguageServerUpdate {
                language_server_id: server_id,
                name: Some(adapter.name()),
                message: proto::update_language_server::Variant::RegisteredForBuffer(
                    proto::RegisteredForBuffer {
                        buffer_abs_path: abs_path.to_string_lossy().into_owned(),
                        buffer_id: buffer_id.to_proto(),
                    },
                ),
            });
        }

        cx.notify();
    }
}
