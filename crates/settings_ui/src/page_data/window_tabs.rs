use super::*;

pub(super) fn status_bar_section() -> [SettingsPageItem; 10] {
    [
        SettingsPageItem::SectionHeader("Status Bar"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Active Language Button",
            description: "Show the active language button in the status bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("status_bar.active_language_button"),
                pick: |settings_content| {
                    settings_content
                        .status_bar
                        .as_ref()?
                        .active_language_button
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .status_bar
                        .get_or_insert_default()
                        .active_language_button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Active Encoding Button",
            description: "Control when to show the active encoding in the status bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("status_bar.active_encoding_button"),
                pick: |settings_content| {
                    settings_content
                        .status_bar
                        .as_ref()?
                        .active_encoding_button
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .status_bar
                        .get_or_insert_default()
                        .active_encoding_button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Cursor Position Button",
            description: "Show the cursor position button in the status bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("status_bar.cursor_position_button"),
                pick: |settings_content| {
                    settings_content
                        .status_bar
                        .as_ref()?
                        .cursor_position_button
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .status_bar
                        .get_or_insert_default()
                        .cursor_position_button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Line Endings Button",
            description: "Show the active line endings button in the status bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("status_bar.line_endings_button"),
                pick: |settings_content| {
                    settings_content
                        .status_bar
                        .as_ref()?
                        .line_endings_button
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .status_bar
                        .get_or_insert_default()
                        .line_endings_button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Terminal Button",
            description: "Show the terminal button in the status bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.button"),
                pick: |settings_content| settings_content.terminal.as_ref()?.button.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.terminal.get_or_insert_default().button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Diagnostics Button",
            description: "Show the project diagnostics button in the status bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("diagnostics.button"),
                pick: |settings_content| settings_content.diagnostics.as_ref()?.button.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.diagnostics.get_or_insert_default().button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Project Search Button",
            description: "Show the project search button in the status bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("search.button"),
                pick: |settings_content| settings_content.editor.search.as_ref()?.button.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .search
                        .get_or_insert_default()
                        .button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Debugger Button",
            description: "Show the debugger button in the status bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("debugger.button"),
                pick: |settings_content| settings_content.debugger.as_ref()?.button.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.debugger.get_or_insert_default().button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Active File Name",
            description: "Show the name of the active file in the status bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("status_bar.show_active_file"),
                pick: |settings_content| {
                    settings_content
                        .status_bar
                        .as_ref()?
                        .show_active_file
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .status_bar
                        .get_or_insert_default()
                        .show_active_file = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn tab_bar_section() -> [SettingsPageItem; 9] {
    [
        SettingsPageItem::SectionHeader("Tab Bar"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Tab Bar",
            description: "Show the tab bar in the editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("tab_bar.show"),
                pick: |settings_content| settings_content.tab_bar.as_ref()?.show.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.tab_bar.get_or_insert_default().show = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Git Status In Tabs",
            description: "Show the Git file status on a tab item.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("tabs.git_status"),
                pick: |settings_content| settings_content.tabs.as_ref()?.git_status.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.tabs.get_or_insert_default().git_status = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show File Icons In Tabs",
            description: "Show the file icon for a tab.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("tabs.file_icons"),
                pick: |settings_content| settings_content.tabs.as_ref()?.file_icons.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.tabs.get_or_insert_default().file_icons = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Tab Close Position",
            description: "Position of the close button in a tab.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("tabs.close_position"),
                pick: |settings_content| settings_content.tabs.as_ref()?.close_position.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.tabs.get_or_insert_default().close_position = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            files: USER,
            title: "Maximum Tabs",
            description: "Maximum open tabs in a pane. Will not close an unsaved tab.",
            // todo(settings_ui): The default for this value is null and it's use in code
            // is complex, so I'm going to come back to this later
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("max_tabs"),
                    pick: |settings_content| settings_content.workspace.max_tabs.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.workspace.max_tabs = value;
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Navigation History Buttons",
            description: "Show the navigation history buttons in the tab bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("tab_bar.show_nav_history_buttons"),
                pick: |settings_content| {
                    settings_content
                        .tab_bar
                        .as_ref()?
                        .show_nav_history_buttons
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .tab_bar
                        .get_or_insert_default()
                        .show_nav_history_buttons = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Tab Bar Buttons",
            description: "Show the tab bar buttons (New, Split Pane, Zoom).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("tab_bar.show_tab_bar_buttons"),
                pick: |settings_content| {
                    settings_content
                        .tab_bar
                        .as_ref()?
                        .show_tab_bar_buttons
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .tab_bar
                        .get_or_insert_default()
                        .show_tab_bar_buttons = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Pinned Tabs Layout",
            description: "Show pinned tabs in a separate row above unpinned tabs.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("tab_bar.show_pinned_tabs_in_separate_row"),
                pick: |settings_content| {
                    settings_content
                        .tab_bar
                        .as_ref()?
                        .show_pinned_tabs_in_separate_row
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .tab_bar
                        .get_or_insert_default()
                        .show_pinned_tabs_in_separate_row = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn tab_settings_section() -> [SettingsPageItem; 4] {
    [
        SettingsPageItem::SectionHeader("Tab Settings"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Activate On Close",
            description: "What to do after closing the current tab.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("tabs.activate_on_close"),
                pick: |settings_content| settings_content.tabs.as_ref()?.activate_on_close.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .tabs
                        .get_or_insert_default()
                        .activate_on_close = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Tab Show Diagnostics",
            description: "Which files containing diagnostic errors/warnings to mark in the tabs.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("tabs.show_diagnostics"),
                pick: |settings_content| settings_content.tabs.as_ref()?.show_diagnostics.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .tabs
                        .get_or_insert_default()
                        .show_diagnostics = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Close Button",
            description: "Controls the appearance behavior of the tab's close button.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("tabs.show_close_button"),
                pick: |settings_content| settings_content.tabs.as_ref()?.show_close_button.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .tabs
                        .get_or_insert_default()
                        .show_close_button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}
