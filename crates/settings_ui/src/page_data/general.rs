use super::*;

pub(super) fn developer_page(cx: &App) -> SettingsPage {
    use feature_flags::FeatureFlagAppExt as _;

    let mut items: Vec<SettingsPageItem> = Vec::new();

    // Feature flag overrides are a staff-only affordance, so only surface the section when the overrides are enabled.
    if cx.feature_flag_overrides_enabled() {
        items.push(SettingsPageItem::SectionHeader("Feature Flags"));
        items.push(SettingsPageItem::SubPageLink(SubPageLink {
            title: "Feature Flags".into(),
            r#type: Default::default(),
            description: None,
            json_path: Some("feature_flags"),
            in_json: true,
            files: USER,
            render: crate::pages::render_feature_flags_page,
        }));
    }

    items.push(SettingsPageItem::SectionHeader("Instrumentation"));
    items.push(SettingsPageItem::SettingItem(SettingItem {
        title: "Performance Profiler",
        description: "Collect timing data for foreground and background executor tasks so they can be inspected via `mav: open performance profiler`. May lead to increased memory usage.",
        field: Box::new(SettingField {
            organization_override: None,
            json_path: Some("instrumentation.performance_profiler.enabled"),
            pick: |settings_content| {
                settings_content
                    .instrumentation
                    .as_ref()
                    .and_then(|i| i.performance_profiler.as_ref())
                    .and_then(|p| p.enabled.as_ref())
            },
            write: |settings_content, value, _| {
                settings_content
                    .instrumentation
                    .get_or_insert_default()
                    .performance_profiler
                    .get_or_insert_default()
                    .enabled = value;
            },
        }),
        metadata: None,
        files: USER,
    }));

    SettingsPage {
        title: "Developer",
        items: items.into_boxed_slice(),
    }
}

