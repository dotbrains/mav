use super::*;

#[test]
fn test_rename_web_search_to_search_web_in_tool_permissions() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_04_10::rename_web_search_to_search_web,
        )],
        &r#"
            {
                "agent": {
                    "tool_permissions": {
                        "tools": {
                            "web_search": {
                                "allow": true
                            }
                        }
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "agent": {
                        "tool_permissions": {
                            "tools": {
                                "search_web": {
                                    "allow": true
                                }
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );
}

#[test]
fn test_rename_web_search_to_search_web_in_profiles() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_04_10::rename_web_search_to_search_web,
        )],
        &r#"
            {
                "agent": {
                    "profiles": {
                        "write": {
                            "tools": {
                                "web_search": false
                            }
                        }
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "agent": {
                        "profiles": {
                            "write": {
                                "tools": {
                                    "search_web": false
                                }
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );
}

#[test]
fn test_rename_web_search_to_search_web_no_change_when_already_migrated() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_04_10::rename_web_search_to_search_web,
        )],
        &r#"
            {
                "agent": {
                    "tool_permissions": {
                        "tools": {
                            "search_web": {
                                "allow": true
                            }
                        }
                    }
                }
            }
            "#
        .unindent(),
        None,
    );
}

#[test]
fn test_rename_web_search_to_search_web_no_clobber() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_04_10::rename_web_search_to_search_web,
        )],
        &r#"
            {
                "agent": {
                    "tool_permissions": {
                        "tools": {
                            "web_search": {
                                "allow": false
                            },
                            "search_web": {
                                "allow": true
                            }
                        }
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "agent": {
                        "tool_permissions": {
                            "tools": {
                                "search_web": {
                                    "allow": false
                                }
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );
}

#[test]
fn test_rename_web_search_to_search_web_platform_override() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_04_10::rename_web_search_to_search_web,
        )],
        &r#"
            {
                "linux": {
                    "agent": {
                        "tool_permissions": {
                            "tools": {
                                "web_search": {
                                    "allow": true
                                }
                            }
                        }
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "linux": {
                        "agent": {
                            "tool_permissions": {
                                "tools": {
                                    "search_web": {
                                        "allow": true
                                    }
                                }
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );
}

#[test]
fn test_rename_web_search_to_search_web_release_channel_override() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_04_10::rename_web_search_to_search_web,
        )],
        &r#"
            {
                "nightly": {
                    "agent": {
                        "tool_permissions": {
                            "tools": {
                                "web_search": {
                                    "default": "allow"
                                }
                            }
                        }
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "nightly": {
                        "agent": {
                            "tool_permissions": {
                                "tools": {
                                    "search_web": {
                                        "default": "allow"
                                    }
                                }
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );
}

#[test]
fn test_rename_web_search_to_search_web_no_agent() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_04_10::rename_web_search_to_search_web,
        )],
        &r#"
            {
                "buffer_font_size": 14
            }
            "#
        .unindent(),
        None,
    );
}

#[test]
fn test_migration_helpers_handle_various_profile_forms() {
    let setting = "a_setting";
    let old_value = "old_value";
    let new_value = "new_value";

    fn language_setting_fn(value: &mut serde_json::Value, _: &[&str]) -> anyhow::Result<()> {
        if let Some(obj) = value.as_object_mut() {
            if let Some(v) = obj.get_mut("a_setting") {
                *v = serde_json::json!("new_value");
            }
        }
        Ok(())
    }

    let mut settings_fn = |map: &mut serde_json::Map<String, serde_json::Value>| {
        if let Some(v) = map.get_mut(setting) {
            *v = serde_json::json!(new_value);
        }
        Ok(())
    };

    // Legacy form
    let input = serde_json::json!({
        "profiles": {
            "work": {
                setting: old_value
            }
        }
    });
    let expected = serde_json::json!({
        "profiles": {
            "work": {
                setting: new_value
            }
        }
    });

    let mut value = input.clone();
    migrations::migrate_settings(&mut value, &mut settings_fn).unwrap();
    assert_eq!(value, expected);

    let mut value = input;
    migrations::migrate_language_setting(&mut value, language_setting_fn).unwrap();
    assert_eq!(value, expected);

    // Form after migration: `m_2026_04_01`
    let input = serde_json::json!({
        "profiles": {
            "work": {
                "settings": {
                    setting: old_value
                }
            }
        }
    });
    let expected = serde_json::json!({
        "profiles": {
            "work": {
                "settings": {
                    setting: new_value
                }
            }
        }
    });

    let mut value = input.clone();
    migrations::migrate_settings(&mut value, &mut settings_fn).unwrap();
    assert_eq!(value, expected);

    let mut value = input;
    migrations::migrate_language_setting(&mut value, language_setting_fn).unwrap();
    assert_eq!(value, expected);

    // Base-only form after migration: `m_2026_04_01` (no settings to migrate)
    let input = serde_json::json!({
        "profiles": {
            "work": {
                "base": "default"
            }
        }
    });

    let mut value = input.clone();
    migrations::migrate_settings(&mut value, &mut settings_fn).unwrap();
    assert_eq!(value, input);

    let mut value = input.clone();
    migrations::migrate_language_setting(&mut value, language_setting_fn).unwrap();
    assert_eq!(value, input);
}

#[test]
fn test_rename_web_search_to_search_web_root_level_profile() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_04_10::rename_web_search_to_search_web,
        )],
        &r#"
            {
                "profiles": {
                    "Work": {
                        "settings": {
                            "agent": {
                                "tool_permissions": {
                                    "tools": {
                                        "web_search": {
                                            "default": "allow"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "profiles": {
                        "Work": {
                            "settings": {
                                "agent": {
                                    "tool_permissions": {
                                        "tools": {
                                            "search_web": {
                                                "default": "allow"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );
}
