use super::*;

#[test]
fn test_mcp_settings_migration() {
    assert_migrate_with_migrations(
        &[MigrationType::TreeSitter(
            migrations::m_2025_06_16::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_06_16,
        )],
        r#"{
    "context_servers": {
        "empty_server": {},
        "extension_server": {
            "settings": {
                "foo": "bar"
            }
        },
        "custom_server": {
            "command": {
                "path": "foo",
                "args": ["bar"],
                "env": {
                    "FOO": "BAR"
                }
            }
        },
        "invalid_server": {
            "command": {
                "path": "foo",
                "args": ["bar"],
                "env": {
                    "FOO": "BAR"
                }
            },
            "settings": {
                "foo": "bar"
            }
        },
        "empty_server2": {},
        "extension_server2": {
            "foo": "bar",
            "settings": {
                "foo": "bar"
            },
            "bar": "foo"
        },
        "custom_server2": {
            "foo": "bar",
            "command": {
                "path": "foo",
                "args": ["bar"],
                "env": {
                    "FOO": "BAR"
                }
            },
            "bar": "foo"
        },
        "invalid_server2": {
            "foo": "bar",
            "command": {
                "path": "foo",
                "args": ["bar"],
                "env": {
                    "FOO": "BAR"
                }
            },
            "bar": "foo",
            "settings": {
                "foo": "bar"
            }
        }
    }
}"#,
        Some(
            r#"{
    "context_servers": {
        "empty_server": {
            "source": "extension",
            "settings": {}
        },
        "extension_server": {
            "source": "extension",
            "settings": {
                "foo": "bar"
            }
        },
        "custom_server": {
            "source": "custom",
            "command": {
                "path": "foo",
                "args": ["bar"],
                "env": {
                    "FOO": "BAR"
                }
            }
        },
        "invalid_server": {
            "source": "custom",
            "command": {
                "path": "foo",
                "args": ["bar"],
                "env": {
                    "FOO": "BAR"
                }
            },
            "settings": {
                "foo": "bar"
            }
        },
        "empty_server2": {
            "source": "extension",
            "settings": {}
        },
        "extension_server2": {
            "source": "extension",
            "foo": "bar",
            "settings": {
                "foo": "bar"
            },
            "bar": "foo"
        },
        "custom_server2": {
            "source": "custom",
            "foo": "bar",
            "command": {
                "path": "foo",
                "args": ["bar"],
                "env": {
                    "FOO": "BAR"
                }
            },
            "bar": "foo"
        },
        "invalid_server2": {
            "source": "custom",
            "foo": "bar",
            "command": {
                "path": "foo",
                "args": ["bar"],
                "env": {
                    "FOO": "BAR"
                }
            },
            "bar": "foo",
            "settings": {
                "foo": "bar"
            }
        }
    }
}"#,
        ),
    );
}

#[test]
fn test_mcp_settings_migration_doesnt_change_valid_settings() {
    let settings = r#"{
    "context_servers": {
        "empty_server": {
            "source": "extension",
            "settings": {}
        },
        "extension_server": {
            "source": "extension",
            "settings": {
                "foo": "bar"
            }
        },
        "custom_server": {
            "source": "custom",
            "command": {
                "path": "foo",
                "args": ["bar"],
                "env": {
                    "FOO": "BAR"
                }
            }
        },
        "invalid_server": {
            "source": "custom",
            "command": {
                "path": "foo",
                "args": ["bar"],
                "env": {
                    "FOO": "BAR"
                }
            },
            "settings": {
                "foo": "bar"
            }
        }
    }
}"#;
    assert_migrate_with_migrations(
        &[MigrationType::TreeSitter(
            migrations::m_2025_06_16::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_06_16,
        )],
        settings,
        None,
    );
}