pub(super) fn general_page(cx: &App) -> SettingsPage {
    fn general_settings_section(_cx: &App) -> Vec<SettingsPageItem> {
        vec![
            SettingsPageItem::SectionHeader("General Settings"),
            SettingsPageItem::SettingItem(SettingItem {
                title: "When Closing With No Tabs",
                description: "What to do when using the 'close active item' action with no tabs.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("when_closing_with_no_tabs"),
                    pick: |settings_content| {
                        settings_content
                            .workspace
                            .when_closing_with_no_tabs
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.when_closing_with_no_tabs = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "On Last Window Closed",
                description: "What to do when the last window is closed.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("on_last_window_closed"),
                    pick: |settings_content| {
                        settings_content.workspace.on_last_window_closed.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.on_last_window_closed = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Use System Path Prompts",
                description: "Use native OS dialogs for 'Open' and 'Save As'.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("use_system_path_prompts"),
                    pick: |settings_content| {
                        settings_content.workspace.use_system_path_prompts.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.use_system_path_prompts = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Use System Prompts",
                description: "Use native OS dialogs for confirmations.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("use_system_prompts"),
                    pick: |settings_content| settings_content.workspace.use_system_prompts.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.workspace.use_system_prompts = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Redact Private Values",
                description: "Hide the values of variables in private files.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("redact_private_values"),
                    pick: |settings_content| settings_content.editor.redact_private_values.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.redact_private_values = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Private Files",
                description: "Globs to match against file paths to determine if a file is private.",
                field: Box::new(
                    SettingField {
                        organization_override: None,
                        json_path: Some("worktree.private_files"),
                        pick: |settings_content| {
                            settings_content.project.worktree.private_files.as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content.project.worktree.private_files = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "CLI Default Open Behavior",
                description: "How `mav <path>` opens directories when no flag is specified.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("cli_default_open_behavior"),
                    pick: |settings_content| {
                        settings_content
                            .workspace
                            .cli_default_open_behavior
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.cli_default_open_behavior = value;
                    },
                }),
                metadata: Some(Box::new(SettingsFieldMetadata {
                    should_do_titlecase: Some(false),
                    ..Default::default()
                })),
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Default Open Behavior",
                description: "How projects open from the UI by default.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("default_open_behavior"),
                    pick: |settings_content| {
                        settings_content.workspace.default_open_behavior.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.default_open_behavior = value;
                    },
                }),
                metadata: Some(Box::new(SettingsFieldMetadata {
                    should_do_titlecase: Some(false),
                    ..Default::default()
                })),
                files: USER,
            }),
        ]
    }
    fn security_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader("Security"),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Trust All Projects By Default",
                description: "When opening Mav, avoid Restricted Mode by auto-trusting all projects, enabling use of all features without having to give permission to each new project.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("session.trust_all_projects"),
                    pick: |settings_content| {
                        settings_content
                            .session
                            .as_ref()
                            .and_then(|session| session.trust_all_worktrees.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .session
                            .get_or_insert_default()
                            .trust_all_worktrees = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn workspace_restoration_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader("Workspace Restoration"),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Restore Unsaved Buffers",
                description: "Whether or not to restore unsaved buffers on restart.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("session.restore_unsaved_buffers"),
                    pick: |settings_content| {
                        settings_content
                            .session
                            .as_ref()
                            .and_then(|session| session.restore_unsaved_buffers.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .session
                            .get_or_insert_default()
                            .restore_unsaved_buffers = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Restore On Startup",
                description: "What to restore from the previous session when opening Mav.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("restore_on_startup"),
                    pick: |settings_content| settings_content.workspace.restore_on_startup.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.workspace.restore_on_startup = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn scoped_settings_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader("Scoped Settings"),
            SettingsPageItem::SettingItem(SettingItem {
                files: USER,
                title: "Preview Channel",
                description: "Which settings should be activated only in Preview build of Mav.",
                field: Box::new(
                    SettingField {
                        organization_override: None,
                        json_path: Some("preview_channel_settings"),
                        pick: |settings_content| Some(settings_content),
                        write: |_settings_content, _value, _| {},
                    }
                    .unimplemented(),
                ),
                metadata: None,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                files: USER,
                title: "Settings Profiles",
                description: "Any number of settings profiles that are temporarily applied on top of your existing user settings.",
                field: Box::new(
                    SettingField {
                        organization_override: None,
                        json_path: Some("settings_profiles"),
                        pick: |settings_content| Some(settings_content),
                        write: |_settings_content, _value, _| {},
                    }
                    .unimplemented(),
                ),
                metadata: None,
            }),
        ]
    }

    fn privacy_section() -> [SettingsPageItem; 4] {
        [
            SettingsPageItem::SectionHeader("Privacy"),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Telemetry Diagnostics",
                description: "Send debug information like crash reports.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("telemetry.diagnostics"),
                    pick: |settings_content| {
                        settings_content
                            .telemetry
                            .as_ref()
                            .and_then(|telemetry| telemetry.diagnostics.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .telemetry
                            .get_or_insert_default()
                            .diagnostics = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Telemetry Metrics",
                description: "Send anonymized usage data like what languages you're using Mav with.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("telemetry.metrics"),
                    pick: |settings_content| {
                        settings_content
                            .telemetry
                            .as_ref()
                            .and_then(|telemetry| telemetry.metrics.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content.telemetry.get_or_insert_default().metrics = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Anthropic Data Retention",
                description: "Allow sending requests to Anthropic models that cannot be offered with Zero Data Retention.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("telemetry.anthropic_retention"),
                    pick: |settings_content| {
                        settings_content
                            .telemetry
                            .as_ref()
                            .and_then(|telemetry| telemetry.anthropic_retention.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .telemetry
                            .get_or_insert_default()
                            .anthropic_retention = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn auto_update_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader("Auto Update"),
            SettingsPageItem::SettingItem(SettingItem {
                title: "Auto Update",
                description: "Whether or not to automatically check for updates.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("auto_update"),
                    pick: |settings_content| settings_content.auto_update.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.auto_update = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    SettingsPage {
        title: "General",
        items: concat_sections!(
            @vec,
            general_settings_section(cx),
            security_section(),
            workspace_restoration_section(),
            scoped_settings_section(),
            privacy_section(),
            auto_update_section(),
        )
        .into(),
    }
}
