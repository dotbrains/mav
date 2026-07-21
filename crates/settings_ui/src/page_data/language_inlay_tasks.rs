use super::*;

pub(super) fn inlay_hints_section() -> [SettingsPageItem; 10] {
    [
        SettingsPageItem::SectionHeader("Inlay Hints"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Enabled",
            description: "Global switch to toggle hints on and off.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).inlay_hints.enabled"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.inlay_hints.as_ref()?.enabled.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.inlay_hints.get_or_insert_default().enabled = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Value Hints",
            description: "Global switch to toggle inline values on and off when debugging.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).inlay_hints.show_value_hints"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.inlay_hints.as_ref()?.show_value_hints.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language
                            .inlay_hints
                            .get_or_insert_default()
                            .show_value_hints = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Type Hints",
            description: "Whether type hints should be shown.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).inlay_hints.show_type_hints"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.inlay_hints.as_ref()?.show_type_hints.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.inlay_hints.get_or_insert_default().show_type_hints = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Parameter Hints",
            description: "Whether parameter hints should be shown.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).inlay_hints.show_parameter_hints"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.inlay_hints.as_ref()?.show_parameter_hints.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language
                            .inlay_hints
                            .get_or_insert_default()
                            .show_parameter_hints = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Other Hints",
            description: "Whether other hints should be shown.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).inlay_hints.show_other_hints"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.inlay_hints.as_ref()?.show_other_hints.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language
                            .inlay_hints
                            .get_or_insert_default()
                            .show_other_hints = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Show Background",
            description: "Show a background for inlay hints.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).inlay_hints.show_background"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.inlay_hints.as_ref()?.show_background.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.inlay_hints.get_or_insert_default().show_background = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Edit Debounce Ms",
            description: "Whether or not to debounce inlay hints updates after buffer edits (set to 0 to disable debouncing).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).inlay_hints.edit_debounce_ms"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.inlay_hints.as_ref()?.edit_debounce_ms.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language
                            .inlay_hints
                            .get_or_insert_default()
                            .edit_debounce_ms = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Scroll Debounce Ms",
            description: "Whether or not to debounce inlay hints updates after buffer scrolls (set to 0 to disable debouncing).",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).inlay_hints.scroll_debounce_ms"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.inlay_hints.as_ref()?.scroll_debounce_ms.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language
                            .inlay_hints
                            .get_or_insert_default()
                            .scroll_debounce_ms = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Toggle On Modifiers Press",
            description: "Toggles inlay hints (hides or shows) when the user presses the modifiers specified.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).inlay_hints.toggle_on_modifiers_press"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language
                                .inlay_hints
                                .as_ref()?
                                .toggle_on_modifiers_press
                                .as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language
                                .inlay_hints
                                .get_or_insert_default()
                                .toggle_on_modifiers_press = value;
                        })
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER | PROJECT,
        }),
    ]
}

pub(super) fn tasks_section() -> [SettingsPageItem; 4] {
    [
        SettingsPageItem::SectionHeader("Tasks"),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Enabled",
            description: "Whether tasks are enabled for this language.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).tasks.enabled"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.tasks.as_ref()?.enabled.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.tasks.get_or_insert_default().enabled = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Variables",
            description: "Extra task variables to set for a particular language.",
            field: Box::new(
                SettingField {
                    organization_override: None,
                    json_path: Some("languages.$(language).tasks.variables"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.tasks.as_ref()?.variables.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.tasks.get_or_insert_default().variables = value;
                        })
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Prefer LSP",
            description: "Use LSP tasks over Mav language extension tasks.",
            field: Box::new(SettingField {
                organization_override: None,
                json_path: Some("languages.$(language).tasks.prefer_lsp"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.tasks.as_ref()?.prefer_lsp.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.tasks.get_or_insert_default().prefer_lsp = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
    ]
}
