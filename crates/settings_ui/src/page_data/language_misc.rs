use super::*;

pub(super) fn miscellaneous_section() -> [SettingsPageItem; 7] {
    [
        SettingsPageItem::SectionHeader("Miscellaneous"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Word Diff Enabled",
            description: "Whether to enable word diff highlighting in the editor. When enabled, changed words within modified lines are highlighted to show exactly what changed.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).word_diff_enabled"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.word_diff_enabled.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.word_diff_enabled = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
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
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.debuggers = value;
                        })
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Middle Click Paste",
            description: "Enable middle-click paste on Linux.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).editor.middle_click_paste"),
                pick: |settings_content| settings_content.editor.middle_click_paste.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.middle_click_paste = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Extend Comment On Newline",
            description: "Whether to start a new line with a comment when a previous line is a comment as well.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).extend_comment_on_newline"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.extend_comment_on_newline.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.extend_comment_on_newline = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Colorize Brackets",
            description: "Whether to colorize brackets in the editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).colorize_brackets"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.colorize_brackets.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.colorize_brackets = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Vim/Emacs Modeline Support",
            description: "Number of lines to search for modelines (set to 0 to disable).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("modeline_lines"),
                pick: |settings_content| settings_content.modeline_lines.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.modeline_lines = value;
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
    ]
}

pub(super) fn global_only_miscellaneous_sub_section() -> [SettingsPageItem; 4] {
    [
        SettingsPageItem::SettingItem(SettingItem {
            title: "Image Viewer",
            description: "The unit for image file sizes.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("image_viewer.unit"),
                pick: |settings_content| {
                    settings_content
                        .image_viewer
                        .as_ref()
                        .and_then(|image_viewer| image_viewer.unit.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content.image_viewer.get_or_insert_default().unit = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::DynamicItem(DynamicItem {
            discriminant: SettingItem {
                files: USER,
                title: "Limit Markdown Preview Width",
                description: "Whether to constrain the markdown preview content to a maximum width, centering it when the pane is wider, for optimal readability.",
                field: Box::new(SettingField::<bool> {
                    organization_override: None,
                    json_path: Some("markdown_preview.limit_content_width"),
                    pick: |settings_content| {
                        settings_content
                            .markdown_preview
                            .as_ref()?
                            .limit_content_width
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .markdown_preview
                            .get_or_insert_default()
                            .limit_content_width = value;
                    },
                }),
                metadata: None,
            },
            pick_discriminant: |settings_content| {
                let enabled = settings_content
                    .markdown_preview
                    .as_ref()?
                    .limit_content_width
                    .unwrap_or(true);
                Some(if enabled { 1 } else { 0 })
            },
            fields: vec![
                vec![],
                vec![SettingItem {
                    files: USER,
                    title: "Max Width",
                    description: "Maximum content width in pixels. Content will be centered when the pane is wider than this value.",
                    field: Box::new(SettingField {
                        organization_override: None,
                        json_path: Some("markdown_preview.max_width"),
                        pick: |settings_content| {
                            settings_content
                                .markdown_preview
                                .as_ref()?
                                .max_width
                                .as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content
                                .markdown_preview
                                .get_or_insert_default()
                                .max_width = value;
                        },
                    }),
                    metadata: None,
                }],
            ],
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Auto Replace Emoji Shortcode",
            description: "Whether to automatically replace emoji shortcodes with emoji characters.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("message_editor.auto_replace_emoji_shortcode"),
                pick: |settings_content| {
                    settings_content
                        .message_editor
                        .as_ref()
                        .and_then(|message_editor| {
                            message_editor.auto_replace_emoji_shortcode.as_ref()
                        })
                },
                write: |settings_content, value, _| {
                    settings_content
                        .message_editor
                        .get_or_insert_default()
                        .auto_replace_emoji_shortcode = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Drop Size Target",
            description: "Relative size of the drop target in the editor that will open dropped file as a split pane.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("drop_target_size"),
                pick: |settings_content| settings_content.workspace.drop_target_size.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.workspace.drop_target_size = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}
