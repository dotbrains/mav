use super::*;

pub(super) fn text_rendering_section() -> [SettingsPageItem; 2] {
    [
        SettingsPageItem::SectionHeader("Text Rendering"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Text Rendering Mode",
            description: "The text rendering mode to use.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("text_rendering_mode"),
                pick: |settings_content| settings_content.workspace.text_rendering_mode.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.workspace.text_rendering_mode = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn cursor_section() -> [SettingsPageItem; 5] {
    [
        SettingsPageItem::SectionHeader("Cursor"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Multi Cursor Modifier",
            description: "Modifier key for adding multiple cursors.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("multi_cursor_modifier"),
                pick: |settings_content| settings_content.editor.multi_cursor_modifier.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.multi_cursor_modifier = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Cursor Blink",
            description: "Whether the cursor blinks in the editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("cursor_blink"),
                pick: |settings_content| settings_content.editor.cursor_blink.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.cursor_blink = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Cursor Shape",
            description: "Cursor shape for the editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("cursor_shape"),
                pick: |settings_content| settings_content.editor.cursor_shape.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.cursor_shape = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Hide Mouse",
            description: "When to hide the mouse cursor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("hide_mouse"),
                pick: |settings_content| settings_content.hide_mouse.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.hide_mouse = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn highlighting_section() -> [SettingsPageItem; 6] {
    [
        SettingsPageItem::SectionHeader("Highlighting"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Unnecessary Code Fade",
            description: "How much to fade out unused code (0.0 - 0.9).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("unnecessary_code_fade"),
                pick: |settings_content| settings_content.theme.unnecessary_code_fade.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.theme.unnecessary_code_fade = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Current Line Highlight",
            description: "How to highlight the current line.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("current_line_highlight"),
                pick: |settings_content| settings_content.editor.current_line_highlight.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.current_line_highlight = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Selection Highlight",
            description: "Highlight all occurrences of selected text.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("selection_highlight"),
                pick: |settings_content| settings_content.editor.selection_highlight.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.selection_highlight = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Rounded Selection",
            description: "Whether the text selection should have rounded corners.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("rounded_selection"),
                pick: |settings_content| settings_content.editor.rounded_selection.as_ref(),
                write: |settings_content, value, _| {
                    settings_content.editor.rounded_selection = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Minimum Contrast For Highlights",
            description: "The minimum APCA perceptual contrast to maintain when rendering text over highlight backgrounds.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("minimum_contrast_for_highlights"),
                pick: |settings_content| {
                    settings_content
                        .editor
                        .minimum_contrast_for_highlights
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content.editor.minimum_contrast_for_highlights = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]
}

pub(super) fn guides_section() -> [SettingsPageItem; 3] {
    [
        SettingsPageItem::SectionHeader("Guides"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Wrap Guides",
            description: "Show wrap guides (vertical rulers).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("show_wrap_guides"),
                pick: |settings_content| {
                    settings_content
                        .project
                        .all_languages
                        .defaults
                        .show_wrap_guides
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project
                        .all_languages
                        .defaults
                        .show_wrap_guides = value;
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        // todo(settings_ui): This needs a custom component
        SettingsPageItem::SettingItem(SettingItem {
            title: "Wrap Guides",
            description: "Character counts at which to show wrap guides.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("wrap_guides"),
                    pick: |settings_content| {
                        settings_content
                            .project
                            .all_languages
                            .defaults
                            .wrap_guides
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.project.all_languages.defaults.wrap_guides = value;
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER | PROJECT,
        }),
    ]
}
