use anyhow::Context as _;
use collections::HashSet;
use gpui::{Context, SharedString};
use language::{DiagnosticSourceKind, LocalFile as _};
use std::{borrow::Cow, path::PathBuf};
use worktree::File;

use super::{
    DocumentDiagnostics, DocumentDiagnosticsUpdate, LspStore,
    server_capabilities_update::notify_server_capabilities_updated,
    server_state::LanguageServerState,
};

impl LspStore {
    pub(crate) fn unregister_server_capabilities(
        &mut self,
        server_id: lsp::LanguageServerId,
        params: lsp::UnregistrationParams,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let server = self
            .language_server_for_id(server_id)
            .with_context(|| format!("no server {server_id} found"))?;
        for unreg in params.unregisterations.iter() {
            match unreg.method.as_str() {
                "workspace/didChangeWatchedFiles" => {
                    let notify = if let Some(local_lsp_store) = self.as_local_mut() {
                        local_lsp_store
                            .on_lsp_unregister_did_change_watched_files(server_id, &unreg.id, cx);
                        true
                    } else {
                        false
                    };
                    if notify {
                        notify_server_capabilities_updated(&server, cx);
                    }
                }
                "workspace/didChangeConfiguration" => {
                    // Ignore payload since we notify clients of setting changes unconditionally, relying on them pulling the latest settings.
                }
                "workspace/didChangeWorkspaceFolders" => {
                    server.update_capabilities(|capabilities| {
                        capabilities
                            .workspace
                            .get_or_insert_with(|| lsp::WorkspaceServerCapabilities {
                                workspace_folders: None,
                                file_operations: None,
                            })
                            .workspace_folders = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "workspace/symbol" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.workspace_symbol_provider = None
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "workspace/fileOperations" => {
                    server.update_capabilities(|capabilities| {
                        capabilities
                            .workspace
                            .get_or_insert_with(|| lsp::WorkspaceServerCapabilities {
                                workspace_folders: None,
                                file_operations: None,
                            })
                            .file_operations = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "workspace/executeCommand" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.execute_command_provider = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/rangeFormatting" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.document_range_formatting_provider = None
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/onTypeFormatting" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.document_on_type_formatting_provider = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/formatting" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.document_formatting_provider = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/rename" => {
                    server.update_capabilities(|capabilities| capabilities.rename_provider = None);
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/codeAction" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.code_action_provider = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/definition" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.definition_provider = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/completion" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.completion_provider = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/hover" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.hover_provider = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/signatureHelp" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.signature_help_provider = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/semanticTokens" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.semantic_tokens_provider = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/didChange" => {
                    server.update_capabilities(|capabilities| {
                        let mut sync_options = Self::take_text_document_sync_options(capabilities);
                        sync_options.change = None;
                        capabilities.text_document_sync =
                            Some(lsp::TextDocumentSyncCapability::Options(sync_options));
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/didSave" => {
                    server.update_capabilities(|capabilities| {
                        let mut sync_options = Self::take_text_document_sync_options(capabilities);
                        sync_options.save = None;
                        capabilities.text_document_sync =
                            Some(lsp::TextDocumentSyncCapability::Options(sync_options));
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/codeLens" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.code_lens_provider = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/diagnostic" => {
                    let local = self
                        .as_local_mut()
                        .context("Expected LSP Store to be local")?;

                    let state = local
                        .language_servers
                        .get_mut(&server_id)
                        .context("Could not obtain Language Servers state")?;
                    let registrations = local
                        .language_server_dynamic_registrations
                        .get_mut(&server_id)
                        .with_context(|| {
                            format!("Expected dynamic registration to exist for server {server_id}")
                        })?;
                    registrations
                        .diagnostics
                        .remove(&Some(unreg.id.clone()))
                        .with_context(|| {
                            format!(
                                "Attempted to unregister non-existent diagnostic registration with ID {}",
                                unreg.id
                            )
                        })?;
                    let removed_last_diagnostic_provider = registrations.diagnostics.is_empty();

                    if let LanguageServerState::Running {
                        workspace_diagnostics_refresh_tasks,
                        ..
                    } = state
                    {
                        workspace_diagnostics_refresh_tasks.remove(&Some(unreg.id.clone()));
                    }

                    self.clear_unregistered_diagnostics(
                        server_id,
                        SharedString::from(unreg.id.clone()),
                        cx,
                    )?;

                    if removed_last_diagnostic_provider {
                        server.update_capabilities(|capabilities| {
                            debug_assert!(capabilities.diagnostic_provider.is_some());
                            capabilities.diagnostic_provider = None;
                        });
                    }

                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/documentColor" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.color_provider = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/foldingRange" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.folding_range_provider = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/documentLink" => {
                    server.update_capabilities(|capabilities| {
                        capabilities.document_link_provider = None;
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                _ => log::warn!("unhandled capability unregistration: {unreg:?}"),
            }
        }

        Ok(())
    }

    fn clear_unregistered_diagnostics(
        &mut self,
        server_id: lsp::LanguageServerId,
        cleared_registration_id: SharedString,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let mut affected_abs_paths: HashSet<PathBuf> = HashSet::default();

        self.buffer_store.update(cx, |buffer_store, cx| {
            for buffer_handle in buffer_store.buffers() {
                let buffer = buffer_handle.read(cx);
                let abs_path = File::from_dyn(buffer.file()).map(|f| f.abs_path(cx));
                let Some(abs_path) = abs_path else {
                    continue;
                };
                affected_abs_paths.insert(abs_path);
            }
        });

        let local = self.as_local().context("Expected LSP Store to be local")?;
        for (worktree_id, diagnostics_for_tree) in local.diagnostics.iter() {
            let Some(worktree) = self
                .worktree_store
                .read(cx)
                .worktree_for_id(*worktree_id, cx)
            else {
                continue;
            };

            for (rel_path, diagnostics_by_server_id) in diagnostics_for_tree.iter() {
                if let Ok(ix) = diagnostics_by_server_id.binary_search_by_key(&server_id, |e| e.0) {
                    let has_matching_registration =
                        diagnostics_by_server_id[ix].1.iter().any(|entry| {
                            entry.diagnostic.registration_id.as_ref()
                                == Some(&cleared_registration_id)
                        });
                    if has_matching_registration {
                        let abs_path = worktree.read(cx).absolutize(rel_path);
                        affected_abs_paths.insert(abs_path);
                    }
                }
            }
        }

        if affected_abs_paths.is_empty() {
            return Ok(());
        }

        // Send a fake diagnostic update which clears the state for the registration ID.
        let clears: Vec<DocumentDiagnosticsUpdate<'static, DocumentDiagnostics>> =
            affected_abs_paths
                .into_iter()
                .map(|abs_path| DocumentDiagnosticsUpdate {
                    diagnostics: DocumentDiagnostics {
                        diagnostics: Vec::new(),
                        document_abs_path: abs_path,
                        version: None,
                    },
                    result_id: None,
                    registration_id: Some(cleared_registration_id.clone()),
                    server_id,
                    disk_based_sources: Cow::Borrowed(&[]),
                })
                .collect();

        let merge_registration_id = cleared_registration_id.clone();
        self.merge_diagnostic_entries(
            clears,
            move |_, diagnostic, _| {
                if diagnostic.source_kind == DiagnosticSourceKind::Pulled {
                    diagnostic.registration_id != Some(merge_registration_id.clone())
                } else {
                    true
                }
            },
            cx,
        )?;

        Ok(())
    }
}
