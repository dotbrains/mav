use super::*;

#[test]
fn test_migrate_tool_permission_defaults_core() {
    // No agent settings - no change
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"{ }"#.unindent(),
        None,
    );

    // always_allow_tool_actions: true -> tool_permissions.default: "allow"
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "always_allow_tool_actions": true
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "agent": {
                        "tool_permissions": {
                            "default": "allow"
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // always_allow_tool_actions: false -> just remove it
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "always_allow_tool_actions": false
                }
            }
            "#
        .unindent(),
        Some(
            // The blank line has spaces because the migration preserves the original indentation
            "{\n    \"agent\": {\n        \n    }\n}\n",
        ),
    );

    // Preserve existing tool_permissions.tools when migrating
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "always_allow_tool_actions": true,
                    "tool_permissions": {
                        "tools": {
                            "terminal": {
                                "always_deny": [{ "pattern": "rm\\s+-rf" }]
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
                            "default": "allow",
                            "tools": {
                                "terminal": {
                                    "always_deny": [{ "pattern": "rm\\s+-rf" }]
                                }
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Don't override existing default (and migrate default_mode to default)
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "always_allow_tool_actions": true,
                    "tool_permissions": {
                        "default_mode": "confirm"
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
                            "default": "confirm"
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Migrate existing default_mode to default (no always_allow_tool_actions)
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "tool_permissions": {
                        "default_mode": "allow"
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
                            "default": "allow"
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // No migration needed if already using new format with "default"
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "tool_permissions": {
                        "default": "allow"
                    }
                }
            }
            "#
        .unindent(),
        None,
    );

    // Migrate default_mode to default in tool-specific rules
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "tool_permissions": {
                        "default_mode": "confirm",
                        "tools": {
                            "terminal": {
                                "default_mode": "allow"
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
                            "default": "confirm",
                            "tools": {
                                "terminal": {
                                    "default": "allow"
                                }
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // When tool_permissions is null, replace it so always_allow is preserved
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "always_allow_tool_actions": true,
                    "tool_permissions": null
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "agent": {
                        "tool_permissions": {
                            "default": "allow"
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Platform-specific agent migration
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "linux": {
                    "agent": {
                        "always_allow_tool_actions": true
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
                                "default": "allow"
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Channel-specific agent migration
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "always_allow_tool_actions": true
                },
                "nightly": {
                    "agent": {
                        "tool_permissions": {
                            "default_mode": "confirm"
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
                            "default": "allow"
                        }
                    },
                    "nightly": {
                        "agent": {
                            "tool_permissions": {
                                "default": "confirm"
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );
}
