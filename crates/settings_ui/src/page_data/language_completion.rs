use super::*;

pub(super) fn whitespace_section() -> [SettingsPageItem; 4] {
    [
        SettingsPageItem::SectionHeader("Whitespace"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Whitespaces",
            description: "Whether to show tabs and spaces in the editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).show_whitespaces"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.show_whitespaces.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.show_whitespaces = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Space Whitespace Indicator",
            description: "Visible character used to render space characters when show_whitespaces is enabled (default: \"•\")",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).whitespace_map.space"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.whitespace_map.as_ref()?.space.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.whitespace_map.get_or_insert_default().space = value;
                        })
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Tab Whitespace Indicator",
            description: "Visible character used to render tab characters when show_whitespaces is enabled (default: \"→\")",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).whitespace_map.tab"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.whitespace_map.as_ref()?.tab.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.whitespace_map.get_or_insert_default().tab = value;
                        })
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER | PROJECT,
        }),
    ]
}

pub(super) fn completions_section() -> [SettingsPageItem; 8] {
    [
        SettingsPageItem::SectionHeader("Completions"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Completions On Input",
            description: "Whether to pop the completions menu while typing in an editor without explicitly requesting it.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).show_completions_on_input"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.show_completions_on_input.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.show_completions_on_input = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Completion Documentation",
            description: "Whether to display inline and alongside documentation for items in the completions menu.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).show_completion_documentation"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.show_completion_documentation.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.show_completion_documentation = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Words",
            description: "Controls how words are completed.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).completions.words"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.completions.as_ref()?.words.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.completions.get_or_insert_default().words = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Words Min Length",
            description: "How many characters has to be in the completions query to automatically show the words-based completions.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).completions.words_min_length"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.completions.as_ref()?.words_min_length.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language
                            .completions
                            .get_or_insert_default()
                            .words_min_length = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Completion Menu Scrollbar",
            description: "When to show the scrollbar in the completion menu.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("editor.completion_menu_scrollbar"),
                pick: |settings_content| settings_content.editor.completion_menu_scrollbar.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.completion_menu_scrollbar = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Completion Detail Alignment",
            description: "Whether to align detail text in code completions context menus left or right.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("editor.completion_detail_alignment"),
                pick: |settings_content| {
                    settings_content.editor.completion_detail_alignment.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content.editor.completion_detail_alignment = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Completion Menu Item Kind",
            description: "How to display the LSP item kind (function, method, variable, etc.) of each entry in the completions menu.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("editor.completion_menu_item_kind"),
                pick: |settings_content| settings_content.editor.completion_menu_item_kind.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.completion_menu_item_kind = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}
