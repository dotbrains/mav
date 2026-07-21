use super::*;

pub(super) fn buffer_font_section() -> [SettingsPageItem; 7] {
    [
        SettingsPageItem::SectionHeader("Buffer Font"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Font Family",
            description: "Font family for editor text.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("buffer_font_family"),
                pick: |settings_content| settings_content.theme.buffer_font_family.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.theme.buffer_font_family = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Font Size",
            description: "Font size for editor text.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("buffer_font_size"),
                pick: |settings_content| settings_content.theme.buffer_font_size.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.theme.buffer_font_size = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Font Weight",
            description: "Font weight for editor text (100-900).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("buffer_font_weight"),
                pick: |settings_content| settings_content.theme.buffer_font_weight.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.theme.buffer_font_weight = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::DynamicItem(DynamicItem {
            discriminant: SettingItem {
                files: USER,
                title: "Line Height",
                description: "Line height for editor text.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("buffer_line_height$"),
                    pick: |settings_content| {
                        Some(
                            &dynamic_variants::<settings::BufferLineHeight>()[settings_content
                                .theme
                                .buffer_line_height
                                .as_ref()?
                                .discriminant()
                                as usize],
                        )
                    },
                    write: |settings_content, value, _| {
                        let Some(value) = value else {
                            settings_content.theme.buffer_line_height = None;
                            return;
                        };
                        let settings_value = settings_content
                            .theme
                            .buffer_line_height
                            .get_or_insert_with(|| settings::BufferLineHeight::default());
                        *settings_value = match value {
                            settings::BufferLineHeightDiscriminants::Comfortable => {
                                settings::BufferLineHeight::Comfortable
                            }
                            settings::BufferLineHeightDiscriminants::Standard => {
                                settings::BufferLineHeight::Standard
                            }
                            settings::BufferLineHeightDiscriminants::Custom => {
                                let custom_value =
                                    theme_settings::BufferLineHeight::from(*settings_value).value();
                                settings::BufferLineHeight::Custom(custom_value)
                            }
                        };
                    },
                }),
                metadata: None,
            },
            pick_discriminant: |settings_content| {
                Some(
                    settings_content
                        .theme
                        .buffer_line_height
                        .as_ref()?
                        .discriminant() as usize,
                )
            },
            fields: dynamic_variants::<settings::BufferLineHeight>()
                .into_iter()
                .map(|variant| match variant {
                    settings::BufferLineHeightDiscriminants::Comfortable => vec![],
                    settings::BufferLineHeightDiscriminants::Standard => vec![],
                    settings::BufferLineHeightDiscriminants::Custom => vec![SettingItem {
                        files: USER,
                        title: "Custom Line Height",
                        description: "Custom line height value (must be at least 1.0).",
                        field: Box::new(SettingField {
                            organization_override: None,
                            json_path: Some("buffer_line_height"),
                            pick: |settings_content| match settings_content
                                .theme
                                .buffer_line_height
                                .as_ref()
                            {
                                Some(settings::BufferLineHeight::Custom(value)) => Some(value),
                                _ => None,
                            },
                            write: |settings_content, value, _| {
                                let Some(value) = value else {
                                    return;
                                };
                                match settings_content.theme.buffer_line_height.as_mut() {
                                    Some(settings::BufferLineHeight::Custom(line_height)) => {
                                        *line_height = f32::max(value, 1.0)
                                    }
                                    _ => return,
                                }
                            },
                        }),
                        metadata: None,
                    }],
                })
                .collect(),
        }),
        SettingsPageItem::SettingItem(SettingItem {
            files: USER,
            title: "Font Features",
            description: "The OpenType features to enable for rendering in text buffers.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("buffer_font_features"),
                    pick: |settings_content| settings_content.theme.buffer_font_features.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.theme.buffer_font_features = value;
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            files: USER,
            title: "Font Fallbacks",
            description: "The font fallbacks to use for rendering in text buffers.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("buffer_font_fallbacks"),
                    pick: |settings_content| settings_content.theme.buffer_font_fallbacks.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.theme.buffer_font_fallbacks = value;
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
        }),
    ]
}

pub(super) fn ui_font_section() -> [SettingsPageItem; 6] {
    [
        SettingsPageItem::SectionHeader("UI Font"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Font Family",
            description: "Font family for UI elements.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("ui_font_family"),
                pick: |settings_content| settings_content.theme.ui_font_family.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.theme.ui_font_family = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Font Size",
            description: "Font size for UI elements.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("ui_font_size"),
                pick: |settings_content| settings_content.theme.ui_font_size.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.theme.ui_font_size = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Font Weight",
            description: "Font weight for UI elements (100-900).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("ui_font_weight"),
                pick: |settings_content| settings_content.theme.ui_font_weight.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.theme.ui_font_weight = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            files: USER,
            title: "Font Features",
            description: "The OpenType features to enable for rendering in UI elements.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("ui_font_features"),
                    pick: |settings_content| settings_content.theme.ui_font_features.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.theme.ui_font_features = value;
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            files: USER,
            title: "Font Fallbacks",
            description: "The font fallbacks to use for rendering in the UI.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("ui_font_fallbacks"),
                    pick: |settings_content| settings_content.theme.ui_font_fallbacks.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.theme.ui_font_fallbacks = value;
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
        }),
    ]
}

pub(super) fn agent_panel_font_section() -> [SettingsPageItem; 3] {
    [
        SettingsPageItem::SectionHeader("Agent Panel Font"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "UI Font Size",
            description: "Font size for agent response text in the agent panel. Falls back to the regular UI font size.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent_ui_font_size"),
                pick: |settings_content| {
                    settings_content
                        .theme
                        .agent_ui_font_size
                        .as_ref()
                        .or(settings_content.theme.ui_font_size.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content.theme.agent_ui_font_size = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Buffer Font Size",
            description: "Font size for user messages text in the agent panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent_buffer_font_size"),
                pick: |settings_content| {
                    settings_content
                        .theme
                        .agent_buffer_font_size
                        .as_ref()
                        .or(settings_content.theme.buffer_font_size.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content.theme.agent_buffer_font_size = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn markdown_preview_font_section() -> [SettingsPageItem; 4] {
    [
        SettingsPageItem::SectionHeader("Markdown Preview Font"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Font Family",
            description: "Font family for the markdown preview. Falls back to the UI font family.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("markdown_preview_font_family"),
                pick: |settings_content| {
                    settings_content.theme.markdown_preview_font_family.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content.theme.markdown_preview_font_family = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Code Font Family",
            description: "Font family for code blocks in the markdown preview. Falls back to the editor font family.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("markdown_preview_code_font_family"),
                pick: |settings_content| {
                    settings_content
                        .theme
                        .markdown_preview_code_font_family
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content.theme.markdown_preview_code_font_family = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Font Size",
            description: "Font size for the markdown preview. Falls back to the editor font size.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("markdown_preview_font_size"),
                pick: |settings_content| {
                    settings_content
                        .theme
                        .markdown_preview_font_size
                        .as_ref()
                        .or(settings_content.theme.buffer_font_size.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content.theme.markdown_preview_font_size = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}