#[test]
fn test_custom_agent_server_settings_migration() {
    assert_migrate_with_migrations(
        &[MigrationType::TreeSitter(
            migrations::m_2025_11_20::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_11_20,
        )],
        r#"{
    "agent_servers": {
        "gemini": {
            "default_model": "gemini-1.5-pro"
        },
        "claude": {},
        "codex": {},
        "my-custom-agent": {
            "command": "/path/to/agent",
            "args": ["--foo"],
            "default_model": "my-model"
        },
        "already-migrated-agent": {
            "type": "custom",
            "command": "/path/to/agent"
        },
        "future-extension-agent": {
            "type": "extension",
            "default_model": "ext-model"
        }
    }
}"#,
        Some(
            r#"{
    "agent_servers": {
        "gemini": {
            "default_model": "gemini-1.5-pro"
        },
        "claude": {},
        "codex": {},
        "my-custom-agent": {
            "type": "custom",
            "command": "/path/to/agent",
            "args": ["--foo"],
            "default_model": "my-model"
        },
        "already-migrated-agent": {
            "type": "custom",
            "command": "/path/to/agent"
        },
        "future-extension-agent": {
            "type": "extension",
            "default_model": "ext-model"
        }
    }
}"#,
        ),
    );
}

#[test]
fn test_remove_version_fields() {
    assert_migrate_settings(
        r#"{
    "language_models": {
        "anthropic": {
            "version": "1",
            "api_url": "https://api.anthropic.com"
        },
        "openai": {
            "version": "1",
            "api_url": "https://api.openai.com/v1"
        }
    },
    "agent": {
        "version": "2",
        "enabled": true,
        "button": true,
        "dock": "right",
        "default_width": 640,
        "default_height": 320,
        "default_model": {
            "provider": "mav.dev",
            "model": "claude-sonnet-4"
        }
    }
}"#,
        Some(
            r#"{
    "language_models": {
        "anthropic": {
            "api_url": "https://api.anthropic.com"
        },
        "openai": {
            "api_url": "https://api.openai.com/v1"
        }
    },
    "agent": {
        "enabled": true,
        "button": true,
        "dock": "right",
        "default_width": 640,
        "default_height": 320,
        "default_model": {
            "provider": "mav.dev",
            "model": "claude-sonnet-4"
        }
    }
}"#,
        ),
    );

    // Test that version fields in other contexts are not removed
    assert_migrate_settings(
        r#"{
    "language_models": {
        "other_provider": {
            "version": "1",
            "api_url": "https://api.example.com"
        }
    },
    "other_section": {
        "version": "1"
    }
}"#,
        None,
    );
}

#[test]
fn test_flatten_context_server_command() {
    assert_migrate_settings(
        r#"{
    "context_servers": {
        "some-mcp-server": {
            "command": {
                "path": "npx",
                "args": [
                    "-y",
                    "@supabase/mcp-server-supabase@latest",
                    "--read-only",
                    "--project-ref=<project-ref>"
                ],
                "env": {
                    "SUPABASE_ACCESS_TOKEN": "<personal-access-token>"
                }
            }
        }
    }
}"#,
        Some(
            r#"{
    "context_servers": {
        "some-mcp-server": {
            "command": "npx",
            "args": [
                "-y",
                "@supabase/mcp-server-supabase@latest",
                "--read-only",
                "--project-ref=<project-ref>"
            ],
            "env": {
                "SUPABASE_ACCESS_TOKEN": "<personal-access-token>"
            }
        }
    }
}"#,
        ),
    );

    // Test with additional keys in server object
    assert_migrate_settings(
        r#"{
    "context_servers": {
        "server-with-extras": {
            "command": {
                "path": "/usr/bin/node",
                "args": ["server.js"]
            },
            "settings": {}
        }
    }
}"#,
        Some(
            r#"{
    "context_servers": {
        "server-with-extras": {
            "command": "/usr/bin/node",
            "args": ["server.js"],
            "settings": {}
        }
    }
}"#,
        ),
    );

    // Test command without args or env
    assert_migrate_settings(
        r#"{
    "context_servers": {
        "simple-server": {
            "command": {
                "path": "simple-mcp-server"
            }
        }
    }
}"#,
        Some(
            r#"{
    "context_servers": {
        "simple-server": {
            "command": "simple-mcp-server"
        }
    }
}"#,
        ),
    );
}
