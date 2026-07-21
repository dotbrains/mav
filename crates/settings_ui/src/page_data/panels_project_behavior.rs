use super::*;

pub(super) fn project_panel_behavior_section() -> [SettingsPageItem; 14] {
    [
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Diagnostics",
            description: "Which files containing diagnostic errors/warnings to mark in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.show_diagnostics"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .show_diagnostics
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .show_diagnostics = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Diagnostic Badges",
            description: "Show error and warning count badges next to file names in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.diagnostic_badges"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .diagnostic_badges
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .diagnostic_badges = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Git Status Indicator",
            description: "Show a git status indicator next to file names in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.git_status_indicator"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .git_status_indicator
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .git_status_indicator = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Sticky Scroll",
            description: "Whether to stick parent directories at top of the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.sticky_scroll"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .sticky_scroll
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .sticky_scroll = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            files: USER,
            title: "Show Indent Guides",
            description: "Show indent guides in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.indent_guides.show"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .indent_guides
                        .as_ref()?
                        .show
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .indent_guides
                        .get_or_insert_default()
                        .show = value;
                },
            }),
            metadata: None,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Drag and Drop",
            description: "Whether to enable drag-and-drop operations in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.drag_and_drop"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .drag_and_drop
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .drag_and_drop = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Hide Root",
            description: "Whether to hide the root entry when only one folder is open in the window.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.hide_root"),
                pick: |settings_content| {
                    settings_content.project_panel.as_ref()?.hide_root.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .hide_root = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Hide Hidden",
            description: "Whether to hide the hidden entries in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.hide_hidden"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .hide_hidden
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .hide_hidden = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Sort Mode",
            description: "Sort order for entries in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.sort_mode"),
                pick: |settings_content| {
                    settings_content.project_panel.as_ref()?.sort_mode.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .sort_mode = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Sort Order",
            description: "Whether to sort file and folder names case-sensitively in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                pick: |settings_content| {
                    settings_content.project_panel.as_ref()?.sort_order.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .sort_order = value;
                },
                json_path: Some("project_panel.sort_order"),
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Auto Open Files On Create",
            description: "Whether to automatically open newly created files in the editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.auto_open.on_create"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .auto_open
                        .as_ref()?
                        .on_create
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .auto_open
                        .get_or_insert_default()
                        .on_create = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Auto Open Files On Paste",
            description: "Whether to automatically open files after pasting or duplicating them.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.auto_open.on_paste"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .auto_open
                        .as_ref()?
                        .on_paste
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .auto_open
                        .get_or_insert_default()
                        .on_paste = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Auto Open Files On Drop",
            description: "Whether to automatically open files dropped from external sources.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.auto_open.on_drop"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .auto_open
                        .as_ref()?
                        .on_drop
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .auto_open
                        .get_or_insert_default()
                        .on_drop = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Hidden Files",
            description: "Globs to match files that will be considered \"hidden\" and can be hidden from the project panel.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("worktree.hidden_files"),
                    pick: |settings_content| {
                        settings_content.project.worktree.hidden_files.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.project.worktree.hidden_files = value;
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER,
        }),
    ]
}
