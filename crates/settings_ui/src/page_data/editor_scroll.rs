use super::*;

pub(super) fn scrollbar_section() -> [SettingsPageItem; 11] {
    [
        SettingsPageItem::SectionHeader("Scrollbar"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show",
            description: "When to show the scrollbar in the editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("scrollbar"),
                pick: |settings_content| settings_content.editor.scrollbar.as_ref()?.show.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .show = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Size",
            description: "Size of the editor scrollbar in pixels.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("scrollbar.size"),
                pick: |settings_content| settings_content.editor.scrollbar.as_ref()?.size.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .size = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Cursors",
            description: "Show cursor positions in the scrollbar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("scrollbar.cursors"),
                pick: |settings_content| {
                    settings_content.editor.scrollbar.as_ref()?.cursors.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .cursors = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Git Diff",
            description: "Show Git diff indicators in the scrollbar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("scrollbar.git_diff"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .git_diff
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .git_diff = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Search Results",
            description: "Show buffer search result indicators in the scrollbar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("scrollbar.search_results"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .search_results
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .search_results = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Selected Text",
            description: "Show selected text occurrences in the scrollbar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("scrollbar.selected_text"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .selected_text
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .selected_text = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Selected Symbol",
            description: "Show selected symbol occurrences in the scrollbar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("scrollbar.selected_symbol"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .selected_symbol
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .selected_symbol = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Diagnostics",
            description: "Which diagnostic indicators to show in the scrollbar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("scrollbar.diagnostics"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .diagnostics
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .diagnostics = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Horizontal Scrollbar",
            description: "When false, forcefully disables the horizontal scrollbar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("scrollbar.axes.horizontal"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .axes
                        .as_ref()?
                        .horizontal
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .axes
                        .get_or_insert_default()
                        .horizontal = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Vertical Scrollbar",
            description: "When false, forcefully disables the vertical scrollbar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("scrollbar.axes.vertical"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .axes
                        .as_ref()?
                        .vertical
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .axes
                        .get_or_insert_default()
                        .vertical = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn minimap_section() -> [SettingsPageItem; 7] {
    [
        SettingsPageItem::SectionHeader("Minimap"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show",
            description: "When to show the minimap in the editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("minimap.show"),
                pick: |settings_content| settings_content.editor.minimap.as_ref()?.show.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.minimap.get_or_insert_default().show = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Display In",
            description: "Where to show the minimap in the editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("minimap.display_in"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .minimap
                        .as_ref()?
                        .display_in
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .minimap
                        .get_or_insert_default()
                        .display_in = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Thumb",
            description: "When to show the minimap thumb.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("minimap.thumb"),
                pick: |settings_content| settings_content.editor.minimap.as_ref()?.thumb.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .minimap
                        .get_or_insert_default()
                        .thumb = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Thumb Border",
            description: "Border style for the minimap's scrollbar thumb.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("minimap.thumb_border"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .minimap
                        .as_ref()?
                        .thumb_border
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .minimap
                        .get_or_insert_default()
                        .thumb_border = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Current Line Highlight",
            description: "How to highlight the current line in the minimap.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("minimap.current_line_highlight"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .minimap
                        .as_ref()
                        .and_then(|minimap| minimap.current_line_highlight.as_ref())
                        .or(settings_content.editor.current_line_highlight.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .minimap
                        .get_or_insert_default()
                        .current_line_highlight = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Max Width Columns",
            description: "Maximum number of columns to display in the minimap.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("minimap.max_width_columns"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .minimap
                        .as_ref()?
                        .max_width_columns
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .minimap
                        .get_or_insert_default()
                        .max_width_columns = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}
