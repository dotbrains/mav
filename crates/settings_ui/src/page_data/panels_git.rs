use super::*;

pub(super) fn git_panel_section() -> [SettingsPageItem; 17] {
    [
        SettingsPageItem::SectionHeader("Git Panel"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Git Panel Button",
            description: "Show the Git panel button in the status bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.button"),
                pick: |settings_content| settings_content.git_panel.as_ref()?.button.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.git_panel.get_or_insert_default().button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Git Panel Dock",
            description: "Where to dock the Git panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.dock"),
                pick: |settings_content| settings_content.git_panel.as_ref()?.dock.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.git_panel.get_or_insert_default().dock = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Git Panel Default Width",
            description: "Default width of the Git panel in pixels.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.default_width"),
                pick: |settings_content| {
                    settings_content.git_panel.as_ref()?.default_width.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .default_width = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Git Panel Status Style",
            description: "How entry statuses are displayed.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.status_style"),
                pick: |settings_content| settings_content.git_panel.as_ref()?.status_style.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .status_style = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Fallback Branch Name",
            description: "Default branch name will be when init.defaultbranch is not set in Git.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.fallback_branch_name"),
                pick: |settings_content| {
                    settings_content
                        .git_panel
                        .as_ref()?
                        .fallback_branch_name
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .fallback_branch_name = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Sort By",
            description: "How to sort entries in the git panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.sort_by"),
                pick: |settings_content| settings_content.git_panel.as_ref()?.sort_by.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.git_panel.get_or_insert_default().sort_by = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Group By",
            description: "How to group entries in the git panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.group_by"),
                pick: |settings_content| settings_content.git_panel.as_ref()?.group_by.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.git_panel.get_or_insert_default().group_by = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Collapse Untracked Diff",
            description: "Whether to collapse untracked files in the diff panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.collapse_untracked_diff"),
                pick: |settings_content| {
                    settings_content
                        .git_panel
                        .as_ref()?
                        .collapse_untracked_diff
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .collapse_untracked_diff = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Tree View",
            description: "Enable to show entries in tree view list, disable to show in flat view list.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.tree_view"),
                pick: |settings_content| settings_content.git_panel.as_ref()?.tree_view.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.git_panel.get_or_insert_default().tree_view = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "File Icons",
            description: "Show file icons next to the Git status icon.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.file_icons"),
                pick: |settings_content| settings_content.git_panel.as_ref()?.file_icons.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .file_icons = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Folder Icons",
            description: "Whether to show folder icons or chevrons for directories in the git panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.folder_icons"),
                pick: |settings_content| settings_content.git_panel.as_ref()?.folder_icons.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .folder_icons = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Diff Stats",
            description: "Whether to show the addition/deletion change count next to each file in the Git panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.diff_stats"),
                pick: |settings_content| settings_content.git_panel.as_ref()?.diff_stats.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .diff_stats = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Primary Click Behavior",
            description: "Default action when clicking a changed file in the Git panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.entry_primary_click_action"),
                pick: |settings_content| {
                    settings_content
                        .git_panel
                        .as_ref()?
                        .entry_primary_click_action
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .entry_primary_click_action = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Count Badge",
            description: "Whether to show a badge on the git panel icon with the count of uncommitted changes.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.show_count_badge"),
                pick: |settings_content| {
                    settings_content
                        .git_panel
                        .as_ref()?
                        .show_count_badge
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .show_count_badge = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Commit Title Max Length",
            description: "Maximum length of the commit message title before a warning is shown. Set to 0 to disable.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.commit_title_max_length"),
                pick: |settings_content| {
                    settings_content
                        .git_panel
                        .as_ref()?
                        .commit_title_max_length
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .commit_title_max_length = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Scroll Bar",
            description: "How and when the scrollbar should be displayed.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("git_panel.scrollbar.show"),
                pick: |settings_content| {
                    show_scrollbar_or_editor(settings_content, |settings_content| {
                        settings_content
                            .git_panel
                            .as_ref()?
                            .scrollbar
                            .as_ref()?
                            .show
                            .as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    settings_content
                        .git_panel
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
