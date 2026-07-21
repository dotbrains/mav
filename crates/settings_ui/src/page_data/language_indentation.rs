use super::*;

pub(super) fn indentation_section() -> [SettingsPageItem; 5] {
    [
        SettingsPageItem::SectionHeader("Indentation"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Tab Size",
            description: "How many columns a tab should occupy.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).tab_size"), // TODO(cameron): not JQ syntax because not URL-safe
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| language.tab_size.as_ref())
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.tab_size = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Hard Tabs",
            description: "Whether to indent lines using tab characters, as opposed to multiple spaces.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).hard_tabs"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.hard_tabs.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.hard_tabs = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Auto Indent",
            description: "Controls automatic indentation behavior when typing.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).auto_indent"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.auto_indent.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.auto_indent = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Auto Indent On Paste",
            description: "Whether indentation of pasted content should be adjusted based on the context.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).auto_indent_on_paste"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.auto_indent_on_paste.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.auto_indent_on_paste = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
    ]
}

pub(super) fn wrapping_section() -> [SettingsPageItem; 6] {
    [
        SettingsPageItem::SectionHeader("Wrapping"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Soft Wrap",
            description: "How to soft-wrap long lines of text.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).soft_wrap"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.soft_wrap.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.soft_wrap = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Wrap Guides",
            description: "Show wrap guides in the editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).show_wrap_guides"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.show_wrap_guides.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.show_wrap_guides = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Preferred Line Length",
            description: "The column at which to soft-wrap lines, for buffers where soft-wrap is enabled.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).preferred_line_length"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.preferred_line_length.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.preferred_line_length = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Wrap Guides",
            description: "Character counts at which to show wrap guides in the editor.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).wrap_guides"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.wrap_guides.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.wrap_guides = value;
                        })
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Allow Rewrap",
            description: "Controls where the `editor::rewrap` action is allowed for this language.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).allow_rewrap"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.allow_rewrap.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.allow_rewrap = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
    ]
}

pub(super) fn indent_guides_section() -> [SettingsPageItem; 6] {
    [
        SettingsPageItem::SectionHeader("Indent Guides"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Enabled",
            description: "Display indent guides in the editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).indent_guides.enabled"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language
                            .indent_guides
                            .as_ref()
                            .and_then(|indent_guides| indent_guides.enabled.as_ref())
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.indent_guides.get_or_insert_default().enabled = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Line Width",
            description: "The width of the indent guides in pixels, between 1 and 10.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).indent_guides.line_width"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language
                            .indent_guides
                            .as_ref()
                            .and_then(|indent_guides| indent_guides.line_width.as_ref())
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.indent_guides.get_or_insert_default().line_width = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Active Line Width",
            description: "The width of the active indent guide in pixels, between 1 and 10.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).indent_guides.active_line_width"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language
                            .indent_guides
                            .as_ref()
                            .and_then(|indent_guides| indent_guides.active_line_width.as_ref())
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language
                            .indent_guides
                            .get_or_insert_default()
                            .active_line_width = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Coloring",
            description: "Determines how indent guides are colored.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).indent_guides.coloring"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language
                            .indent_guides
                            .as_ref()
                            .and_then(|indent_guides| indent_guides.coloring.as_ref())
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.indent_guides.get_or_insert_default().coloring = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Background Coloring",
            description: "Determines how indent guide backgrounds are colored.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).indent_guides.background_coloring"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language
                            .indent_guides
                            .as_ref()
                            .and_then(|indent_guides| indent_guides.background_coloring.as_ref())
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language
                            .indent_guides
                            .get_or_insert_default()
                            .background_coloring = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
    ]
}
