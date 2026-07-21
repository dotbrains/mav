use super::*;

pub(super) fn preview_tabs_section() -> [SettingsPageItem; 8] {
    [
        SettingsPageItem::SectionHeader("Preview Tabs"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Preview Tabs Enabled",
            description: "Show opened editors as preview tabs.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("preview_tabs.enabled"),
                pick: |settings_content| settings_content.preview_tabs.as_ref()?.enabled.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enabled = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Enable Preview From Project Panel",
            description: "Whether to open tabs in preview mode when opened from the project panel with a single click.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("preview_tabs.enable_preview_from_project_panel"),
                pick: |settings_content| {
                    settings_content
                        .preview_tabs
                        .as_ref()?
                        .enable_preview_from_project_panel
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enable_preview_from_project_panel = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Enable Preview From File Finder",
            description: "Whether to open tabs in preview mode when selected from the file finder.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("preview_tabs.enable_preview_from_file_finder"),
                pick: |settings_content| {
                    settings_content
                        .preview_tabs
                        .as_ref()?
                        .enable_preview_from_file_finder
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enable_preview_from_file_finder = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Enable Preview From Multibuffer",
            description: "Whether to open tabs in preview mode when opened from a multibuffer.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("preview_tabs.enable_preview_from_multibuffer"),
                pick: |settings_content| {
                    settings_content
                        .preview_tabs
                        .as_ref()?
                        .enable_preview_from_multibuffer
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enable_preview_from_multibuffer = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Enable Preview Multibuffer From Code Navigation",
            description: "Whether to open tabs in preview mode when code navigation is used to open a multibuffer.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("preview_tabs.enable_preview_multibuffer_from_code_navigation"),
                pick: |settings_content| {
                    settings_content
                        .preview_tabs
                        .as_ref()?
                        .enable_preview_multibuffer_from_code_navigation
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enable_preview_multibuffer_from_code_navigation = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Enable Preview File From Code Navigation",
            description: "Whether to open tabs in preview mode when code navigation is used to open a single file.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("preview_tabs.enable_preview_file_from_code_navigation"),
                pick: |settings_content| {
                    settings_content
                        .preview_tabs
                        .as_ref()?
                        .enable_preview_file_from_code_navigation
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enable_preview_file_from_code_navigation = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Enable Keep Preview On Code Navigation",
            description: "Whether to keep tabs in preview mode when code navigation is used to navigate away from them. If `enable_preview_file_from_code_navigation` or `enable_preview_multibuffer_from_code_navigation` is also true, the new tab may replace the existing one.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("preview_tabs.enable_keep_preview_on_code_navigation"),
                pick: |settings_content| {
                    settings_content
                        .preview_tabs
                        .as_ref()?
                        .enable_keep_preview_on_code_navigation
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enable_keep_preview_on_code_navigation = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn layout_section() -> [SettingsPageItem; 6] {
    [
        SettingsPageItem::SectionHeader("Layout"),
        SettingsPageItem::SettingItem(SettingItem {
            files: USER,
            title: "Card Gap",
            description: "Gap between workspace cards, in pixels.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("card_gap"),
                pick: |settings_content| settings_content.workspace.card_gap.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.workspace.card_gap = value;
                },
            }),
            metadata: None,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            files: USER,
            title: "Centered Layout Left Padding",
            description: "Left padding for centered layout.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("centered_layout.left_padding"),
                pick: |settings_content| {
                    settings_content
                        .workspace
                        .centered_layout
                        .as_ref()?
                        .left_padding
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .workspace
                        .centered_layout
                        .get_or_insert_default()
                        .left_padding = value;
                },
            }),
            metadata: None,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            files: USER,
            title: "Centered Layout Right Padding",
            description: "Right padding for centered layout.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("centered_layout.right_padding"),
                pick: |settings_content| {
                    settings_content
                        .workspace
                        .centered_layout
                        .as_ref()?
                        .right_padding
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .workspace
                        .centered_layout
                        .get_or_insert_default()
                        .right_padding = value;
                },
            }),
            metadata: None,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Focus Follows Mouse",
            description: "Whether to change focus to a pane when the mouse hovers over it.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("focus_follows_mouse.enabled"),
                pick: |settings_content| {
                    settings_content
                        .workspace
                        .focus_follows_mouse
                        .as_ref()
                        .and_then(|s| s.enabled.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content
                        .workspace
                        .focus_follows_mouse
                        .get_or_insert_default()
                        .enabled = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Focus Follows Mouse Debounce ms",
            description: "Amount of time to wait before changing focus.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("focus_follows_mouse.debounce_ms"),
                pick: |settings_content| {
                    settings_content
                        .workspace
                        .focus_follows_mouse
                        .as_ref()
                        .and_then(|s| s.debounce_ms.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content
                        .workspace
                        .focus_follows_mouse
                        .get_or_insert_default()
                        .debounce_ms = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn window_section() -> [SettingsPageItem; 3] {
    [
        SettingsPageItem::SectionHeader("Window"),
        // todo(settings_ui): Should we filter by platform.as_ref()?
        SettingsPageItem::SettingItem(SettingItem {
            title: "Use System Window Tabs",
            description: "(macOS only) whether to allow Windows to tab together.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("use_system_window_tabs"),
                pick: |settings_content| settings_content.workspace.use_system_window_tabs.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.workspace.use_system_window_tabs = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Window Decorations",
            description: "(Linux only) whether Mav or your compositor should draw window decorations.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("window_decorations"),
                pick: |settings_content| settings_content.workspace.window_decorations.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.workspace.window_decorations = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn pane_modifiers_section() -> [SettingsPageItem; 4] {
    [
        SettingsPageItem::SectionHeader("Pane Modifiers"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Inactive Opacity",
            description: "Opacity of inactive panels (0.0 - 1.0).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("active_pane_modifiers.inactive_opacity"),
                pick: |settings_content| {
                    settings_content
                        .workspace
                        .active_pane_modifiers
                        .as_ref()?
                        .inactive_opacity
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .workspace
                        .active_pane_modifiers
                        .get_or_insert_default()
                        .inactive_opacity = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Border Size",
            description: "Size of the border surrounding the active pane.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("active_pane_modifiers.border_size"),
                pick: |settings_content| {
                    settings_content
                        .workspace
                        .active_pane_modifiers
                        .as_ref()?
                        .border_size
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .workspace
                        .active_pane_modifiers
                        .get_or_insert_default()
                        .border_size = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Zoomed Padding",
            description: "Show padding for zoomed panes.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("zoomed_padding"),
                pick: |settings_content| settings_content.workspace.zoomed_padding.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.workspace.zoomed_padding = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn pane_split_direction_section() -> [SettingsPageItem; 3] {
    [
        SettingsPageItem::SectionHeader("Pane Split Direction"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Vertical Split Direction",
            description: "Direction to split vertically.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("pane_split_direction_vertical"),
                pick: |settings_content| {
                    settings_content
                        .workspace
                        .pane_split_direction_vertical
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content.workspace.pane_split_direction_vertical = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Horizontal Split Direction",
            description: "Direction to split horizontally.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("pane_split_direction_horizontal"),
                pick: |settings_content| {
                    settings_content
                        .workspace
                        .pane_split_direction_horizontal
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content.workspace.pane_split_direction_horizontal = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}
