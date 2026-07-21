use super::*;

pub(super) fn terminal_font_section() -> [SettingsPageItem; 6] {
    [
        SettingsPageItem::SectionHeader("Font"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Font Size",
            description: "Font size for terminal text. If not set, defaults to buffer font size.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.font_size"),
                pick: |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()
                        .and_then(|terminal| terminal.font_size.as_ref())
                        .or(settings_content.theme.buffer_font_size.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content.terminal.get_or_insert_default().font_size = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Font Family",
            description: "Font family for terminal text. If not set, defaults to buffer font family.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.font_family"),
                pick: |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()
                        .and_then(|terminal| terminal.font_family.as_ref())
                        .or(settings_content.theme.buffer_font_family.as_ref())
                },
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .font_family = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Font Fallbacks",
            description: "Font fallbacks for terminal text. If not set, defaults to buffer font fallbacks.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("terminal.font_fallbacks"),
                    pick: |settings_content| {
                        settings_content
                            .terminal
                            .as_ref()
                            .and_then(|terminal| terminal.font_fallbacks.as_ref())
                            .or(settings_content.theme.buffer_font_fallbacks.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .font_fallbacks = value;
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Font Weight",
            description: "Font weight for terminal text in CSS weight units (100-900).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("terminal.font_weight"),
                pick: |settings_content| settings_content.terminal.as_ref()?.font_weight.as_ref(),
                write: |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .font_weight = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Font Features",
            description: "Font features for terminal text.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("terminal.font_features"),
                    pick: |settings_content| {
                        settings_content
                            .terminal
                            .as_ref()
                            .and_then(|terminal| terminal.font_features.as_ref())
                            .or(settings_content.theme.buffer_font_features.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .font_features = value;
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER,
        }),
    ]
}
