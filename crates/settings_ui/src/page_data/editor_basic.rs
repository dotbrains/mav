use super::*;

pub(super) fn auto_save_section() -> [SettingsPageItem; 2] {
    [
        SettingsPageItem::SectionHeader("Auto Save"),
        SettingsPageItem::DynamicItem(DynamicItem {
            discriminant: SettingItem {
                files: USER,
                title: "Auto Save Mode",
                description: "When to auto save buffer changes.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("autosave$"),
                    pick: |settings_content| {
                        Some(
                            &dynamic_variants::<settings::AutosaveSetting>()[settings_content
                                .workspace
                                .autosave
                                .as_ref()?
                                .discriminant()
                                as usize],
                        )
                    },
                    write: |settings_content, value, _| {
                        let Some(value) = value else {
                            settings_content.workspace.autosave = None;
                            return;
                        };
                        let settings_value = settings_content
                            .workspace
                            .autosave
                            .get_or_insert_with(|| settings::AutosaveSetting::Off);
                        *settings_value = match value {
                            settings::AutosaveSettingDiscriminants::Off => {
                                settings::AutosaveSetting::Off
                            }
                            settings::AutosaveSettingDiscriminants::AfterDelay => {
                                let milliseconds = match settings_value {
                                    settings::AutosaveSetting::AfterDelay { milliseconds } => {
                                        *milliseconds
                                    }
                                    _ => settings::DelayMs(1000),
                                };
                                settings::AutosaveSetting::AfterDelay { milliseconds }
                            }
                            settings::AutosaveSettingDiscriminants::OnFocusChange => {
                                settings::AutosaveSetting::OnFocusChange
                            }
                            settings::AutosaveSettingDiscriminants::OnWindowChange => {
                                settings::AutosaveSetting::OnWindowChange
                            }
                        };
                    },
                }),
                metadata: None,
            },
            pick_discriminant: |settings_content| {
                Some(settings_content.workspace.autosave.as_ref()?.discriminant() as usize)
            },
            fields: dynamic_variants::<settings::AutosaveSetting>()
                .into_iter()
                .map(|variant| match variant {
                    settings::AutosaveSettingDiscriminants::Off => vec![],
                    settings::AutosaveSettingDiscriminants::AfterDelay => vec![SettingItem {
                        files: USER,
                        title: "Delay (milliseconds)",
                        description: "Save after inactivity period (in milliseconds).",
                        field: Box::new(SettingField {
                            organization_override: None,
                            json_path: Some("autosave.after_delay.milliseconds"),
                            pick: |settings_content| match settings_content
                                .workspace
                                .autosave
                                .as_ref()
                            {
                                Some(settings::AutosaveSetting::AfterDelay { milliseconds }) => {
                                    Some(milliseconds)
                                }
                                _ => None,
                            },
                            write: |settings_content, value, _| {
                                let Some(value) = value else {
                                    settings_content.workspace.autosave = None;
                                    return;
                                };
                                match settings_content.workspace.autosave.as_mut() {
                                    Some(settings::AutosaveSetting::AfterDelay {
                                        milliseconds,
                                    }) => *milliseconds = value,
                                    _ => return,
                                }
                            },
                        }),
                        metadata: None,
                    }],
                    settings::AutosaveSettingDiscriminants::OnFocusChange => vec![],
                    settings::AutosaveSettingDiscriminants::OnWindowChange => vec![],
                })
                .collect(),
        }),
    ]
}

