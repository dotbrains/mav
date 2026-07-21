use super::*;

pub(super) fn display_settings_section() -> [SettingsPageItem; 6] {
    [
        SettingsPageItem::SectionHeader("Display Settings"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Line Height",
            description: "Line height for terminal text.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("terminal.line_height"),
                    pick: |settings_content| {
                        settings_content.terminal.as_ref()?.line_height.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .line_height = value;
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Cursor Shape",
            description: "Default cursor shape for the terminal (bar, block, underline, or hollow).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.cursor_shape"),
                pick: |settings_content| settings_content.terminal.as_ref()?.cursor_shape.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .cursor_shape = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Cursor Blinking",
            description: "Sets the cursor blinking behavior in the terminal.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.blinking"),
                pick: |settings_content| settings_content.terminal.as_ref()?.blinking.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.terminal.get_or_insert_default().blinking = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Alternate Scroll",
            description: "Whether alternate scroll mode is active by default (converts mouse scroll to arrow keys in apps like Vim).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.alternate_scroll"),
                pick: |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .alternate_scroll
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .alternate_scroll = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Minimum Contrast",
            description: "The minimum APCA perceptual contrast between foreground and background colors (0-106).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.minimum_contrast"),
                pick: |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .minimum_contrast
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .minimum_contrast = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn behavior_settings_section() -> [SettingsPageItem; 5] {
    [
        SettingsPageItem::SectionHeader("Behavior Settings"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Option As Meta",
            description: "Whether the option key behaves as the meta key.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.option_as_meta"),
                pick: |settings_content| {
                    settings_content.terminal.as_ref()?.option_as_meta.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .option_as_meta = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Copy On Select",
            description: "Whether selecting text in the terminal automatically copies to the system clipboard.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.copy_on_select"),
                pick: |settings_content| {
                    settings_content.terminal.as_ref()?.copy_on_select.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .copy_on_select = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Keep Selection On Copy",
            description: "Whether to keep the text selection after copying it to the clipboard.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.keep_selection_on_copy"),
                pick: |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .keep_selection_on_copy
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .keep_selection_on_copy = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Audible Bell",
            description: "Whether to play a sound when the BEL character (`\\a`, `0x07`) is printed",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.bell"),
                pick: |settings_content| settings_content.terminal.as_ref()?.bell.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.terminal.get_or_insert_default().bell = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn layout_settings_section() -> [SettingsPageItem; 3] {
    [
        SettingsPageItem::SectionHeader("Layout Settings"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Default Width",
            description: "Default width when the terminal is docked to the left or right (in pixels).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.default_width"),
                pick: |settings_content| settings_content.terminal.as_ref()?.default_width.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .default_width = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Default Height",
            description: "Default height when the terminal is docked to the bottom (in pixels).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.default_height"),
                pick: |settings_content| {
                    settings_content.terminal.as_ref()?.default_height.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .default_height = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn advanced_settings_section() -> [SettingsPageItem; 3] {
    [
        SettingsPageItem::SectionHeader("Advanced Settings"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Max Scroll History Lines",
            description: "Maximum number of lines to keep in scrollback history (max: 100,000; 0 disables scrolling).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.max_scroll_history_lines"),
                pick: |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .max_scroll_history_lines
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .max_scroll_history_lines = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Scroll Multiplier",
            description: "The multiplier for scrolling in the terminal with the mouse wheel",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.scroll_multiplier"),
                pick: |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .scroll_multiplier
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .scroll_multiplier = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn terminal_toolbar_section() -> [SettingsPageItem; 2] {
    [
        SettingsPageItem::SectionHeader("Toolbar"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Breadcrumbs",
            description: "Display the terminal title in breadcrumbs inside the terminal pane.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.toolbar.breadcrumbs"),
                pick: |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .toolbar
                        .as_ref()?
                        .breadcrumbs
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .toolbar
                        .get_or_insert_default()
                        .breadcrumbs = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn scrollbar_section() -> [SettingsPageItem; 2] {
    [
        SettingsPageItem::SectionHeader("Scrollbar"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Scrollbar",
            description: "When to show the scrollbar in the terminal.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.scrollbar.show"),
                pick: |settings_content| {
                    show_scrollbar_or_editor(settings_content, |settings_content| {
                        settings_content
                            .terminal
                            .as_ref()?
                            .scrollbar
                            .as_ref()?
                            .show
                            .as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .scrollbar
                        .get_or_insert_default()
                        .show = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}
