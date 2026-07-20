use anyhow::Context as _;
use collections::BTreeSet;
use gpui::Context;
use lsp::{CompletionOptions, DiagnosticServerCapabilities, OneOf, TextDocumentSyncSaveOptions};

use super::{
    LspStore, registration_options::parse_register_capabilities,
    server_capabilities_update::notify_server_capabilities_updated,
    server_state::LanguageServerState, workspace_diagnostics::lsp_workspace_diagnostics_refresh,
};

impl LspStore {
    pub(crate) fn register_server_capabilities(
        &mut self,
        server_id: lsp::LanguageServerId,
        params: lsp::RegistrationParams,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let server = self
            .language_server_for_id(server_id)
            .with_context(|| format!("no server {server_id} found"))?;
        for reg in params.registrations {
            match reg.method.as_str() {
                "workspace/didChangeWatchedFiles" => {
                    if let Some(options) = reg.register_options {
                        let notify = if let Some(local_lsp_store) = self.as_local_mut() {
                            let caps = serde_json::from_value(options)?;
                            local_lsp_store
                                .on_lsp_did_change_watched_files(server_id, &reg.id, caps, cx);
                            true
                        } else {
                            false
                        };
                        if notify {
                            notify_server_capabilities_updated(&server, cx);
                        }
                    }
                }
                "workspace/didChangeConfiguration" => {
                    // Ignore payload since we notify clients of setting changes unconditionally, relying on them pulling the latest settings.
                }
                "workspace/didChangeWorkspaceFolders" => {
                    // In this case register options is an empty object, we can ignore it.
                    let caps = lsp::WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Right(reg.id)),
                    };
                    server.update_capabilities(|capabilities| {
                        capabilities
                            .workspace
                            .get_or_insert_default()
                            .workspace_folders = Some(caps);
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "workspace/symbol" => {
                    let options = parse_register_capabilities(reg)?;
                    server.update_capabilities(|capabilities| {
                        capabilities.workspace_symbol_provider = Some(options);
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "workspace/fileOperations" => {
                    if let Some(options) = reg.register_options {
                        let caps = serde_json::from_value(options)?;
                        server.update_capabilities(|capabilities| {
                            capabilities
                                .workspace
                                .get_or_insert_default()
                                .file_operations = Some(caps);
                        });
                        notify_server_capabilities_updated(&server, cx);
                    }
                }
                "workspace/executeCommand" => {
                    if let Some(options) = reg.register_options {
                        let options = serde_json::from_value(options)?;
                        server.update_capabilities(|capabilities| {
                            capabilities.execute_command_provider = Some(options);
                        });
                        notify_server_capabilities_updated(&server, cx);
                    }
                }
                "textDocument/rangeFormatting" => {
                    let options = parse_register_capabilities(reg)?;
                    server.update_capabilities(|capabilities| {
                        capabilities.document_range_formatting_provider = Some(options);
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/onTypeFormatting" => {
                    if let Some(options) = reg
                        .register_options
                        .map(serde_json::from_value)
                        .transpose()?
                    {
                        server.update_capabilities(|capabilities| {
                            capabilities.document_on_type_formatting_provider = Some(options);
                        });
                        notify_server_capabilities_updated(&server, cx);
                    }
                }
                "textDocument/formatting" => {
                    let options = parse_register_capabilities(reg)?;
                    server.update_capabilities(|capabilities| {
                        capabilities.document_formatting_provider = Some(options);
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/rename" => {
                    let options = parse_register_capabilities(reg)?;
                    server.update_capabilities(|capabilities| {
                        capabilities.rename_provider = Some(options);
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/inlayHint" => {
                    let options = parse_register_capabilities(reg)?;
                    server.update_capabilities(|capabilities| {
                        capabilities.inlay_hint_provider = Some(options);
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/documentSymbol" => {
                    let options = parse_register_capabilities(reg)?;
                    server.update_capabilities(|capabilities| {
                        capabilities.document_symbol_provider = Some(options);
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/codeAction" => {
                    let options = parse_register_capabilities(reg)?;
                    let provider = match options {
                        OneOf::Left(value) => lsp::CodeActionProviderCapability::Simple(value),
                        OneOf::Right(caps) => caps,
                    };
                    server.update_capabilities(|capabilities| {
                        capabilities.code_action_provider = Some(provider);
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/definition" => {
                    let options = parse_register_capabilities(reg)?;
                    server.update_capabilities(|capabilities| {
                        capabilities.definition_provider = Some(options);
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/completion" => {
                    if let Some(caps) = reg
                        .register_options
                        .map(serde_json::from_value::<CompletionOptions>)
                        .transpose()?
                    {
                        server.update_capabilities(|capabilities| {
                            capabilities.completion_provider = Some(caps.clone());
                        });

                        if let Some(local) = self.as_local() {
                            let mut buffers_with_language_server = Vec::new();
                            for handle in self.buffer_store.read(cx).buffers() {
                                let buffer_id = handle.read(cx).remote_id();
                                if local
                                    .buffers_opened_in_servers
                                    .get(&buffer_id)
                                    .filter(|s| s.contains(&server_id))
                                    .is_some()
                                {
                                    buffers_with_language_server.push(handle);
                                }
                            }
                            let triggers = caps
                                .trigger_characters
                                .unwrap_or_default()
                                .into_iter()
                                .collect::<BTreeSet<_>>();
                            for handle in buffers_with_language_server {
                                let triggers = triggers.clone();
                                let _ = handle.update(cx, move |buffer, cx| {
                                    buffer.set_completion_triggers(server_id, triggers, cx);
                                });
                            }
                        }
                        notify_server_capabilities_updated(&server, cx);
                    }
                }
                "textDocument/hover" => {
                    let options = parse_register_capabilities(reg)?;
                    let provider = match options {
                        OneOf::Left(value) => lsp::HoverProviderCapability::Simple(value),
                        OneOf::Right(caps) => caps,
                    };
                    server.update_capabilities(|capabilities| {
                        capabilities.hover_provider = Some(provider);
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/signatureHelp" => {
                    if let Some(caps) = reg
                        .register_options
                        .map(serde_json::from_value)
                        .transpose()?
                    {
                        server.update_capabilities(|capabilities| {
                            capabilities.signature_help_provider = Some(caps);
                        });
                        notify_server_capabilities_updated(&server, cx);
                    }
                }
                "textDocument/didChange" => {
                    if let Some(sync_kind) = reg
                        .register_options
                        .and_then(|opts| opts.get("syncKind").cloned())
                        .map(serde_json::from_value::<lsp::TextDocumentSyncKind>)
                        .transpose()?
                    {
                        server.update_capabilities(|capabilities| {
                            let mut sync_options =
                                Self::take_text_document_sync_options(capabilities);
                            sync_options.change = Some(sync_kind);
                            capabilities.text_document_sync =
                                Some(lsp::TextDocumentSyncCapability::Options(sync_options));
                        });
                        notify_server_capabilities_updated(&server, cx);
                    }
                }
                "textDocument/didSave" => {
                    if let Some(include_text) = reg
                        .register_options
                        .map(|opts| {
                            let transpose = opts
                                .get("includeText")
                                .cloned()
                                .map(serde_json::from_value::<Option<bool>>)
                                .transpose();
                            match transpose {
                                Ok(value) => Ok(value.flatten()),
                                Err(e) => Err(e),
                            }
                        })
                        .transpose()?
                    {
                        server.update_capabilities(|capabilities| {
                            let mut sync_options =
                                Self::take_text_document_sync_options(capabilities);
                            sync_options.save =
                                Some(TextDocumentSyncSaveOptions::SaveOptions(lsp::SaveOptions {
                                    include_text,
                                }));
                            capabilities.text_document_sync =
                                Some(lsp::TextDocumentSyncCapability::Options(sync_options));
                        });
                        notify_server_capabilities_updated(&server, cx);
                    }
                }
                "textDocument/codeLens" => {
                    if let Some(caps) = reg
                        .register_options
                        .map(serde_json::from_value)
                        .transpose()?
                    {
                        server.update_capabilities(|capabilities| {
                            capabilities.code_lens_provider = Some(caps);
                        });
                        notify_server_capabilities_updated(&server, cx);
                        self.refresh_code_lens(cx);
                    }
                }
                "textDocument/diagnostic" => {
                    if let Some(caps) = reg
                        .register_options
                        .map(serde_json::from_value::<DiagnosticServerCapabilities>)
                        .transpose()?
                    {
                        let local = self
                            .as_local_mut()
                            .context("Expected LSP Store to be local")?;
                        let state = local
                            .language_servers
                            .get_mut(&server_id)
                            .context("Could not obtain Language Servers state")?;
                        local
                            .language_server_dynamic_registrations
                            .entry(server_id)
                            .or_default()
                            .diagnostics
                            .insert(Some(reg.id.clone()), caps.clone());

                        let supports_workspace_diagnostics =
                            |capabilities: &DiagnosticServerCapabilities| match capabilities {
                                DiagnosticServerCapabilities::Options(diagnostic_options) => {
                                    diagnostic_options.workspace_diagnostics
                                }
                                DiagnosticServerCapabilities::RegistrationOptions(
                                    diagnostic_registration_options,
                                ) => {
                                    diagnostic_registration_options
                                        .diagnostic_options
                                        .workspace_diagnostics
                                }
                            };

                        if supports_workspace_diagnostics(&caps) {
                            if let LanguageServerState::Running {
                                workspace_diagnostics_refresh_tasks,
                                ..
                            } = state
                                && let Some(task) = lsp_workspace_diagnostics_refresh(
                                    Some(reg.id.clone()),
                                    caps.clone(),
                                    server.clone(),
                                    cx,
                                )
                            {
                                workspace_diagnostics_refresh_tasks.insert(Some(reg.id), task);
                            }
                        }

                        server.update_capabilities(|capabilities| {
                            capabilities.diagnostic_provider = Some(caps);
                        });

                        notify_server_capabilities_updated(&server, cx);

                        let _ = self.pull_document_diagnostics_for_server(server_id, None, cx);
                    }
                }
                "textDocument/documentColor" => {
                    let options = parse_register_capabilities(reg)?;
                    let provider = match options {
                        OneOf::Left(value) => lsp::ColorProviderCapability::Simple(value),
                        OneOf::Right(caps) => caps,
                    };
                    server.update_capabilities(|capabilities| {
                        capabilities.color_provider = Some(provider);
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/foldingRange" => {
                    let options = parse_register_capabilities(reg)?;
                    let provider = match options {
                        OneOf::Left(value) => lsp::FoldingRangeProviderCapability::Simple(value),
                        OneOf::Right(caps) => caps,
                    };
                    server.update_capabilities(|capabilities| {
                        capabilities.folding_range_provider = Some(provider);
                    });
                    notify_server_capabilities_updated(&server, cx);
                }
                "textDocument/documentLink" => {
                    if let Some(caps) = reg
                        .register_options
                        .map(serde_json::from_value)
                        .transpose()?
                    {
                        server.update_capabilities(|capabilities| {
                            capabilities.document_link_provider = Some(caps);
                        });
                        notify_server_capabilities_updated(&server, cx);
                    }
                }
                "textDocument/semanticTokens" => {
                    if let Some(caps) = reg
                        .register_options
                        .map(serde_json::from_value::<lsp::SemanticTokensRegistrationOptions>)
                        .transpose()?
                    {
                        server.update_capabilities(|capabilities| {
                            capabilities.semantic_tokens_provider = Some(
                                lsp::SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(caps),
                            );
                        });
                        notify_server_capabilities_updated(&server, cx);
                        // Re-query already-open buffers, which would otherwise keep
                        // tree-sitter-only highlighting until edited.
                        self.refresh_semantic_tokens(server_id, None, cx);
                    }
                }
                _ => log::warn!("unhandled capability registration: {reg:?}"),
            }
        }

        Ok(())
    }
}
