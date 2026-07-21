use super::*;

pub(super) fn agent_configuration_section(cx: &App) -> Box<[SettingsPageItem]> {
    use feature_flags::FeatureFlagAppExt as _;

    // The LLM provider and MCP server pages are gated behind a feature flag
    // while their configuration is being moved out of the agent panel.
    let agent_settings_ui_enabled = cx.has_flag::<feature_flags::AgentSettingsUiFeatureFlag>();

    let mut items = vec![SettingsPageItem::SectionHeader("Agent Configuration")];

    if agent_settings_ui_enabled {
        items.push(SettingsPageItem::SubPageLink(SubPageLink {
            title: "LLM Providers".into(),
            r#type: Default::default(),
            json_path: Some("llm_providers"),
            description: Some("Configure API keys and settings for LLM providers.".into()),
            in_json: false,
            files: USER,
            render: render_llm_providers_page,
        }));
    }

    items.extend([
        SettingsPageItem::SubPageLink(SubPageLink {
            title: "Skills".into(),
            r#type: Default::default(),
            json_path: Some(mav_actions::AGENT_SKILLS_SETTINGS_PATH),
            description: Some("View and manage agent skills installed globally or in project worktrees.".into()),
            in_json: false,
            files: USER | PROJECT,
            render: render_skills_setup_page,
        }),
        SettingsPageItem::SubPageLink(SubPageLink {
            title: "Sandbox".into(),
            r#type: Default::default(),
            json_path: Some(mav_actions::AGENT_SANDBOX_SETTINGS_PATH),
            description: Some(
                "Review and change the elevated terminal sandbox permissions that are always allowed without prompting."
                    .into(),
            ),
            in_json: true,
            files: USER,
            render: render_sandbox_settings_page,
        }),
        SettingsPageItem::SubPageLink(SubPageLink {
            title: "Tool Permissions".into(),
            r#type: Default::default(),
            json_path: Some("agent.tool_permissions"),
            description: Some("Set up regex patterns to auto-allow, auto-deny, or always request confirmation, for specific tool inputs.".into()),
            in_json: true,
            files: USER,
            render: render_tool_permissions_setup_page,
        }),
    ]);

    if agent_settings_ui_enabled {
        items.push(SettingsPageItem::SubPageLink(SubPageLink {
            title: "MCP Servers".into(),
            r#type: Default::default(),
            json_path: Some("context_servers"),
            description: Some(
                "View, add, configure, and remove Model Context Protocol servers.".into(),
            ),
            in_json: false,
            files: USER,
            render: render_mcp_servers_page,
        }));
        items.push(SettingsPageItem::SubPageLink(SubPageLink {
            title: "External Agents".into(),
            r#type: Default::default(),
            json_path: Some("agent_servers"),
            description: Some(
                "View, add, and remove agents connected through the Agent Client Protocol.".into(),
            ),
            in_json: false,
            files: USER,
            render: render_external_agents_page,
        }));
    }

    items.extend([
        SettingsPageItem::SettingItem(SettingItem {
            title: "Single File Review",
            description: "When enabled, agent edits will also be displayed in single-file buffers for review.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.single_file_review"),
                pick: |settings_content| {
                    settings_content.agent.as_ref()?.single_file_review.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .single_file_review = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Enable Feedback",
            description: "Show voting thumbs up/down icon buttons for feedback on agent edits.",
            field: Box::new(SettingField {
                organization_override: Some(|org_config| if org_config.is_agent_thread_feedback_enabled {
                    None
                } else {
                    Some(&false)
                }),
                json_path: Some("agent.enable_feedback"),
                pick: |settings_content| {
                    settings_content.agent.as_ref()?.enable_feedback.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .enable_feedback = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Notify When Agent Waiting",
            description: "Where to show notifications when the agent has completed its response or needs confirmation before running a tool action.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.notify_when_agent_waiting"),
                pick: |settings_content| {
                    settings_content
                        .agent
                        .as_ref()?
                        .notify_when_agent_waiting
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .notify_when_agent_waiting = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Play Sound When Agent Done",
            description: "When to play a sound when the agent has either completed its response, or needs user input.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.play_sound_when_agent_done"),
                pick: |settings_content| {
                    settings_content
                        .agent
                        .as_ref()?
                        .play_sound_when_agent_done
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .play_sound_when_agent_done = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Expand Edit Card",
            description: "Whether to have edit cards in the agent panel expanded, showing a Preview of the diff.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.expand_edit_card"),
                pick: |settings_content| {
                    settings_content.agent.as_ref()?.expand_edit_card.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .expand_edit_card = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Expand Terminal Card",
            description: "Whether to have terminal cards in the agent panel expanded, showing the whole command output.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.expand_terminal_card"),
                pick: |settings_content| {
                    settings_content
                        .agent
                        .as_ref()?
                        .expand_terminal_card
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .expand_terminal_card = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Terminal Thread Init Command",
            description: "Command to automatically run when Mav creates a Terminal Thread shell in the agent panel. Runs in your configured shell.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.terminal_init_command"),
                pick: |settings_content| {
                    settings_content
                        .agent
                        .as_ref()?
                        .terminal_init_command
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .terminal_init_command = value;
                },
            }),
            metadata: Some(Box::new(SettingsFieldMetadata {
                placeholder: Some("e.g. claude"),
                display_confirm_button: true,
                display_clear_button: true,
                confirm_on_focus_out: true,
                treat_missing_text_as_empty: true,
                ..Default::default()
            })),
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Thinking Display",
            description: "How thinking blocks should be displayed by default. 'Auto' fully expands during streaming, then auto-collapses when done. 'Preview' auto-expands with a height constraint during streaming. 'Always Expanded' shows full content. 'Always Collapsed' keeps them collapsed.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.thinking_display"),
                pick: |settings_content| {
                    settings_content
                        .agent
                        .as_ref()?
                        .thinking_display
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .thinking_display = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Cancel Generation On Terminal Stop",
            description: "Whether clicking the stop button on a running terminal tool should also cancel the agent's generation. Note that this only applies to the stop button, not to ctrl+c inside the terminal.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.cancel_generation_on_terminal_stop"),
                pick: |settings_content| {
                    settings_content
                        .agent
                        .as_ref()?
                        .cancel_generation_on_terminal_stop
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .cancel_generation_on_terminal_stop = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Use Modifier To Send",
            description: "Whether to always use cmd-enter (or ctrl-enter on Linux or Windows) to send messages.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.use_modifier_to_send"),
                pick: |settings_content| {
                    settings_content
                        .agent
                        .as_ref()?
                        .use_modifier_to_send
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .use_modifier_to_send = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Message Editor Min Lines",
            description: "Minimum number of lines to display in the agent message editor.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.message_editor_min_lines"),
                pick: |settings_content| {
                    settings_content
                        .agent
                        .as_ref()?
                        .message_editor_min_lines
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .message_editor_min_lines = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Turn Stats",
            description: "Whether to show turn statistics like elapsed time during generation and final turn duration.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.show_turn_stats"),
                pick: |settings_content| {
                    settings_content.agent.as_ref()?.show_turn_stats.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .show_turn_stats = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Merge Conflict Indicator",
            description: "Whether to show the merge conflict indicator in the status bar that offers to resolve conflicts using the agent.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.show_merge_conflict_indicator"),
                pick: |settings_content| {
                    settings_content.agent.as_ref()?.show_merge_conflict_indicator.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .show_merge_conflict_indicator = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
    ]);

    items.extend([
        SettingsPageItem::SettingItem(SettingItem {
            title: "Auto Compact",
            description: "Automatically compact the agent's context when it grows too large, summarizing earlier messages to free up room in the model's context window.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.auto_compact.enabled"),
                pick: |settings_content| {
                    settings_content
                        .agent
                        .as_ref()?
                        .auto_compact
                        .as_ref()?
                        .enabled
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .auto_compact
                        .get_or_insert_default()
                        .enabled = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Auto Compact Threshold",
            description: "When auto compaction runs. A percentage string like \"90%\" is measured against the context window. A positive integer is the number of used tokens to compact after. A negative integer is the number of tokens remaining in the context window before compacting.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("agent.auto_compact.threshold"),
                pick: |settings_content| {
                    settings_content
                        .agent
                        .as_ref()?
                        .auto_compact
                        .as_ref()?
                        .threshold
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .auto_compact
                        .get_or_insert_default()
                        .threshold = value;
                },
            }),
            metadata: Some(Box::new(SettingsFieldMetadata {
                placeholder: Some("90%"),
                ..Default::default()
            })),
            files: USER,
        }),
    ]);

    items.into_boxed_slice()
}
