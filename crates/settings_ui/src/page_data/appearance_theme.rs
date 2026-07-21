use super::*;

pub(super) fn theme_section() -> [SettingsPageItem; 3] {
    [
        SettingsPageItem::SectionHeader("Theme"),
        SettingsPageItem::DynamicItem(DynamicItem {
            discriminant: SettingItem {
                files: USER,
                title: "Theme Mode",
                description: "Choose a static, fixed theme or dynamically select themes based on appearance and light/dark modes.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("theme$"),
                    pick: |settings_content| {
                        Some(&dynamic_variants::<settings::ThemeSelection>()[
                            settings_content
                                .theme
                                .theme
                                .as_ref()?
                                .discriminant() as usize])
                    },
                    write: |settings_content, value, app: &App| {
                        let Some(value) = value else {
                            settings_content.theme.theme = None;
                            return;
                        };
                        let settings_value = settings_content.theme.theme.get_or_insert_default();
                        *settings_value = match value {
                            settings::ThemeSelectionDiscriminants::Static => {
                                let name = match settings_value {
                                    settings::ThemeSelection::Static(_) => return,
                                    settings::ThemeSelection::Dynamic { mode, light, dark } => {
                                        match mode {
                                            theme_settings::ThemeAppearanceMode::Light => light.clone(),
                                            theme_settings::ThemeAppearanceMode::Dark => dark.clone(),
                                            theme_settings::ThemeAppearanceMode::System => {
                                                if SystemAppearance::global(app).is_light() {
                                                    light.clone()
                                                } else {
                                                    dark.clone()
                                                }
                                            }
                                        }
                                    },
                                };
                                settings::ThemeSelection::Static(name)
                            },
                            settings::ThemeSelectionDiscriminants::Dynamic => {
                                let static_name = match settings_value {
                                    settings::ThemeSelection::Static(theme_name) => theme_name.clone(),
                                    settings::ThemeSelection::Dynamic {..} => return,
                                };

                                settings::ThemeSelection::Dynamic {
                                    mode: settings::ThemeAppearanceMode::System,
                                    light: static_name.clone(),
                                    dark: static_name,
                                }
                            },
                        };
                    },
                }),
                metadata: None,
            },
            pick_discriminant: |settings_content| {
                Some(settings_content.theme.theme.as_ref()?.discriminant() as usize)
            },
            fields: dynamic_variants::<settings::ThemeSelection>().into_iter().map(|variant| {
                match variant {
                    settings::ThemeSelectionDiscriminants::Static => vec![
                        SettingItem {
                            files: USER,
                            title: "Theme Name",
                            description: "The name of your selected theme.",
                            field: Box::new(SettingField {
                                organization_override: None,
                                json_path: Some("theme"),
                                pick: |settings_content| {
                                    match settings_content.theme.theme.as_ref() {
                                        Some(settings::ThemeSelection::Static(name)) => Some(name),
                                        _ => None
                                    }
                                },
                                write: |settings_content, value, _| {
                                    let Some(value) = value else {
                                        return;
                                    };
                                    match settings_content
                                        .theme
                                        .theme.get_or_insert_default() {
                                            settings::ThemeSelection::Static(theme_name) => *theme_name = value,
                                            _ => return
                                        }
                                },
                            }),
                            metadata: None,
                        }
                    ],
                    settings::ThemeSelectionDiscriminants::Dynamic => vec![
                        SettingItem {
                            files: USER,
                            title: "Mode",
                            description: "Choose whether to use the selected light or dark theme or to follow your OS appearance configuration.",
                            field: Box::new(SettingField {
                                organization_override: None,
                                json_path: Some("theme.mode"),
                                pick: |settings_content| {
                                    match settings_content.theme.theme.as_ref() {
                                        Some(settings::ThemeSelection::Dynamic { mode, ..}) => Some(mode),
                                        _ => None
                                    }
                                },
                                write: |settings_content, value, _| {
                                    let Some(value) = value else {
                                        return;
                                    };
                                    match settings_content
                                        .theme
                                        .theme.get_or_insert_default() {
                                            settings::ThemeSelection::Dynamic{ mode, ..} => *mode = value,
                                            _ => return
                                        }
                                },
                            }),
                            metadata: None,
                        },
                        SettingItem {
                            files: USER,
                            title: "Light Theme",
                            description: "The theme to use when mode is set to light, or when mode is set to system and it is in light mode.",
                            field: Box::new(SettingField {
                                organization_override: None,
                                json_path: Some("theme.light"),
                                pick: |settings_content| {
                                    match settings_content.theme.theme.as_ref() {
                                        Some(settings::ThemeSelection::Dynamic { light, ..}) => Some(light),
                                        _ => None
                                    }
                                },
                                write: |settings_content, value, _| {
                                    let Some(value) = value else {
                                        return;
                                    };
                                    match settings_content
                                        .theme
                                        .theme.get_or_insert_default() {
                                            settings::ThemeSelection::Dynamic{ light, ..} => *light = value,
                                            _ => return
                                        }
                                },
                            }),
                            metadata: None,
                        },
                        SettingItem {
                            files: USER,
                            title: "Dark Theme",
                            description: "The theme to use when mode is set to dark, or when mode is set to system and it is in dark mode.",
                            field: Box::new(SettingField {
                                organization_override: None,
                                json_path: Some("theme.dark"),
                                pick: |settings_content| {
                                    match settings_content.theme.theme.as_ref() {
                                        Some(settings::ThemeSelection::Dynamic { dark, ..}) => Some(dark),
                                        _ => None
                                    }
                                },
                                write: |settings_content, value, _| {
                                    let Some(value) = value else {
                                        return;
                                    };
                                    match settings_content
                                        .theme
                                        .theme.get_or_insert_default() {
                                            settings::ThemeSelection::Dynamic{ dark, ..} => *dark = value,
                                            _ => return
                                        }
                                },
                            }),
                            metadata: None,
                        }
                    ],
                }
            }).collect(),
        }),
        SettingsPageItem::DynamicItem(DynamicItem {
            discriminant: SettingItem {
                files: USER,
                title: "Icon Theme",
                description: "The custom set of icons Mav will associate with files and directories.",
                field: Box::new(SettingField {
                    organization_override: None,
                    json_path: Some("icon_theme$"),
                    pick: |settings_content| {
                        Some(&dynamic_variants::<settings::IconThemeSelection>()[
                            settings_content
                                .theme
                                .icon_theme
                                .as_ref()?
                                .discriminant() as usize])
                    },
                    write: |settings_content, value, app| {
                        let Some(value) = value else {
                            settings_content.theme.icon_theme = None;
                            return;
                        };
                        let settings_value = settings_content.theme.icon_theme.get_or_insert_with(|| {
                            settings::IconThemeSelection::Static(settings::IconThemeName(theme::default_icon_theme().name.clone().into()))
                        });
                        *settings_value = match value {
                            settings::IconThemeSelectionDiscriminants::Static => {
                                let name = match settings_value {
                                    settings::IconThemeSelection::Static(_) => return,
                                    settings::IconThemeSelection::Dynamic { mode, light, dark } => {
                                        match mode {
                                            theme_settings::ThemeAppearanceMode::Light => light.clone(),
                                            theme_settings::ThemeAppearanceMode::Dark => dark.clone(),
                                            theme_settings::ThemeAppearanceMode::System => {
                                                if SystemAppearance::global(app).is_light() {
                                                    light.clone()
                                                } else {
                                                    dark.clone()
                                                }
                                            }
                                        }
                                    },
                                };
                                settings::IconThemeSelection::Static(name)
                            },
                            settings::IconThemeSelectionDiscriminants::Dynamic => {
                                let static_name = match settings_value {
                                    settings::IconThemeSelection::Static(theme_name) => theme_name.clone(),
                                    settings::IconThemeSelection::Dynamic {..} => return,
                                };

                                settings::IconThemeSelection::Dynamic {
                                    mode: settings::ThemeAppearanceMode::System,
                                    light: static_name.clone(),
                                    dark: static_name,
                                }
                            },
                        };
                    },
                }),
                metadata: None,
            },
            pick_discriminant: |settings_content| {
                Some(settings_content.theme.icon_theme.as_ref()?.discriminant() as usize)
            },
            fields: dynamic_variants::<settings::IconThemeSelection>().into_iter().map(|variant| {
                match variant {
                    settings::IconThemeSelectionDiscriminants::Static => vec![
                        SettingItem {
                            files: USER,
                            title: "Icon Theme Name",
                            description: "The name of your selected icon theme.",
                            field: Box::new(SettingField {
                                organization_override: None,
                                json_path: Some("icon_theme$string"),
                                pick: |settings_content| {
                                    match settings_content.theme.icon_theme.as_ref() {
                                        Some(settings::IconThemeSelection::Static(name)) => Some(name),
                                        _ => None
                                    }
                                },
                                write: |settings_content, value, _| {
                                    let Some(value) = value else {
                                        return;
                                    };
                                    match settings_content
                                        .theme
                                        .icon_theme.as_mut() {
                                            Some(settings::IconThemeSelection::Static(theme_name)) => *theme_name = value,
                                            _ => return
                                        }
                                },
                            }),
                            metadata: None,
                        }
                    ],
                    settings::IconThemeSelectionDiscriminants::Dynamic => vec![
                        SettingItem {
                            files: USER,
                            title: "Mode",
                            description: "Choose whether to use the selected light or dark icon theme or to follow your OS appearance configuration.",
                            field: Box::new(SettingField {
                                organization_override: None,
                                json_path: Some("icon_theme"),
                                pick: |settings_content| {
                                    match settings_content.theme.icon_theme.as_ref() {
                                        Some(settings::IconThemeSelection::Dynamic { mode, ..}) => Some(mode),
                                        _ => None
                                    }
                                },
                                write: |settings_content, value, _| {
                                    let Some(value) = value else {
                                        return;
                                    };
                                    match settings_content
                                        .theme
                                        .icon_theme.as_mut() {
                                            Some(settings::IconThemeSelection::Dynamic{ mode, ..}) => *mode = value,
                                            _ => return
                                        }
                                },
                            }),
                            metadata: None,
                        },
                        SettingItem {
                            files: USER,
                            title: "Light Icon Theme",
                            description: "The icon theme to use when mode is set to light, or when mode is set to system and it is in light mode.",
                            field: Box::new(SettingField {
                                organization_override: None,
                                json_path: Some("icon_theme.light"),
                                pick: |settings_content| {
                                    match settings_content.theme.icon_theme.as_ref() {
                                        Some(settings::IconThemeSelection::Dynamic { light, ..}) => Some(light),
                                        _ => None
                                    }
                                },
                                write: |settings_content, value, _| {
                                    let Some(value) = value else {
                                        return;
                                    };
                                    match settings_content
                                        .theme
                                        .icon_theme.as_mut() {
                                            Some(settings::IconThemeSelection::Dynamic{ light, ..}) => *light = value,
                                            _ => return
                                        }
                                },
                            }),
                            metadata: None,
                        },
                        SettingItem {
                            files: USER,
                            title: "Dark Icon Theme",
                            description: "The icon theme to use when mode is set to dark, or when mode is set to system and it is in dark mode.",
                            field: Box::new(SettingField {
                                organization_override: None,
                                json_path: Some("icon_theme.dark"),
                                pick: |settings_content| {
                                    match settings_content.theme.icon_theme.as_ref() {
                                        Some(settings::IconThemeSelection::Dynamic { dark, ..}) => Some(dark),
                                        _ => None
                                    }
                                },
                                write: |settings_content, value, _| {
                                    let Some(value) = value else {
                                        return;
                                    };
                                    match settings_content
                                        .theme
                                        .icon_theme.as_mut() {
                                            Some(settings::IconThemeSelection::Dynamic{ dark, ..}) => *dark = value,
                                            _ => return
                                        }
                                },
                            }),
                            metadata: None,
                        }
                    ],
                }
            }).collect(),
        }),
    ]
}
