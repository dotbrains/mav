use super::*;

pub(super) fn project_panel_display_section() -> [SettingsPageItem; 15] {
    [
        SettingsPageItem::SectionHeader("Project Panel"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Project Panel Dock",
            description: "Where to dock the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.dock"),
                pick: |settings_content| settings_content.project_panel.as_ref()?.dock.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.project_panel.get_or_insert_default().dock = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Project Panel Default Width",
            description: "Default width of the project panel in pixels.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.default_width"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .default_width
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .default_width = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Hide .gitignore",
            description: "Whether to hide the gitignore entries in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.hide_gitignore"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .hide_gitignore
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .hide_gitignore = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Entry Spacing",
            description: "Spacing between worktree entries in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.entry_spacing"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .entry_spacing
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .entry_spacing = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "File Icons",
            description: "Show file icons in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.file_icons"),
                pick: |settings_content| {
                    settings_content.project_panel.as_ref()?.file_icons.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .file_icons = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Folder Icons",
            description: "Whether to show folder icons or chevrons for directories in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.folder_icons"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .folder_icons
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .folder_icons = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Git Status",
            description: "Show the Git status in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.git_status"),
                pick: |settings_content| {
                    settings_content.project_panel.as_ref()?.git_status.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .git_status = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Indent Size",
            description: "Amount of indentation for nested items.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.indent_size"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .indent_size
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .indent_size = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Auto Reveal Entries",
            description: "Whether to reveal entries in the project panel automatically when a corresponding project entry becomes active.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.auto_reveal_entries"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .auto_reveal_entries
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .auto_reveal_entries = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Starts Open",
            description: "Whether the project panel should open on startup.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.starts_open"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .starts_open
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .starts_open = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Auto Fold Directories",
            description: "Whether to fold directories automatically and show compact folders when a directory has only one subdirectory inside.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.auto_fold_dirs"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .auto_fold_dirs
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .auto_fold_dirs = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Bold Folder Labels",
            description: "Whether to show folder names with bold text in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.bold_folder_labels"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .bold_folder_labels
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .bold_folder_labels = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Scrollbar",
            description: "Show the scrollbar in the project panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.scrollbar.show"),
                pick: |settings_content| {
                    show_scrollbar_or_editor(settings_content, |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .scrollbar
                            .as_ref()?
                            .show
                            .as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .scrollbar
                        .get_or_insert_default()
                        .show = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Horizontal Scroll",
            description: "Whether to allow horizontal scrolling in the project panel. When disabled, the view is always locked to the leftmost position and long file names are clipped.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("project_panel.scrollbar.horizontal_scroll"),
                pick: |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .scrollbar
                        .as_ref()?
                        .horizontal_scroll
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .scrollbar
                        .get_or_insert_default()
                        .horizontal_scroll = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}
