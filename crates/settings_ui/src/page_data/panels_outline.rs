use super::*;

pub(super) fn outline_panel_section() -> [SettingsPageItem; 11] {
    [
        SettingsPageItem::SectionHeader("Outline Panel"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Outline Panel Button",
            description: "Show the outline panel button in the status bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("outline_panel.button"),
                pick: |settings_content| settings_content.outline_panel.as_ref()?.button.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Outline Panel Dock",
            description: "Where to dock the outline panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("outline_panel.dock"),
                pick: |settings_content| settings_content.outline_panel.as_ref()?.dock.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.outline_panel.get_or_insert_default().dock = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Outline Panel Default Width",
            description: "Default width of the outline panel in pixels.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("outline_panel.default_width"),
                pick: |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()?
                        .default_width
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .default_width = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "File Icons",
            description: "Show file icons in the outline panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("outline_panel.file_icons"),
                pick: |settings_content| {
                    settings_content.outline_panel.as_ref()?.file_icons.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .file_icons = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Folder Icons",
            description: "Whether to show folder icons or chevrons for directories in the outline panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("outline_panel.folder_icons"),
                pick: |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()?
                        .folder_icons
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .folder_icons = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Git Status",
            description: "Show the Git status in the outline panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("outline_panel.git_status"),
                pick: |settings_content| {
                    settings_content.outline_panel.as_ref()?.git_status.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .outline_panel
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
                json_path: Some("outline_panel.indent_size"),
                pick: |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()?
                        .indent_size
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .indent_size = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Auto Reveal Entries",
            description: "Whether to reveal when a corresponding outline entry becomes active.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("outline_panel.auto_reveal_entries"),
                pick: |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()?
                        .auto_reveal_entries
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .auto_reveal_entries = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Auto Fold Directories",
            description: "Whether to fold directories automatically when a directory contains only one subdirectory.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("outline_panel.auto_fold_dirs"),
                pick: |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()?
                        .auto_fold_dirs
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .auto_fold_dirs = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            files: USER,
            title: "Show Indent Guides",
            description: "When to show indent guides in the outline panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("outline_panel.indent_guides.show"),
                pick: |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()?
                        .indent_guides
                        .as_ref()?
                        .show
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .indent_guides
                        .get_or_insert_default()
                        .show = value;
                },
            }),
            metadata: None,
        }),
    ]
}

pub(super) fn collaboration_panel_section() -> [SettingsPageItem; 4] {
    [
        SettingsPageItem::SectionHeader("Collaboration Panel"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Collaboration Panel Button",
            description: "Show the collaboration panel button in the status bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("collaboration_panel.button"),
                pick: |settings_content| {
                    settings_content
                        .collaboration_panel
                        .as_ref()?
                        .button
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .collaboration_panel
                        .get_or_insert_default()
                        .button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Collaboration Panel Dock",
            description: "Where to dock the collaboration panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("collaboration_panel.dock"),
                pick: |settings_content| {
                    settings_content.collaboration_panel.as_ref()?.dock.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .collaboration_panel
                        .get_or_insert_default()
                        .dock = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Collaboration Panel Default Width",
            description: "Default width of the collaboration panel in pixels.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("collaboration_panel.dock"),
                pick: |settings_content| {
                    settings_content
                        .collaboration_panel
                        .as_ref()?
                        .default_width
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .collaboration_panel
                        .get_or_insert_default()
                        .default_width = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn agent_panel_section() -> [SettingsPageItem; 7] {
    [
        SettingsPageItem::SectionHeader("Agent Panel"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Agent Panel Button",
            description: "Whether to show the agent panel button in the status bar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.button"),
                pick: |settings_content| settings_content.agent.as_ref()?.button.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.agent.get_or_insert_default().button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Agent Panel Dock",
            description: "Where to dock the agent panel.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.dock"),
                pick: |settings_content| settings_content.agent.as_ref()?.dock.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.agent.get_or_insert_default().dock = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Agent Panel Flexible Sizing",
            description: "Whether the agent panel should use flexible (proportional) sizing when docked to the left or right.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.flexible"),
                pick: |settings_content| settings_content.agent.as_ref()?.flexible.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.agent.get_or_insert_default().flexible = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Agent Panel Default Width",
            description: "Default width when the agent panel is docked to the left or right.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.default_width"),
                pick: |settings_content| settings_content.agent.as_ref()?.default_width.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.agent.get_or_insert_default().default_width = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Agent Panel Default Height",
            description: "Default height when the agent panel is docked to the bottom.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.default_height"),
                pick: |settings_content| settings_content.agent.as_ref()?.default_height.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .default_height = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::DynamicItem(DynamicItem {
            discriminant: SettingItem {
                files: USER,
                title: "Limit Content Width",
                description: "Whether to constrain the agent panel content to a maximum width, centering it when the panel is wider, for optimal readability.",
                field: Box::new(SettingField::<bool> {
                    organization_override: None,
                    json_path: Some("agent.limit_content_width"),
                    pick: |settings_content| {
                        settings_content
                            .agent
                            .as_ref()?
                            .limit_content_width
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .agent
                            .get_or_insert_default()
                            .limit_content_width = value;
                    },
                }),
                metadata: None,
            },
            pick_discriminant: |settings_content| {
                let enabled = settings_content
                    .agent
                    .as_ref()?
                    .limit_content_width
                    .unwrap_or(true);
                Some(if enabled { 1 } else { 0 })
            },
            fields: vec![
                vec![],
                vec![SettingItem {
                    files: USER,
                    title: "Max Content Width",
                    description: "Maximum content width in pixels. Content will be centered when the panel is wider than this value.",
                    field: Box::new(SettingField {
                        organization_override: None,
                        json_path: Some("agent.max_content_width"),
                        pick: |settings_content| {
                            settings_content.agent.as_ref()?.max_content_width.as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content
                                .agent
                                .get_or_insert_default()
                                .max_content_width = value;
                        },
                    }),
                    metadata: None,
                }],
            ],
        }),
    ]
}
