use super::*;

pub(super) fn toolbar_section() -> [SettingsPageItem; 8] {
    [
        SettingsPageItem::SectionHeader("Toolbar"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Breadcrumbs",
            description: "Show breadcrumbs.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("toolbar.breadcrumbs"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .toolbar
                        .as_ref()?
                        .breadcrumbs
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .toolbar
                        .get_or_insert_default()
                        .breadcrumbs = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Compact Mode",
            description: "Use compact toolbar height.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("toolbar.compact_mode"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .toolbar
                        .as_ref()?
                        .compact_mode
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .toolbar
                        .get_or_insert_default()
                        .compact_mode = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Breadcrumb Symbols",
            description: "Show document symbols in breadcrumbs.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("toolbar.show_breadcrumb_symbols"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .toolbar
                        .as_ref()?
                        .show_breadcrumb_symbols
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .toolbar
                        .get_or_insert_default()
                        .show_breadcrumb_symbols = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Quick Actions",
            description: "Show quick action buttons (e.g., search, selection, editor controls, etc.).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("toolbar.quick_actions"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .toolbar
                        .as_ref()?
                        .quick_actions
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .toolbar
                        .get_or_insert_default()
                        .quick_actions = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Selections Menu",
            description: "Show the selections menu in the editor toolbar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("toolbar.selections_menu"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .toolbar
                        .as_ref()?
                        .selections_menu
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .toolbar
                        .get_or_insert_default()
                        .selections_menu = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Agent Review",
            description: "Show agent review buttons in the editor toolbar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("toolbar.agent_review"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .toolbar
                        .as_ref()?
                        .agent_review
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .toolbar
                        .get_or_insert_default()
                        .agent_review = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Code Actions",
            description: "Show code action buttons in the editor toolbar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("toolbar.code_actions"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .toolbar
                        .as_ref()?
                        .code_actions
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .toolbar
                        .get_or_insert_default()
                        .code_actions = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}
