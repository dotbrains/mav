use super::*;

#[test]
fn test_migrate_builtin_agent_servers_to_registry_simple() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry,
        )],
        r#"{
    "agent_servers": {
        "gemini": {
            "default_model": "gemini-2.0-flash"
        },
        "claude": {
            "default_mode": "plan"
        },
        "codex": {
            "default_model": "o4-mini"
        }
    }
}"#,
        Some(
            r#"{
    "agent_servers": {
        "codex-acp": {
            "type": "registry",
            "default_model": "o4-mini"
        },
        "claude-acp": {
            "type": "registry",
            "default_mode": "plan"
        },
        "gemini": {
            "type": "registry",
            "default_model": "gemini-2.0-flash"
        }
    }
}"#,
        ),
    );
}

#[test]
fn test_migrate_builtin_agent_servers_empty_entries() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry,
        )],
        r#"{
    "agent_servers": {
        "gemini": {},
        "claude": {},
        "codex": {}
    }
}"#,
        Some(
            r#"{
    "agent_servers": {
        "codex-acp": {
            "type": "registry"
        },
        "claude-acp": {
            "type": "registry"
        },
        "gemini": {
            "type": "registry"
        }
    }
}"#,
        ),
    );
}

#[test]
fn test_migrate_builtin_agent_servers_with_command() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry,
        )],
        r#"{
    "agent_servers": {
        "claude": {
            "command": "/usr/local/bin/claude",
            "args": ["--verbose"],
            "env": {"CLAUDE_KEY": "abc123"},
            "default_mode": "plan",
            "default_model": "claude-sonnet-4"
        }
    }
}"#,
        Some(
            r#"{
    "agent_servers": {
        "claude-acp-custom": {
            "type": "custom",
            "command": "/usr/local/bin/claude",
            "args": [
                "--verbose"
            ],
            "env": {
                "CLAUDE_KEY": "abc123"
            },
            "default_mode": "plan",
            "default_model": "claude-sonnet-4"
        }
    }
}"#,
        ),
    );
}

#[test]
fn test_migrate_builtin_agent_servers_gemini_with_command() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry,
        )],
        r#"{
    "agent_servers": {
        "gemini": {
            "command": "/opt/gemini/bin/gemini",
            "default_model": "gemini-2.0-flash"
        }
    }
}"#,
        Some(
            r#"{
    "agent_servers": {
        "gemini-custom": {
            "type": "custom",
            "command": "/opt/gemini/bin/gemini",
            "default_model": "gemini-2.0-flash"
        }
    }
}"#,
        ),
    );
}

#[test]
fn test_migrate_builtin_agent_servers_gemini_ignore_system_version_false() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry,
        )],
        r#"{
    "agent_servers": {
        "gemini": {
            "ignore_system_version": false,
            "default_model": "gemini-2.0-flash"
        }
    }
}"#,
        Some(
            r#"{
    "agent_servers": {
        "gemini-custom": {
            "type": "custom",
            "command": "gemini",
            "default_model": "gemini-2.0-flash"
        }
    }
}"#,
        ),
    );
}

#[test]
fn test_migrate_builtin_agent_servers_gemini_ignore_system_version_true() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry,
        )],
        r#"{
    "agent_servers": {
        "gemini": {
            "ignore_system_version": true,
            "default_model": "gemini-2.0-flash"
        }
    }
}"#,
        Some(
            r#"{
    "agent_servers": {
        "gemini": {
            "type": "registry",
            "default_model": "gemini-2.0-flash"
        }
    }
}"#,
        ),
    );
}

#[test]
fn test_migrate_builtin_agent_servers_already_typed_unchanged() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry,
        )],
        r#"{
    "agent_servers": {
        "gemini": {
            "type": "registry",
            "default_model": "gemini-2.0-flash"
        },
        "claude-acp": {
            "type": "registry",
            "default_mode": "plan"
        }
    }
}"#,
        None,
    );
}

#[test]
fn test_migrate_builtin_agent_servers_preserves_custom_entries() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry,
        )],
        r#"{
    "agent_servers": {
        "claude": {
            "default_mode": "plan"
        },
        "my-custom-agent": {
            "type": "custom",
            "command": "/path/to/agent"
        }
    }
}"#,
        Some(
            r#"{
    "agent_servers": {
        "claude-acp": {
            "type": "registry",
            "default_mode": "plan"
        },
        "my-custom-agent": {
            "type": "custom",
            "command": "/path/to/agent"
        }
    }
}"#,
        ),
    );
}

#[test]
fn test_migrate_builtin_agent_servers_target_already_exists() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry,
        )],
        r#"{
    "agent_servers": {
        "claude": {
            "default_mode": "plan"
        },
        "claude-acp": {
            "type": "registry",
            "default_model": "claude-sonnet-4"
        }
    }
}"#,
        Some(
            r#"{
    "agent_servers": {
        "claude-acp": {
            "type": "registry",
            "default_model": "claude-sonnet-4"
        }
    }
}"#,
        ),
    );
}

#[test]
fn test_migrate_builtin_agent_servers_no_agent_servers_key() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry,
        )],
        r#"{
    "agent": {
        "enabled": true
    }
}"#,
        None,
    );
}

#[test]
fn test_migrate_builtin_agent_servers_all_fields() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry,
        )],
        r#"{
    "agent_servers": {
        "codex": {
            "env": {"OPENAI_API_KEY": "sk-123"},
            "default_mode": "read-only",
            "default_model": "o4-mini",
            "favorite_models": ["o4-mini", "codex-mini-latest"],
            "default_config_options": {"approval_mode": "auto-edit"},
            "favorite_config_option_values": {"approval_mode": ["auto-edit", "suggest"]}
        }
    }
}"#,
        Some(
            r#"{
    "agent_servers": {
        "codex-acp": {
            "type": "registry",
            "env": {
                "OPENAI_API_KEY": "sk-123"
            },
            "default_mode": "read-only",
            "default_model": "o4-mini",
            "favorite_models": [
                "o4-mini",
                "codex-mini-latest"
            ],
            "default_config_options": {
                "approval_mode": "auto-edit"
            },
            "favorite_config_option_values": {
                "approval_mode": [
                    "auto-edit",
                    "suggest"
                ]
            }
        }
    }
}"#,
        ),
    );
}

#[test]
fn test_migrate_builtin_agent_servers_codex_with_command() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry,
        )],
        r#"{
    "agent_servers": {
        "codex": {
            "command": "/usr/local/bin/codex",
            "args": ["--full-auto"],
            "default_model": "o4-mini"
        }
    }
}"#,
        Some(
            r#"{
    "agent_servers": {
        "codex-acp-custom": {
            "type": "custom",
            "command": "/usr/local/bin/codex",
            "args": [
                "--full-auto"
            ],
            "default_model": "o4-mini"
        }
    }
}"#,
        ),
    );
}

#[test]
fn test_migrate_builtin_agent_servers_mixed_migrated_and_not() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry,
        )],
        r#"{
    "agent_servers": {
        "gemini": {
            "type": "registry",
            "default_model": "gemini-2.0-flash"
        },
        "claude": {
            "default_mode": "plan"
        },
        "codex": {}
    }
}"#,
        Some(
            r#"{
    "agent_servers": {
        "codex-acp": {
            "type": "registry"
        },
        "claude-acp": {
            "type": "registry",
            "default_mode": "plan"
        },
        "gemini": {
            "type": "registry",
            "default_model": "gemini-2.0-flash"
        }
    }
}"#,
        ),
    );
}
