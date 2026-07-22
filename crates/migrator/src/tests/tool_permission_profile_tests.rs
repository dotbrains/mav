use super::*;

#[test]
fn test_migrate_tool_permission_defaults_profiles() {
    // Profile-level migration
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "profiles": {
                        "custom": {
                            "always_allow_tool_actions": true,
                            "tool_permissions": {
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
                        "profiles": {
                            "custom": {
                                "tool_permissions": {
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

    // Platform-specific agent with profiles
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "macos": {
                    "agent": {
                        "always_allow_tool_actions": true,
                        "profiles": {
                            "strict": {
                                "tool_permissions": {
                                    "default_mode": "deny"
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
                    "macos": {
                        "agent": {
                            "tool_permissions": {
                                "default": "allow"
                            },
                            "profiles": {
                                "strict": {
                                    "tool_permissions": {
                                        "default": "deny"
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

    // Root-level profile with always_allow_tool_actions
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "profiles": {
                    "work": {
                        "agent": {
                            "always_allow_tool_actions": true
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
                        "work": {
                            "agent": {
                                "tool_permissions": {
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

    // Root-level profile with default_mode
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "profiles": {
                    "work": {
                        "agent": {
                            "tool_permissions": {
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
                    "profiles": {
                        "work": {
                            "agent": {
                                "tool_permissions": {
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

    // Root-level profile + root-level agent both migrated
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "always_allow_tool_actions": true
                },
                "profiles": {
                    "strict": {
                        "agent": {
                            "tool_permissions": {
                                "default_mode": "deny"
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
                            "default": "allow"
                        }
                    },
                    "profiles": {
                        "strict": {
                            "agent": {
                                "tool_permissions": {
                                    "default": "deny"
                                }
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Non-boolean always_allow_tool_actions (string "true") is left in place
    // so the schema validator can report it, rather than silently dropping user data.
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "always_allow_tool_actions": "true"
                }
            }
            "#
        .unindent(),
        None,
    );

    // null always_allow_tool_actions is removed (treated as false)
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "always_allow_tool_actions": null
                }
            }
            "#
        .unindent(),
        Some(&"{\n    \"agent\": {\n        \n    }\n}\n"),
    );

    // Project-local settings (.mav/settings.json) with always_allow_tool_actions
    // These files have no platform/channel overrides or root-level profiles.
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
                                "default_mode": "confirm",
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
                                    "default": "confirm",
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

    // Project-local settings with only default_mode (no always_allow_tool_actions)
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "tool_permissions": {
                        "default_mode": "deny"
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
                            "default": "deny"
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Project-local settings with no agent section at all - no change
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "tab_size": 4,
                "format_on_save": "on"
            }
            "#
        .unindent(),
        None,
    );

    // Existing agent_servers are left untouched
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "always_allow_tool_actions": true
                },
                "agent_servers": {
                    "claude": {
                        "default_mode": "plan"
                    },
                    "codex": {
                        "default_mode": "read-only"
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
                    "agent_servers": {
                        "claude": {
                            "default_mode": "plan"
                        },
                        "codex": {
                            "default_mode": "read-only"
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Existing agent_servers are left untouched even with partial entries
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "always_allow_tool_actions": true
                },
                "agent_servers": {
                    "claude": {
                        "default_mode": "plan"
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
                    "agent_servers": {
                        "claude": {
                            "default_mode": "plan"
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // always_allow_tool_actions: false leaves agent_servers untouched
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_04::migrate_tool_permission_defaults,
        )],
        &r#"
            {
                "agent": {
                    "always_allow_tool_actions": false
                },
                "agent_servers": {
                    "claude": {}
                }
            }
            "#
        .unindent(),
        Some(
            "{\n    \"agent\": {\n        \n    },\n    \"agent_servers\": {\n        \"claude\": {}\n    }\n}\n",
        ),
    );
}
