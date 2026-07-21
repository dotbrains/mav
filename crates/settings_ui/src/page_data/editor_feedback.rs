use super::*;

pub(super) fn signature_help_section() -> [SettingsPageItem; 4] {
    [
        SettingsPageItem::SectionHeader("Signature Help"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Auto Signature Help",
            description: "Automatically show a signature help pop-up.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("auto_signature_help"),
                pick: |settings_content| settings_content.editor.auto_signature_help.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.auto_signature_help = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Signature Help After Edits",
            description: "Show the signature help pop-up after completions or bracket pairs are inserted.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("show_signature_help_after_edits"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .show_signature_help_after_edits
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content.editor.show_signature_help_after_edits = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Snippet Sort Order",
            description: "Determines how snippets are sorted relative to other completion items.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("snippet_sort_order"),
                pick: |settings_content| settings_content.editor.snippet_sort_order.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.snippet_sort_order = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn hover_popover_section() -> [SettingsPageItem; 5] {
    [
        SettingsPageItem::SectionHeader("Hover Popover"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Enabled",
            description: "Show the informational hover box when moving the mouse over symbols in the editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("hover_popover_enabled"),
                pick: |settings_content| settings_content.editor.hover_popover_enabled.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.hover_popover_enabled = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        // todo(settings ui): add units to this number input
        SettingsPageItem::SettingItem(SettingItem {
            title: "Delay",
            description: "Time to wait in milliseconds before showing the informational hover box.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("hover_popover_delay"),
                pick: |settings_content| settings_content.editor.hover_popover_delay.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.hover_popover_delay = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Sticky",
            description: "Whether the hover popover sticks when the mouse moves toward it, allowing interaction with its contents.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("hover_popover_sticky"),
                pick: |settings_content| settings_content.editor.hover_popover_sticky.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.hover_popover_sticky = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        // todo(settings ui): add units to this number input
        SettingsPageItem::SettingItem(SettingItem {
            title: "Hiding Delay",
            description: "Time to wait in milliseconds before hiding the hover popover after the mouse moves away.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("hover_popover_hiding_delay"),
                pick: |settings_content| {
                    settings_content.editor.hover_popover_hiding_delay.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content.editor.hover_popover_hiding_delay = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn drag_and_drop_selection_section() -> [SettingsPageItem; 3] {
    [
        SettingsPageItem::SectionHeader("Drag And Drop Selection"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Enabled",
            description: "Enable drag and drop selection.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("drag_and_drop_selection.enabled"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .drag_and_drop_selection
                        .as_ref()
                        .and_then(|drag_and_drop| drag_and_drop.enabled.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .drag_and_drop_selection
                        .get_or_insert_default()
                        .enabled = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Delay",
            description: "Delay in milliseconds before drag and drop selection starts.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("drag_and_drop_selection.delay"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .drag_and_drop_selection
                        .as_ref()
                        .and_then(|drag_and_drop| drag_and_drop.delay.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .drag_and_drop_selection
                        .get_or_insert_default()
                        .delay = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn gutter_section() -> [SettingsPageItem; 9] {
    [
        SettingsPageItem::SectionHeader("Gutter"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Line Numbers",
            description: "Show line numbers in the gutter.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("gutter.line_numbers"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .gutter
                        .as_ref()
                        .and_then(|gutter| gutter.line_numbers.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .gutter
                        .get_or_insert_default()
                        .line_numbers = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Relative Line Numbers",
            description: "Controls line number display in the editor's gutter. \"disabled\" shows absolute line numbers, \"enabled\" shows relative line numbers for each absolute line, and \"wrapped\" shows relative line numbers for every line, absolute or wrapped.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("relative_line_numbers"),
                pick: |settings_content| settings_content.editor.relative_line_numbers.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.relative_line_numbers = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Runnables",
            description: "Show runnable buttons in the gutter.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("gutter.runnables"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .gutter
                        .as_ref()
                        .and_then(|gutter| gutter.runnables.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .gutter
                        .get_or_insert_default()
                        .runnables = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Breakpoints",
            description: "Show breakpoints in the gutter.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("gutter.breakpoints"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .gutter
                        .as_ref()
                        .and_then(|gutter| gutter.breakpoints.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .gutter
                        .get_or_insert_default()
                        .breakpoints = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Bookmarks",
            description: "Show bookmarks in the gutter.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("gutter.bookmarks"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .gutter
                        .as_ref()
                        .and_then(|gutter| gutter.bookmarks.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .gutter
                        .get_or_insert_default()
                        .bookmarks = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Folds",
            description: "Show code folding controls in the gutter.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("gutter.folds"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .gutter
                        .as_ref()
                        .and_then(|gutter| gutter.folds.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content.editor.gutter.get_or_insert_default().folds = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Min Line Number Digits",
            description: "Minimum number of characters to reserve space for in the gutter.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("gutter.min_line_number_digits"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .gutter
                        .as_ref()
                        .and_then(|gutter| gutter.min_line_number_digits.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content
                        .editor
                        .gutter
                        .get_or_insert_default()
                        .min_line_number_digits = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Inline Code Actions",
            description: "Show code action button at start of buffer line.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("inline_code_actions"),
                pick: |settings_content| settings_content.editor.inline_code_actions.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.inline_code_actions = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}
