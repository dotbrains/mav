use super::*;

pub(super) fn formatting_section() -> [SettingsPageItem; 8] {
    [
        SettingsPageItem::SectionHeader("Formatting"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Format On Save",
            description: "Whether or not to perform a buffer format before saving.",
            field: Box::new(
                // TODO(settings_ui): this setting should just be a bool
                SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).format_on_save"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.format_on_save.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.format_on_save = value;
                        })
                    },
                },
            ),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Remove Trailing Whitespace On Save",
            description: "Whether or not to remove any trailing whitespace from lines of a buffer before saving it.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).remove_trailing_whitespace_on_save"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.remove_trailing_whitespace_on_save.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.remove_trailing_whitespace_on_save = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Ensure Final Newline On Save",
            description: "Whether or not to ensure there's a single newline at the end of a buffer when saving it.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).ensure_final_newline_on_save"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.ensure_final_newline_on_save.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.ensure_final_newline_on_save = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Line Ending",
            description: "How line endings should be handled for new files and during format and save operations.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).line_ending"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.line_ending.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.line_ending = value;
                    })
                },
            }),
            metadata: Some(Box::new(SettingsFieldMetadata {
                should_do_titlecase: Some(false),
                ..Default::default()
            })),
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Formatter",
            description: "How to perform a buffer format.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).formatter"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.formatter.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.formatter = value;
                        })
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Use On Type Format",
            description: "Whether to use additional LSP queries to format (and amend) the code after every \"trigger\" symbol input, defined by LSP server capabilities",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).use_on_type_format"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.use_on_type_format.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.use_on_type_format = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Code Actions On Format",
            description: "Additional code actions to run when formatting.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).code_actions_on_format"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.code_actions_on_format.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.code_actions_on_format = value;
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

pub(super) fn autoclose_section() -> [SettingsPageItem; 5] {
    [
        SettingsPageItem::SectionHeader("Autoclose"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Use Autoclose",
            description: "Whether to automatically type closing characters for you. For example, when you type '(', Mav will automatically add a closing ')' at the correct position.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).use_autoclose"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.use_autoclose.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.use_autoclose = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Use Auto Surround",
            description: "Whether to automatically surround text with characters for you. For example, when you select text and type '(', Mav will automatically surround text with ().",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).use_auto_surround"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.use_auto_surround.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.use_auto_surround = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Always Treat Brackets As Autoclosed",
            description: "Controls whether the closing characters are always skipped over and auto-removed no matter how they were inserted.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).always_treat_brackets_as_autoclosed"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.always_treat_brackets_as_autoclosed.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.always_treat_brackets_as_autoclosed = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "JSX Tag Auto Close",
            description: "Whether to automatically close JSX tags.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).jsx_tag_auto_close"),
                // TODO(settings_ui): this setting should just be a bool
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.jsx_tag_auto_close.as_ref()?.enabled.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.jsx_tag_auto_close.get_or_insert_default().enabled = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
    ]
}
