use super::ai_agent_configuration::agent_configuration_section;
use super::*;

pub(super) fn ai_page(cx: &App) -> SettingsPage {
    fn general_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader("General"),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Disable AI",
                description: "Whether to disable all AI features in Mav.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("disable_ai"),
                    pick: |settings_content| settings_content.project.disable_ai.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.project.disable_ai = value;
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Sidebar Side",
                description: "Which side of the window the sidebar appears on.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("sidebar.side"),
                    pick: |settings_content| settings_content.sidebar.as_ref()?.side.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.sidebar.get_or_insert_default().side = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn context_servers_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader("Context Servers"),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Context Server Timeout",
                description: "Default timeout in seconds for context server tool calls. Can be overridden per-server in context_servers configuration.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("context_server_timeout"),
                    pick: |settings_content| {
                        settings_content.project.context_server_timeout.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.project.context_server_timeout = value;
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn edit_prediction_display_sub_section() -> [SettingsPageItem; 1] {
        [SettingsPageItem::SettingItem(SettingItem {
            title: "Display Mode",
            description: "When to show edit predictions previews in buffer. The eager mode displays them inline, while the subtle mode displays them only when holding a modifier key.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("edit_prediction.display_mode"),
                pick: |settings_content| {
                    settings_content
                        .project
                        .all_languages
                        .edit_predictions
                        .as_ref()?
                        .mode
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_default()
                        .mode = value;
                },
            }),
            metadata: None,
            files: USER,
        })]
    }

    use feature_flags::FeatureFlagAppExt as _;

    // When the agent settings UI is enabled, the context server timeout is shown
    // inside the MCP Servers sub-page. Otherwise it remains a standalone section
    // here so it stays reachable.
    let agent_settings_ui_enabled = cx.has_flag::<feature_flags::AgentSettingsUiFeatureFlag>();

    let mut items = concat_sections!(
        @vec,
        general_section(),
        agent_configuration_section(cx),
    );
    if !agent_settings_ui_enabled {
        items.extend(context_servers_section());
    }
    items.extend(concat_sections!(
        @vec,
        edit_prediction_language_settings_section(),
        edit_prediction_display_sub_section(),
    ));

    SettingsPage {
        title: "AI",
        items: items.into(),
    }
}
