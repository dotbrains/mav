use super::*;

pub(super) fn non_editor_language_settings_data() -> Box<[SettingsPageItem]> {
    fn lsp_section() -> [SettingsPageItem; 9] {
        [
            SettingsPageItem::SectionHeader("LSP"),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Enable Language Server",
                description: "Whether to use language servers to provide code intelligence.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).enable_language_server"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.enable_language_server.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.enable_language_server = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Language Servers",
                description: "The list of language servers to use (or disable) for this language.",
                field: Box::new(
                    SettingField {
                        organization_override: None,
                        json_path: Some("languages.$(language).language_servers"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.language_servers.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.language_servers = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Linked Edits",
                description: "Whether to perform linked edits of associated ranges, if the LS supports it. For example, when editing opening <html> tag, the contents of the closing </html> tag will be edited as well.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).linked_edits"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.linked_edits.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.linked_edits = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Go To Definition Fallback",
                description: "Whether to follow-up empty Go to definition responses from the language server.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("go_to_definition_fallback"),
                    pick: |settings_content| {
                        settings_content.editor.go_to_definition_fallback.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.go_to_definition_fallback = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Go To Definition Scroll Strategy",
                description: "How to scroll the target into view when navigating to a definition or reference.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("go_to_definition_scroll_strategy"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .go_to_definition_scroll_strategy
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.go_to_definition_scroll_strategy = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Semantic Tokens",
                description: {
                    static DESCRIPTION: OnceLock<&'static str> = OnceLock::new();
                    DESCRIPTION.get_or_init(|| {
                        SemanticTokens::VARIANTS
                            .iter()
                            .filter_map(|v| {
                                v.get_documentation().map(|doc| format!("{v:?}: {doc}"))
                            })
                            .join("\n")
                            .leak()
                    })
                },
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).semantic_tokens"),
                    pick: |settings_content| {
                        settings_content
                            .project
                            .all_languages
                            .defaults
                            .semantic_tokens
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project
                            .all_languages
                            .defaults
                            .semantic_tokens = value;
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "LSP Folding Ranges",
                description: "When enabled, use folding ranges from the language server instead of indent-based folding.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).document_folding_ranges"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.document_folding_ranges.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.document_folding_ranges = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "LSP Document Symbols",
                description: "When enabled, use the language server's document symbols for outlines and breadcrumbs instead of tree-sitter.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).document_symbols"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.document_symbols.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.document_symbols = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn lsp_completions_section() -> [SettingsPageItem; 4] {
        [
            SettingsPageItem::SectionHeader("LSP Completions"),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Enabled",
                description: "Whether to fetch LSP completions or not.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).completions.lsp"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.completions.as_ref()?.lsp.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.completions.get_or_insert_default().lsp = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Fetch Timeout (milliseconds)",
                description: "When fetching LSP completions, determines how long to wait for a response of a particular server (set to 0 to wait indefinitely).",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).completions.lsp_fetch_timeout_ms"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.completions.as_ref()?.lsp_fetch_timeout_ms.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language
                                .completions
                                .get_or_insert_default()
                                .lsp_fetch_timeout_ms = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Insert Mode",
                description: "Controls how LSP completions are inserted.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).completions.lsp_insert_mode"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.completions.as_ref()?.lsp_insert_mode.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.completions.get_or_insert_default().lsp_insert_mode = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn debugger_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader("Debuggers"),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Debuggers",
                description: "Preferred debuggers for this language.",
                field: Box::new(
                    SettingField {
                        organization_override: None,
                        json_path: Some("languages.$(language).debuggers"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.debuggers.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.debuggers = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn prettier_section() -> [SettingsPageItem; 5] {
        [
            SettingsPageItem::SectionHeader("Prettier"),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Allowed",
                description: "Enables or disables formatting with Prettier for a given language.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).prettier.allowed"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.prettier.as_ref()?.allowed.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.prettier.get_or_insert_default().allowed = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Parser",
                description: "Forces Prettier integration to use a specific parser name when formatting files with the language.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).prettier.parser"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.prettier.as_ref()?.parser.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.prettier.get_or_insert_default().parser = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Plugins",
                description: "Forces Prettier integration to use specific plugins when formatting files with the language.",
                field: Box::new(
                    SettingField {
                        organization_override: None,
                        json_path: Some("languages.$(language).prettier.plugins"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.prettier.as_ref()?.plugins.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.prettier.get_or_insert_default().plugins = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Options",
                description: "Default Prettier options, in the format as in package.json section for Prettier.",
                field: Box::new(
                    SettingField {
                        organization_override: None,
                        json_path: Some("languages.$(language).prettier.options"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.prettier.as_ref()?.options.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.prettier.get_or_insert_default().options = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    concat_sections!(
        lsp_section(),
        lsp_completions_section(),
        debugger_section(),
        prettier_section(),
    )
}
