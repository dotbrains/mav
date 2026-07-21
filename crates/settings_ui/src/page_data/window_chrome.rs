use super::*;

pub(super) fn sidebar_chrome_section() -> [SettingsPageItem; 11] {
    [
        SettingsPageItem::SectionHeader("Sidebar"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Project Pane Button",
            description: "Show the project pane toggle button in the sidebar header.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("sidebar.show_project_pane_button"),
                pick: |settings_content| {
                    settings_content
                        .sidebar
                        .as_ref()?
                        .show_project_pane_button
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .sidebar
                        .get_or_insert_default()
                        .show_project_pane_button = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Branch Status Icon",
            description: "Show git status indicators on the branch icon in the sidebar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("sidebar.show_branch_status_icon"),
                pick: |settings_content| {
                    settings_content
                        .sidebar
                        .as_ref()?
                        .show_branch_status_icon
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .sidebar
                        .get_or_insert_default()
                        .show_branch_status_icon = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Branch Name",
            description: "Show the branch name button in the sidebar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("sidebar.show_branch_name"),
                pick: |settings_content| {
                    settings_content
                        .sidebar
                        .as_ref()?
                        .show_branch_name
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .sidebar
                        .get_or_insert_default()
                        .show_branch_name = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Project Items",
            description: "Show the project host and name in the sidebar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("sidebar.show_project_items"),
                pick: |settings_content| {
                    settings_content
                        .sidebar
                        .as_ref()?
                        .show_project_items
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .sidebar
                        .get_or_insert_default()
                        .show_project_items = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Onboarding Banner",
            description: "Show banners announcing new features in the sidebar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("sidebar.show_onboarding_banner"),
                pick: |settings_content| {
                    settings_content
                        .sidebar
                        .as_ref()?
                        .show_onboarding_banner
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .sidebar
                        .get_or_insert_default()
                        .show_onboarding_banner = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Sign In",
            description: "Show the sign in button in the sidebar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("sidebar.show_sign_in"),
                pick: |settings_content| {
                    settings_content.sidebar.as_ref()?.show_sign_in.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .sidebar
                        .get_or_insert_default()
                        .show_sign_in = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show User Menu",
            description: "Show the user menu button in the sidebar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("sidebar.show_user_menu"),
                pick: |settings_content| {
                    settings_content.sidebar.as_ref()?.show_user_menu.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .sidebar
                        .get_or_insert_default()
                        .show_user_menu = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show User Picture",
            description: "Show user picture in the sidebar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("sidebar.show_user_picture"),
                pick: |settings_content| {
                    settings_content
                        .sidebar
                        .as_ref()?
                        .show_user_picture
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .sidebar
                        .get_or_insert_default()
                        .show_user_picture = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Menus",
            description: "Show the menus in the sidebar.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("sidebar.show_menus"),
                pick: |settings_content| {
                    settings_content.sidebar.as_ref()?.show_menus.as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .sidebar
                        .get_or_insert_default()
                        .show_menus = value;
                },
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::DynamicItem(DynamicItem {
            discriminant: SettingItem {
                files: USER,
                title: "Button Layout",
                description:
                    "(Linux only) choose how window control buttons are laid out in the sidebar.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("sidebar.button_layout$"),
                    pick: |settings_content| {
                        Some(
                            &dynamic_variants::<settings::WindowButtonLayoutContent>()[settings_content
                                .sidebar
                                .as_ref()?
                                .button_layout
                                .as_ref()?
                                .discriminant()
                                as usize],
                        )
                    },
                    write: |settings_content, value, _| {
                        let Some(value) = value else {
                            settings_content
                                .sidebar
                                .get_or_insert_default()
                                .button_layout = None;
                            return;
                        };

                        let current_custom_layout = settings_content
                            .sidebar
                            .as_ref()
                            .and_then(|sidebar| sidebar.button_layout.as_ref())
                            .and_then(|button_layout| match button_layout {
                                settings::WindowButtonLayoutContent::Custom(layout) => {
                                    Some(layout.clone())
                                }
                                _ => None,
                            });

                        let button_layout = match value {
                            settings::WindowButtonLayoutContentDiscriminants::PlatformDefault => {
                                settings::WindowButtonLayoutContent::PlatformDefault
                            }
                            settings::WindowButtonLayoutContentDiscriminants::Standard => {
                                settings::WindowButtonLayoutContent::Standard
                            }
                            settings::WindowButtonLayoutContentDiscriminants::Custom => {
                                settings::WindowButtonLayoutContent::Custom(
                                    current_custom_layout.unwrap_or_else(|| {
                                        "close:minimize,maximize".to_string()
                                    }),
                                )
                            }
                        };

                        settings_content
                            .sidebar
                            .get_or_insert_default()
                            .button_layout = Some(button_layout);
                    },
                }),
                metadata: None,
            },
            pick_discriminant: |settings_content| {
                Some(
                    settings_content
                        .sidebar
                        .as_ref()?
                        .button_layout
                        .as_ref()?
                        .discriminant() as usize,
                )
            },
            fields: dynamic_variants::<settings::WindowButtonLayoutContent>()
                .into_iter()
                .map(|variant| match variant {
                    settings::WindowButtonLayoutContentDiscriminants::PlatformDefault => {
                        vec![]
                    }
                    settings::WindowButtonLayoutContentDiscriminants::Standard => vec![],
                    settings::WindowButtonLayoutContentDiscriminants::Custom => vec![
                        SettingItem {
                            files: USER,
                            title: "Custom Button Layout",
                            description:
                                "GNOME-style layout string such as \"close:minimize,maximize\".",
                            field: Box::new(SettingField {
                                organization_override: None,
                                json_path: Some("sidebar.button_layout"),
                                pick: |settings_content| match settings_content
                                    .sidebar
                                    .as_ref()?
                                    .button_layout
                                    .as_ref()?
                                {
                                    settings::WindowButtonLayoutContent::Custom(layout) => {
                                        Some(layout)
                                    }
                                    _ => DEFAULT_EMPTY_STRING,
                                },
                                write: |settings_content, value, _| {
                                    settings_content
                                        .sidebar
                                        .get_or_insert_default()
                                        .button_layout = value
                                        .map(settings::WindowButtonLayoutContent::Custom);
                                },
                            }),
                            metadata: Some(Box::new(SettingsFieldMetadata {
                                placeholder: Some("close:minimize,maximize"),
                                ..Default::default()
                            })),
                        },
                    ],
                })
                .collect(),
        }),
    ]
}
