use super::*;

impl LanguageServer {
    pub fn default_initialize_params(
        &self,
        pull_diagnostics: bool,
        augments_syntax_tokens: bool,
        cx: &App,
    ) -> InitializeParams {
        let workspace_folders = self.workspace_folders.as_ref().map_or_else(
            || {
                vec![WorkspaceFolder {
                    name: Default::default(),
                    uri: self.root_uri.clone(),
                }]
            },
            |folders| {
                folders
                    .lock()
                    .iter()
                    .cloned()
                    .map(|uri| WorkspaceFolder {
                        name: Default::default(),
                        uri,
                    })
                    .collect()
            },
        );

        #[allow(deprecated)]
        InitializeParams {
            process_id: Some(std::process::id()),
            root_path: Some(
                self.root_uri
                    .to_file_path()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| self.root_uri.path().to_string()),
            ),
            root_uri: Some(self.root_uri.clone()),
            initialization_options: None,
            capabilities: ClientCapabilities {
                general: Some(GeneralClientCapabilities {
                    position_encodings: Some(vec![PositionEncodingKind::UTF16]),
                    ..GeneralClientCapabilities::default()
                }),
                workspace: Some(WorkspaceClientCapabilities {
                    configuration: Some(true),
                    did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
                        dynamic_registration: Some(true),
                        relative_pattern_support: Some(true),
                    }),
                    did_change_configuration: Some(DynamicRegistrationClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
                    workspace_folders: Some(true),
                    symbol: Some(WorkspaceSymbolClientCapabilities {
                        resolve_support: None,
                        dynamic_registration: Some(true),
                        ..WorkspaceSymbolClientCapabilities::default()
                    }),
                    inlay_hint: Some(InlayHintWorkspaceClientCapabilities {
                        refresh_support: Some(true),
                    }),
                    diagnostics: Some(DiagnosticWorkspaceClientCapabilities {
                        refresh_support: Some(true),
                    })
                    .filter(|_| pull_diagnostics),
                    code_lens: Some(CodeLensWorkspaceClientCapabilities {
                        refresh_support: Some(true),
                    }),
                    workspace_edit: Some(WorkspaceEditClientCapabilities {
                        resource_operations: Some(vec![
                            ResourceOperationKind::Create,
                            ResourceOperationKind::Rename,
                            ResourceOperationKind::Delete,
                        ]),
                        document_changes: Some(true),
                        snippet_edit_support: Some(true),
                        ..WorkspaceEditClientCapabilities::default()
                    }),
                    file_operations: Some(WorkspaceFileOperationsClientCapabilities {
                        dynamic_registration: Some(true),
                        did_rename: Some(true),
                        will_rename: Some(true),
                        ..WorkspaceFileOperationsClientCapabilities::default()
                    }),
                    apply_edit: Some(true),
                    execute_command: Some(ExecuteCommandClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
                    semantic_tokens: Some(SemanticTokensWorkspaceClientCapabilities {
                        refresh_support: Some(true),
                    }),
                    ..WorkspaceClientCapabilities::default()
                }),
                text_document: Some(TextDocumentClientCapabilities {
                    definition: Some(GotoCapability {
                        link_support: Some(true),
                        dynamic_registration: Some(true),
                    }),
                    code_action: Some(CodeActionClientCapabilities {
                        code_action_literal_support: Some(CodeActionLiteralSupport {
                            code_action_kind: CodeActionKindLiteralSupport {
                                value_set: vec![
                                    CodeActionKind::REFACTOR.as_str().into(),
                                    CodeActionKind::QUICKFIX.as_str().into(),
                                    CodeActionKind::SOURCE.as_str().into(),
                                ],
                            },
                        }),
                        data_support: Some(true),
                        resolve_support: Some(CodeActionCapabilityResolveSupport {
                            properties: vec![
                                "kind".to_string(),
                                "diagnostics".to_string(),
                                "isPreferred".to_string(),
                                "disabled".to_string(),
                                "edit".to_string(),
                                "command".to_string(),
                            ],
                        }),
                        dynamic_registration: Some(true),
                        ..CodeActionClientCapabilities::default()
                    }),
                    completion: Some(CompletionClientCapabilities {
                        completion_item: Some(CompletionItemCapability {
                            snippet_support: Some(true),
                            resolve_support: Some(CompletionItemCapabilityResolveSupport {
                                properties: vec![
                                    "additionalTextEdits".to_string(),
                                    "command".to_string(),
                                    "detail".to_string(),
                                    "documentation".to_string(),
                                    // NB: Do not have this resolved, otherwise Mav becomes slow to complete things
                                    // "textEdit".to_string(),
                                ],
                            }),
                            deprecated_support: Some(true),
                            tag_support: Some(TagSupport {
                                value_set: vec![CompletionItemTag::DEPRECATED],
                            }),
                            insert_replace_support: Some(true),
                            label_details_support: Some(true),
                            insert_text_mode_support: Some(InsertTextModeSupport {
                                value_set: vec![
                                    InsertTextMode::AS_IS,
                                    InsertTextMode::ADJUST_INDENTATION,
                                ],
                            }),
                            documentation_format: Some(vec![
                                MarkupKind::Markdown,
                                MarkupKind::PlainText,
                            ]),
                            ..CompletionItemCapability::default()
                        }),
                        insert_text_mode: Some(InsertTextMode::ADJUST_INDENTATION),
                        completion_list: Some(CompletionListCapability {
                            item_defaults: Some(vec![
                                "commitCharacters".to_owned(),
                                "editRange".to_owned(),
                                "insertTextMode".to_owned(),
                                "insertTextFormat".to_owned(),
                                "data".to_owned(),
                            ]),
                        }),
                        context_support: Some(true),
                        dynamic_registration: Some(true),
                        ..CompletionClientCapabilities::default()
                    }),
                    rename: Some(RenameClientCapabilities {
                        prepare_support: Some(true),
                        prepare_support_default_behavior: Some(
                            PrepareSupportDefaultBehavior::IDENTIFIER,
                        ),
                        dynamic_registration: Some(true),
                        ..RenameClientCapabilities::default()
                    }),
                    hover: Some(HoverClientCapabilities {
                        content_format: Some(vec![MarkupKind::Markdown]),
                        dynamic_registration: Some(true),
                    }),
                    inlay_hint: Some(InlayHintClientCapabilities {
                        resolve_support: Some(InlayHintResolveClientCapabilities {
                            properties: vec![
                                "textEdits".to_string(),
                                "tooltip".to_string(),
                                "label.tooltip".to_string(),
                                "label.location".to_string(),
                                "label.command".to_string(),
                            ],
                        }),
                        dynamic_registration: Some(true),
                    }),
                    semantic_tokens: Some(SemanticTokensClientCapabilities {
                        dynamic_registration: Some(true),
                        requests: SemanticTokensClientCapabilitiesRequests {
                            range: None,
                            full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
                        },
                        token_types: SEMANTIC_TOKEN_TYPES.to_vec(),
                        token_modifiers: SEMANTIC_TOKEN_MODIFIERS.to_vec(),
                        formats: vec![TokenFormat::RELATIVE],
                        overlapping_token_support: Some(true),
                        multiline_token_support: Some(true),
                        server_cancel_support: Some(true),
                        augments_syntax_tokens: Some(augments_syntax_tokens),
                    }),
                    publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
                        related_information: Some(true),
                        version_support: Some(true),
                        data_support: Some(true),
                        tag_support: Some(TagSupport {
                            value_set: vec![DiagnosticTag::UNNECESSARY, DiagnosticTag::DEPRECATED],
                        }),
                        code_description_support: Some(true),
                    }),
                    formatting: Some(DynamicRegistrationClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
                    range_formatting: Some(DynamicRegistrationClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
                    on_type_formatting: Some(DynamicRegistrationClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
                    signature_help: Some(SignatureHelpClientCapabilities {
                        signature_information: Some(SignatureInformationSettings {
                            documentation_format: Some(vec![
                                MarkupKind::Markdown,
                                MarkupKind::PlainText,
                            ]),
                            parameter_information: Some(ParameterInformationSettings {
                                label_offset_support: Some(true),
                            }),
                            active_parameter_support: Some(true),
                        }),
                        dynamic_registration: Some(true),
                        ..SignatureHelpClientCapabilities::default()
                    }),
                    synchronization: Some(TextDocumentSyncClientCapabilities {
                        did_save: Some(true),
                        dynamic_registration: Some(true),
                        ..TextDocumentSyncClientCapabilities::default()
                    }),
                    code_lens: Some(CodeLensClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
                    document_symbol: Some(DocumentSymbolClientCapabilities {
                        hierarchical_document_symbol_support: Some(true),
                        dynamic_registration: Some(true),
                        ..DocumentSymbolClientCapabilities::default()
                    }),
                    diagnostic: Some(DiagnosticClientCapabilities {
                        dynamic_registration: Some(true),
                        related_document_support: Some(true),
                    })
                    .filter(|_| pull_diagnostics),
                    color_provider: Some(DocumentColorClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
                    document_link: Some(DocumentLinkClientCapabilities {
                        dynamic_registration: Some(true),
                        tooltip_support: Some(true),
                    }),
                    folding_range: Some(FoldingRangeClientCapabilities {
                        dynamic_registration: Some(true),
                        line_folding_only: Some(false),
                        range_limit: None,
                        folding_range: Some(FoldingRangeCapability {
                            collapsed_text: Some(true),
                        }),
                        folding_range_kind: Some(FoldingRangeKindCapability {
                            value_set: Some(vec![
                                FoldingRangeKind::Comment,
                                FoldingRangeKind::Region,
                                FoldingRangeKind::Imports,
                            ]),
                        }),
                    }),
                    ..TextDocumentClientCapabilities::default()
                }),
                experimental: Some(json!({
                    "serverStatusNotification": true,
                    "localDocs": true,
                })),
                window: Some(WindowClientCapabilities {
                    work_done_progress: Some(true),
                    show_message: Some(ShowMessageRequestClientCapabilities {
                        message_action_item: Some(MessageActionItemCapabilities {
                            additional_properties_support: Some(true),
                        }),
                    }),
                    ..WindowClientCapabilities::default()
                }),
            },
            trace: None,
            workspace_folders: Some(workspace_folders),
            client_info: release_channel::ReleaseChannel::try_global(cx).map(|release_channel| {
                ClientInfo {
                    name: release_channel.display_name().to_string(),
                    version: Some(release_channel::AppVersion::global(cx).to_string()),
                }
            }),
            locale: None,
            ..InitializeParams::default()
        }
    }

    /// Initializes a language server by sending the `Initialize` request.
    /// Note that `options` is used directly to construct [`InitializeParams`], which is why it is owned.
    ///
    /// [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#initialize)
    pub fn initialize(
        mut self,
        params: InitializeParams,
        configuration: Arc<DidChangeConfigurationParams>,
        timeout: Duration,
        cx: &App,
    ) -> Task<Result<Arc<Self>>> {
        cx.background_spawn(async move {
            let response = self
                .request::<request::Initialize>(params, timeout)
                .await
                .into_response()
                .with_context(|| {
                    format!(
                        "initializing server {}, id {}",
                        self.name(),
                        self.server_id()
                    )
                })?;
            if let Some(info) = response.server_info {
                self.version = info.version.map(SharedString::from);
                self.process_name = info.name.into();
            }
            self.capabilities = RwLock::new(response.capabilities);
            self.configuration = configuration;

            self.notify::<notification::Initialized>(InitializedParams {})?;
            Ok(Arc::new(self))
        })
    }
}