pub(super) fn which_key_section() -> [SettingsPageItem; 3] {
    [
        SettingsPageItem::SectionHeader("Which-key Menu"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Which-key Menu",
            description: "Display the which-key menu with matching bindings while a multi-stroke binding is pending.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("which_key.enabled"),
                pick: |settings_content| {
                    settings_content
                        .which_key
                        .as_ref()
                        .and_then(|settings| settings.enabled.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content.which_key.get_or_insert_default().enabled = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Menu Delay",
            description: "Delay in milliseconds before the which-key menu appears.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("which_key.delay_ms"),
                pick: |settings_content| {
                    settings_content
                        .which_key
                        .as_ref()
                        .and_then(|settings| settings.delay_ms.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content.which_key.get_or_insert_default().delay_ms = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn multibuffer_section() -> [SettingsPageItem; 7] {
    [
        SettingsPageItem::SectionHeader("Multibuffer"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Double Click In Multibuffer",
            description: "What to do when multibuffer is double-clicked in some of its excerpts.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("double_click_in_multibuffer"),
                pick: |settings_content| {
                    settings_content.editor.double_click_in_multibuffer.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content.editor.double_click_in_multibuffer = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Expand Excerpt Lines",
            description: "How many lines to expand the multibuffer excerpts by default.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("expand_excerpt_lines"),
                pick: |settings_content| settings_content.editor.expand_excerpt_lines.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.expand_excerpt_lines = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Excerpt Context Lines",
            description: "How many lines of context to provide in multibuffer excerpts by default.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("excerpt_context_lines"),
                pick: |settings_content| settings_content.editor.excerpt_context_lines.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.excerpt_context_lines = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Expand Outlines With Depth",
            description: "Default depth to expand outline items in the current file.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("outline_panel.expand_outlines_with_depth"),
                pick: |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()
                        .and_then(|outline_panel| outline_panel.expand_outlines_with_depth.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .expand_outlines_with_depth = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Diff View Style",
            description: "How to display diffs in the editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("diff_view_style"),
                pick: |settings_content| settings_content.editor.diff_view_style.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.diff_view_style = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Minimum Split Diff Width",
            description: "The minimum width (in columns) at which the split diff view is used. When the editor is narrower, the diff view automatically switches to unified mode. Set to 0 to disable.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("minimum_split_diff_width"),
                pick: |settings_content| settings_content.editor.minimum_split_diff_width.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.minimum_split_diff_width = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn scrolling_section() -> [SettingsPageItem; 9] {
    [
        SettingsPageItem::SectionHeader("Scrolling"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Scroll Beyond Last Line",
            description: "Whether the editor will scroll beyond the last line.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("scroll_beyond_last_line"),
                pick: |settings_content| settings_content.editor.scroll_beyond_last_line.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.scroll_beyond_last_line = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Vertical Scroll Margin",
            description: "The number of lines to keep above/below the cursor when auto-scrolling.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("vertical_scroll_margin"),
                pick: |settings_content| settings_content.editor.vertical_scroll_margin.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.vertical_scroll_margin = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Horizontal Scroll Margin",
            description: "The number of characters to keep on either side when scrolling with the mouse.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("horizontal_scroll_margin"),
                pick: |settings_content| settings_content.editor.horizontal_scroll_margin.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.horizontal_scroll_margin = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Scroll Sensitivity",
            description: "Scroll sensitivity multiplier for both horizontal and vertical scrolling.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("scroll_sensitivity"),
                pick: |settings_content| settings_content.editor.scroll_sensitivity.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.scroll_sensitivity = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Mouse Wheel Zoom",
            description: "Whether to zoom the editor font size with the mouse wheel while holding the primary modifier key.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("mouse_wheel_zoom"),
                pick: |settings_content| settings_content.editor.mouse_wheel_zoom.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.mouse_wheel_zoom = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Fast Scroll Sensitivity",
            description: "Fast scroll sensitivity multiplier for both horizontal and vertical scrolling.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("fast_scroll_sensitivity"),
                pick: |settings_content| settings_content.editor.fast_scroll_sensitivity.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.fast_scroll_sensitivity = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Autoscroll On Clicks",
            description: "Whether to scroll when clicking near the edge of the visible text area.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("autoscroll_on_clicks"),
                pick: |settings_content| settings_content.editor.autoscroll_on_clicks.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.autoscroll_on_clicks = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Sticky Scroll",
            description: "Whether to stick scopes to the top of the editor",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("sticky_scroll.enabled"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .sticky_scroll
                        .as_ref()
                        .and_then(|sticky_scroll| sticky_scroll.enabled.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .sticky_scroll
                        .get_or_insert_default()
                        .enabled = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}
