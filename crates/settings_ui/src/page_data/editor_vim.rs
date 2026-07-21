use super::*;

pub(super) fn vim_settings_section() -> [SettingsPageItem; 14] {
    [
        SettingsPageItem::SectionHeader("Vim"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Default Mode",
            description: "The default mode when Vim starts.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("vim.default_mode"),
                pick: |settings_content| settings_content.vim.as_ref()?.default_mode.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.vim.get_or_insert_default().default_mode = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Toggle Relative Line Numbers",
            description: "Toggle relative line numbers in Vim mode.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("vim.toggle_relative_line_numbers"),
                pick: |settings_content| {
                    settings_content
                        .vim
                        .as_ref()?
                        .toggle_relative_line_numbers
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .toggle_relative_line_numbers = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Use System Clipboard",
            description: "Controls when to use system clipboard in Vim mode.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("vim.use_system_clipboard"),
                pick: |settings_content| {
                    settings_content.vim.as_ref()?.use_system_clipboard.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .use_system_clipboard = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Use Smartcase Find",
            description: "Enable smartcase searching in Vim mode.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("vim.use_smartcase_find"),
                pick: |settings_content| settings_content.vim.as_ref()?.use_smartcase_find.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .use_smartcase_find = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Global Substitution Default",
            description: "When enabled, the :substitute command replaces all matches in a line by default. The 'g' flag then toggles this behavior.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("vim.gdefault"),
                pick: |settings_content| settings_content.vim.as_ref()?.gdefault.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.vim.get_or_insert_default().gdefault = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Highlight on Yank Duration",
            description: "Duration in milliseconds to highlight yanked text in Vim mode.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("vim.highlight_on_yank_duration"),
                pick: |settings_content| {
                    settings_content
                        .vim
                        .as_ref()?
                        .highlight_on_yank_duration
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .highlight_on_yank_duration = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Regex Search",
            description: "Use regex search by default in Vim search.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("vim.use_regex_search"),
                pick: |settings_content| settings_content.vim.as_ref()?.use_regex_search.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .use_regex_search = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Edit Predictions in Normal Mode",
            description: "Whether edit predictions are shown in normal mode. By default, edit predictions are only shown in insert and replace modes.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("vim.show_edit_predictions_in_normal_mode"),
                pick: |settings_content| {
                    settings_content
                        .vim
                        .as_ref()?
                        .show_edit_predictions_in_normal_mode
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .show_edit_predictions_in_normal_mode = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Cursor Shape - Normal Mode",
            description: "Cursor shape for normal mode.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("vim.cursor_shape.normal"),
                pick: |settings_content| {
                    settings_content
                        .vim
                        .as_ref()?
                        .cursor_shape
                        .as_ref()?
                        .normal
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .cursor_shape
                        .get_or_insert_default()
                        .normal = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Cursor Shape - Insert Mode",
            description: "Cursor shape for insert mode. Inherit uses the editor's cursor shape.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("vim.cursor_shape.insert"),
                pick: |settings_content| {
                    settings_content
                        .vim
                        .as_ref()?
                        .cursor_shape
                        .as_ref()?
                        .insert
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .cursor_shape
                        .get_or_insert_default()
                        .insert = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Cursor Shape - Replace Mode",
            description: "Cursor shape for replace mode.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("vim.cursor_shape.replace"),
                pick: |settings_content| {
                    settings_content
                        .vim
                        .as_ref()?
                        .cursor_shape
                        .as_ref()?
                        .replace
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .cursor_shape
                        .get_or_insert_default()
                        .replace = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Cursor Shape - Visual Mode",
            description: "Cursor shape for visual mode.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("vim.cursor_shape.visual"),
                pick: |settings_content| {
                    settings_content
                        .vim
                        .as_ref()?
                        .cursor_shape
                        .as_ref()?
                        .visual
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .cursor_shape
                        .get_or_insert_default()
                        .visual = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Custom Digraphs",
            description: "Custom digraph mappings for Vim mode.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("vim.custom_digraphs"),
                    pick: |settings_content| {
                        settings_content.vim.as_ref()?.custom_digraphs.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.vim.get_or_insert_default().custom_digraphs = value;
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER,
        }),
    ]
}
