use super::*;
use unindent::Unindent as _;

#[track_caller]
fn assert_migrated_correctly(migrated: Option<String>, expected: Option<&str>) {
    match (&migrated, &expected) {
        (Some(migrated), Some(expected)) => {
            pretty_assertions::assert_str_eq!(expected, migrated);
        }
        _ => {
            pretty_assertions::assert_eq!(migrated.as_deref(), expected);
        }
    }
}

#[track_caller]
fn assert_migrate_keymap(input: &str, output: Option<&str>) {
    let migrated = migrate_keymap(input).unwrap();
    pretty_assertions::assert_eq!(migrated.as_deref(), output);
}

#[track_caller]
fn assert_migrate_settings(input: &str, output: Option<&str>) {
    let migrated = migrate_settings(input).unwrap();
    assert_migrated_correctly(migrated.clone(), output);

    // expect that rerunning the migration does not result in another migration
    if let Some(migrated) = migrated {
        let rerun = migrate_settings(&migrated).unwrap();
        assert_migrated_correctly(rerun, None);
    }
}

#[track_caller]
fn assert_migrate_with_migrations(migrations: &[MigrationType], input: &str, output: Option<&str>) {
    let migrated = run_migrations(input, migrations).unwrap();
    assert_migrated_correctly(migrated.clone(), output);

    // expect that rerunning the migration does not result in another migration
    if let Some(migrated) = migrated {
        let rerun = run_migrations(&migrated, migrations).unwrap();
        assert_migrated_correctly(rerun, None);
    }
}

#[test]
fn test_empty_content() {
    assert_migrate_settings("", None)
}

#[test]
fn test_replace_array_with_single_string() {
    assert_migrate_keymap(
        r#"
            [
                {
                    "bindings": {
                        "cmd-1": ["workspace::ActivatePaneInDirection", "Up"]
                    }
                }
            ]
            "#,
        Some(
            r#"
            [
                {
                    "bindings": {
                        "cmd-1": "workspace::ActivatePaneUp"
                    }
                }
            ]
            "#,
        ),
    )
}

#[test]
fn test_replace_action_argument_object_with_single_value() {
    assert_migrate_keymap(
        r#"
            [
                {
                    "bindings": {
                        "cmd-1": ["editor::FoldAtLevel", { "level": 1 }]
                    }
                }
            ]
            "#,
        Some(
            r#"
            [
                {
                    "bindings": {
                        "cmd-1": ["editor::FoldAtLevel", 1]
                    }
                }
            ]
            "#,
        ),
    )
}

#[test]
fn test_replace_action_argument_object_with_single_value_2() {
    assert_migrate_keymap(
        r#"
            [
                {
                    "bindings": {
                        "cmd-1": ["vim::PushOperator", { "Object": { "some" : "value" } }]
                    }
                }
            ]
            "#,
        Some(
            r#"
            [
                {
                    "bindings": {
                        "cmd-1": ["vim::PushObject", { "some" : "value" }]
                    }
                }
            ]
            "#,
        ),
    )
}

#[test]
fn test_rename_string_action() {
    assert_migrate_keymap(
        r#"
                [
                    {
                        "bindings": {
                            "cmd-1": "inline_completion::ToggleMenu"
                        }
                    }
                ]
            "#,
        Some(
            r#"
                [
                    {
                        "bindings": {
                            "cmd-1": "edit_prediction::ToggleMenu"
                        }
                    }
                ]
            "#,
        ),
    )
}

#[test]
fn test_rename_context_key() {
    assert_migrate_keymap(
        r#"
                [
                    {
                        "context": "Editor && inline_completion && !showing_completions"
                    }
                ]
            "#,
        Some(
            r#"
                [
                    {
                        "context": "Editor && edit_prediction && !showing_completions"
                    }
                ]
            "#,
        ),
    )
}

#[test]
fn test_incremental_migrations() {
    // Here string transforms to array internally. Then, that array transforms back to string.
    assert_migrate_keymap(
        r#"
                [
                    {
                        "bindings": {
                            "ctrl-q": "editor::GoToHunk", // should remain same
                            "ctrl-w": "editor::GoToPrevHunk", // should rename
                            "ctrl-q": ["editor::GoToHunk", { "center_cursor": true }], // should transform
                            "ctrl-w": ["editor::GoToPreviousHunk", { "center_cursor": true }] // should transform
                        }
                    }
                ]
            "#,
        Some(
            r#"
                [
                    {
                        "bindings": {
                            "ctrl-q": "editor::GoToHunk", // should remain same
                            "ctrl-w": "editor::GoToPreviousHunk", // should rename
                            "ctrl-q": "editor::GoToHunk", // should transform
                            "ctrl-w": "editor::GoToPreviousHunk" // should transform
                        }
                    }
                ]
            "#,
        ),
    )
}

#[test]
fn test_action_argument_snake_case() {
    // First performs transformations, then replacements
    assert_migrate_keymap(
        r#"
            [
                {
                    "bindings": {
                        "cmd-1": ["vim::PushOperator", { "Object": { "around": false } }],
                        "cmd-3": ["pane::CloseActiveItem", { "saveIntent": "saveAll" }],
                        "cmd-2": ["vim::NextWordStart", { "ignorePunctuation": true }],
                        "cmd-4": ["task::Spawn", { "task_name": "a b" }] // should remain as it is
                    }
                }
            ]
            "#,
        Some(
            r#"
            [
                {
                    "bindings": {
                        "cmd-1": ["vim::PushObject", { "around": false }],
                        "cmd-3": ["pane::CloseActiveItem", { "save_intent": "save_all" }],
                        "cmd-2": ["vim::NextWordStart", { "ignore_punctuation": true }],
                        "cmd-4": ["task::Spawn", { "task_name": "a b" }] // should remain as it is
                    }
                }
            ]
            "#,
        ),
    )
}

#[test]
fn test_replace_setting_name() {
    assert_migrate_settings(
        r#"
                {
                    "show_inline_completions_in_menu": true,
                    "show_inline_completions": true,
                    "inline_completions_disabled_in": ["string"],
                    "inline_completions": { "some" : "value" }
                }
            "#,
        Some(
            r#"
                {
                    "show_edit_predictions_in_menu": true,
                    "show_edit_predictions": true,
                    "edit_predictions_disabled_in": ["string"],
                    "edit_predictions": { "some" : "value" }
                }
            "#,
        ),
    )
}

#[test]
fn test_nested_string_replace_for_settings() {
    assert_migrate_settings(
        &r#"
            {
                "features": {
                    "inline_completion_provider": "mav"
                },
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "edit_predictions": {
                        "provider": "mav"
                    }
                }
                "#
            .unindent(),
        ),
    )
}

#[test]
fn test_replace_settings_in_languages() {
    assert_migrate_settings(
        r#"
                {
                    "languages": {
                        "Astro": {
                            "show_inline_completions": true
                        }
                    }
                }
            "#,
        Some(
            r#"
                {
                    "languages": {
                        "Astro": {
                            "show_edit_predictions": true
                        }
                    }
                }
            "#,
        ),
    )
}

#[test]
fn test_replace_settings_value() {
    assert_migrate_settings(
        r#"
                {
                    "scrollbar": {
                        "diagnostics": true
                    },
                    "chat_panel": {
                        "button": true
                    }
                }
            "#,
        Some(
            r#"
                {
                    "scrollbar": {
                        "diagnostics": "all"
                    },
                    "chat_panel": {
                        "button": "always"
                    }
                }
            "#,
        ),
    )
}

#[test]
fn test_replace_settings_name_and_value() {
    assert_migrate_settings(
        r#"
                {
                    "tabs": {
                        "always_show_close_button": true
                    }
                }
            "#,
        Some(
            r#"
                {
                    "tabs": {
                        "show_close_button": "always"
                    }
                }
            "#,
        ),
    )
}

#[test]
fn test_replace_bash_with_terminal_in_profiles() {
    assert_migrate_settings(
        r#"
                {
                    "assistant": {
                        "profiles": {
                            "custom": {
                                "name": "Custom",
                                "tools": {
                                    "bash": true,
                                    "diagnostics": true
                                }
                            }
                        }
                    }
                }
            "#,
        Some(
            r#"
                {
                    "agent": {
                        "profiles": {
                            "custom": {
                                "name": "Custom",
                                "tools": {
                                    "terminal": true,
                                    "diagnostics": true
                                }
                            }
                        }
                    }
                }
            "#,
        ),
    )
}

#[test]
fn test_replace_bash_false_with_terminal_in_profiles() {
    assert_migrate_settings(
        r#"
                {
                    "assistant": {
                        "profiles": {
                            "custom": {
                                "name": "Custom",
                                "tools": {
                                    "bash": false,
                                    "diagnostics": true
                                }
                            }
                        }
                    }
                }
            "#,
        Some(
            r#"
                {
                    "agent": {
                        "profiles": {
                            "custom": {
                                "name": "Custom",
                                "tools": {
                                    "terminal": false,
                                    "diagnostics": true
                                }
                            }
                        }
                    }
                }
            "#,
        ),
    )
}

#[test]
fn test_no_bash_in_profiles() {
    assert_migrate_settings(
        r#"
                {
                    "assistant": {
                        "profiles": {
                            "custom": {
                                "name": "Custom",
                                "tools": {
                                    "diagnostics": true,
                                    "find_path": true,
                                    "read_file": true
                                }
                            }
                        }
                    }
                }
            "#,
        Some(
            r#"
                {
                    "agent": {
                        "profiles": {
                            "custom": {
                                "name": "Custom",
                                "tools": {
                                    "diagnostics": true,
                                    "find_path": true,
                                    "read_file": true
                                }
                            }
                        }
                    }
                }
            "#,
        ),
    )
}

#[test]
fn test_rename_path_search_to_find_path() {
    assert_migrate_settings(
        r#"
                {
                    "assistant": {
                        "profiles": {
                            "default": {
                                "tools": {
                                    "path_search": true,
                                    "read_file": true
                                }
                            }
                        }
                    }
                }
            "#,
        Some(
            r#"
                {
                    "agent": {
                        "profiles": {
                            "default": {
                                "tools": {
                                    "find_path": true,
                                    "read_file": true
                                }
                            }
                        }
                    }
                }
            "#,
        ),
    );
}

#[test]
fn test_rename_assistant() {
    assert_migrate_settings(
        r#"{
                "assistant": {
                    "foo": "bar"
                },
                "edit_predictions": {
                    "enabled_in_assistant": false,
                }
            }"#,
        Some(
            r#"{
                "agent": {
                    "foo": "bar"
                },
                "edit_predictions": {
                    "enabled_in_text_threads": false,
                }
            }"#,
        ),
    );
}

#[test]
fn test_comment_duplicated_agent() {
    assert_migrate_settings(
        r#"{
                "agent": {
                    "name": "assistant-1",
                "model": "gpt-4", // weird formatting
                    "utf8": "привіт"
                },
                "something": "else",
                "agent": {
                    "name": "assistant-2",
                    "model": "gemini-pro"
                }
            }
        "#,
        Some(
            r#"{
                /* Duplicated key auto-commented: "agent": {
                    "name": "assistant-1",
                "model": "gpt-4", // weird formatting
                    "utf8": "привіт"
                }, */
                "something": "else",
                "agent": {
                    "name": "assistant-2",
                    "model": "gemini-pro"
                }
            }
        "#,
        ),
    );
}

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

#[test]
fn test_flatten_code_action_formatters_basic_array() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_01::flatten_code_actions_formatters,
        )],
        &r#"{
        "formatter": [
          {
            "code_actions": {
              "included-1": true,
              "included-2": true,
              "excluded": false,
            }
          }
        ]
      }"#
        .unindent(),
        Some(
            &r#"{
        "formatter": [
          {
            "code_action": "included-1"
          },
          {
            "code_action": "included-2"
          }
        ]
      }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_flatten_code_action_formatters_basic_object() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_01::flatten_code_actions_formatters,
        )],
        &r#"{
        "formatter": {
          "code_actions": {
            "included-1": true,
            "excluded": false,
            "included-2": true
          }
        }
      }"#
        .unindent(),
        Some(
            &r#"{
                  "formatter": [
                    {
                      "code_action": "included-1"
                    },
                    {
                      "code_action": "included-2"
                    }
                  ]
                }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_flatten_code_action_formatters_array_with_multiple_action_blocks() {
    assert_migrate_settings(
        &r#"{
          "formatter": [
            {
               "code_actions": {
                  "included-1": true,
                  "included-2": true,
                  "excluded": false,
               }
            },
            {
              "language_server": "ruff"
            },
            {
               "code_actions": {
                  "excluded": false,
                  "excluded-2": false,
               }
            }
            // some comment
            ,
            {
               "code_actions": {
                "excluded": false,
                "included-3": true,
                "included-4": true,
               }
            },
          ]
        }"#
        .unindent(),
        Some(
            &r#"{
        "formatter": [
          {
            "code_action": "included-1"
          },
          {
            "code_action": "included-2"
          },
          {
            "language_server": "ruff"
          },
          {
            "code_action": "included-3"
          },
          {
            "code_action": "included-4"
          }
        ]
      }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_flatten_code_action_formatters_array_with_multiple_action_blocks_in_languages() {
    assert_migrate_settings(
        &r#"{
        "languages": {
          "Rust": {
            "formatter": [
              {
                "code_actions": {
                  "included-1": true,
                  "included-2": true,
                  "excluded": false,
                }
              },
              {
                "language_server": "ruff"
              },
              {
                "code_actions": {
                  "excluded": false,
                  "excluded-2": false,
                }
              }
              // some comment
              ,
              {
                "code_actions": {
                  "excluded": false,
                  "included-3": true,
                  "included-4": true,
                }
              },
            ]
          }
        }
      }"#
        .unindent(),
        Some(
            &r#"{
          "languages": {
            "Rust": {
              "formatter": [
                {
                  "code_action": "included-1"
                },
                {
                  "code_action": "included-2"
                },
                {
                  "language_server": "ruff"
                },
                {
                  "code_action": "included-3"
                },
                {
                  "code_action": "included-4"
                }
              ]
            }
          }
        }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_flatten_code_action_formatters_array_with_multiple_action_blocks_in_defaults_and_multiple_languages()
 {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_01::flatten_code_actions_formatters,
        )],
        &r#"{
        "formatter": {
          "code_actions": {
            "default-1": true,
            "default-2": true,
            "default-3": true,
            "default-4": true,
          }
        },
        "languages": {
          "Rust": {
            "formatter": [
              {
                "code_actions": {
                  "included-1": true,
                  "included-2": true,
                  "excluded": false,
                }
              },
              {
                "language_server": "ruff"
              },
              {
                "code_actions": {
                  "excluded": false,
                  "excluded-2": false,
                }
              }
              // some comment
              ,
              {
                "code_actions": {
                  "excluded": false,
                  "included-3": true,
                  "included-4": true,
                }
              },
            ]
          },
          "Python": {
            "formatter": [
              {
                "language_server": "ruff"
              },
              {
                "code_actions": {
                  "excluded": false,
                  "excluded-2": false,
                }
              }
              // some comment
              ,
              {
                "code_actions": {
                  "excluded": false,
                  "included-3": true,
                  "included-4": true,
                }
              },
            ]
          }
        }
      }"#
        .unindent(),
        Some(
            &r#"{
          "formatter": [
            {
              "code_action": "default-1"
            },
            {
              "code_action": "default-2"
            },
            {
              "code_action": "default-3"
            },
            {
              "code_action": "default-4"
            }
          ],
          "languages": {
            "Rust": {
              "formatter": [
                {
                  "code_action": "included-1"
                },
                {
                  "code_action": "included-2"
                },
                {
                  "language_server": "ruff"
                },
                {
                  "code_action": "included-3"
                },
                {
                  "code_action": "included-4"
                }
              ]
            },
            "Python": {
              "formatter": [
                {
                  "language_server": "ruff"
                },
                {
                  "code_action": "included-3"
                },
                {
                  "code_action": "included-4"
                }
              ]
            }
          }
        }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_flatten_code_action_formatters_array_with_format_on_save_and_multiple_languages() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_01::flatten_code_actions_formatters,
        )],
        &r#"{
        "formatter": {
          "code_actions": {
            "default-1": true,
            "default-2": true,
            "default-3": true,
            "default-4": true,
          }
        },
        "format_on_save": [
          {
            "code_actions": {
              "included-1": true,
              "included-2": true,
              "excluded": false,
            }
          },
          {
            "language_server": "ruff"
          },
          {
            "code_actions": {
              "excluded": false,
              "excluded-2": false,
            }
          }
          // some comment
          ,
          {
            "code_actions": {
              "excluded": false,
              "included-3": true,
              "included-4": true,
            }
          },
        ],
        "languages": {
          "Rust": {
            "format_on_save": "prettier",
            "formatter": [
              {
                "code_actions": {
                  "included-1": true,
                  "included-2": true,
                  "excluded": false,
                }
              },
              {
                "language_server": "ruff"
              },
              {
                "code_actions": {
                  "excluded": false,
                  "excluded-2": false,
                }
              }
              // some comment
              ,
              {
                "code_actions": {
                  "excluded": false,
                  "included-3": true,
                  "included-4": true,
                }
              },
            ]
          },
          "Python": {
            "format_on_save": {
              "code_actions": {
                "on-save-1": true,
                "on-save-2": true,
              }
            },
            "formatter": [
              {
                "language_server": "ruff"
              },
              {
                "code_actions": {
                  "excluded": false,
                  "excluded-2": false,
                }
              }
              // some comment
              ,
              {
                "code_actions": {
                  "excluded": false,
                  "included-3": true,
                  "included-4": true,
                }
              },
            ]
          }
        }
      }"#
        .unindent(),
        Some(
            &r#"
        {
          "formatter": [
            {
              "code_action": "default-1"
            },
            {
              "code_action": "default-2"
            },
            {
              "code_action": "default-3"
            },
            {
              "code_action": "default-4"
            }
          ],
          "format_on_save": [
            {
              "code_action": "included-1"
            },
            {
              "code_action": "included-2"
            },
            {
              "language_server": "ruff"
            },
            {
              "code_action": "included-3"
            },
            {
              "code_action": "included-4"
            }
          ],
          "languages": {
            "Rust": {
              "format_on_save": "prettier",
              "formatter": [
                {
                  "code_action": "included-1"
                },
                {
                  "code_action": "included-2"
                },
                {
                  "language_server": "ruff"
                },
                {
                  "code_action": "included-3"
                },
                {
                  "code_action": "included-4"
                }
              ]
            },
            "Python": {
              "format_on_save": [
                {
                  "code_action": "on-save-1"
                },
                {
                  "code_action": "on-save-2"
                }
              ],
              "formatter": [
                {
                  "language_server": "ruff"
                },
                {
                  "code_action": "included-3"
                },
                {
                  "code_action": "included-4"
                }
              ]
            }
          }
        }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_format_on_save_formatter_migration_basic() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_02::remove_formatters_on_save,
        )],
        &r#"{
                  "format_on_save": "prettier"
              }"#
        .unindent(),
        Some(
            &r#"{
                      "formatter": "prettier",
                      "format_on_save": "on"
                  }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_format_on_save_formatter_migration_array() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_02::remove_formatters_on_save,
        )],
        &r#"{
                "format_on_save": ["prettier", {"language_server": "eslint"}]
            }"#
        .unindent(),
        Some(
            &r#"{
                    "formatter": [
                        "prettier",
                        {
                            "language_server": "eslint"
                        }
                    ],
                    "format_on_save": "on"
                }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_format_on_save_on_off_unchanged() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_02::remove_formatters_on_save,
        )],
        &r#"{
                "format_on_save": "on"
            }"#
        .unindent(),
        None,
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_02::remove_formatters_on_save,
        )],
        &r#"{
                "format_on_save": "off"
            }"#
        .unindent(),
        None,
    );
}

#[test]
fn test_format_on_save_formatter_migration_in_languages() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_02::remove_formatters_on_save,
        )],
        &r#"{
                "languages": {
                    "Rust": {
                        "format_on_save": "rust-analyzer"
                    },
                    "Python": {
                        "format_on_save": ["ruff", "black"]
                    }
                }
            }"#
        .unindent(),
        Some(
            &r#"{
                    "languages": {
                        "Rust": {
                            "formatter": "rust-analyzer",
                            "format_on_save": "on"
                        },
                        "Python": {
                            "formatter": [
                                "ruff",
                                "black"
                            ],
                            "format_on_save": "on"
                        }
                    }
                }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_format_on_save_formatter_migration_mixed_global_and_languages() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_02::remove_formatters_on_save,
        )],
        &r#"{
                "format_on_save": "prettier",
                "languages": {
                    "Rust": {
                        "format_on_save": "rust-analyzer"
                    },
                    "Python": {
                        "format_on_save": "on"
                    }
                }
            }"#
        .unindent(),
        Some(
            &r#"{
                    "formatter": "prettier",
                    "format_on_save": "on",
                    "languages": {
                        "Rust": {
                            "formatter": "rust-analyzer",
                            "format_on_save": "on"
                        },
                        "Python": {
                            "format_on_save": "on"
                        }
                    }
                }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_format_on_save_no_migration_when_no_format_on_save() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_02::remove_formatters_on_save,
        )],
        &r#"{
                "formatter": ["prettier"]
            }"#
        .unindent(),
        None,
    );
}

#[test]
fn test_restore_code_actions_on_format() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_16::restore_code_actions_on_format,
        )],
        &r#"{
                "formatter": {
                    "code_action": "foo"
                }
            }"#
        .unindent(),
        Some(
            &r#"{
                    "code_actions_on_format": {
                        "foo": true
                    },
                    "formatter": []
                }"#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_16::restore_code_actions_on_format,
        )],
        &r#"{
                "formatter": [
                    { "code_action": "foo" },
                    "auto"
                ]
            }"#
        .unindent(),
        None,
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_16::restore_code_actions_on_format,
        )],
        &r#"{
                "formatter": {
                    "code_action": "foo"
                },
                "code_actions_on_format": {
                    "bar": true,
                    "baz": false
                }
            }"#
        .unindent(),
        Some(
            &r#"{
                    "formatter": [],
                    "code_actions_on_format": {
                        "foo": true,
                        "bar": true,
                        "baz": false
                    }
                }"#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_16::restore_code_actions_on_format,
        )],
        &r#"{
                "formatter": [
                    { "code_action": "foo" },
                    { "code_action": "qux" },
                ],
                "code_actions_on_format": {
                    "bar": true,
                    "baz": false
                }
            }"#
        .unindent(),
        Some(
            &r#"{
                    "formatter": [],
                    "code_actions_on_format": {
                        "foo": true,
                        "qux": true,
                        "bar": true,
                        "baz": false
                    }
                }"#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_16::restore_code_actions_on_format,
        )],
        &r#"{
                "formatter": [],
                "code_actions_on_format": {
                    "bar": true,
                    "baz": false
                }
            }"#
        .unindent(),
        None,
    );
}

#[test]
fn test_make_file_finder_include_ignored_an_enum() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_17::make_file_finder_include_ignored_an_enum,
        )],
        &r#"{ }"#.unindent(),
        None,
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_17::make_file_finder_include_ignored_an_enum,
        )],
        &r#"{
                "file_finder": {
                    "include_ignored": true
                }
            }"#
        .unindent(),
        Some(
            &r#"{
                    "file_finder": {
                        "include_ignored": "all"
                    }
                }"#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_17::make_file_finder_include_ignored_an_enum,
        )],
        &r#"{
                "file_finder": {
                    "include_ignored": false
                }
            }"#
        .unindent(),
        Some(
            &r#"{
                    "file_finder": {
                        "include_ignored": "indexed"
                    }
                }"#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_17::make_file_finder_include_ignored_an_enum,
        )],
        &r#"{
                "file_finder": {
                    "include_ignored": null
                }
            }"#
        .unindent(),
        Some(
            &r#"{
                    "file_finder": {
                        "include_ignored": "smart"
                    }
                }"#
            .unindent(),
        ),
    );

    // Platform key: settings nested inside "linux" should be migrated
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_17::make_file_finder_include_ignored_an_enum,
        )],
        &r#"
            {
                "linux": {
                    "file_finder": {
                        "include_ignored": true
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "linux": {
                        "file_finder": {
                            "include_ignored": "all"
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Profile: settings nested inside profiles should be migrated
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_17::make_file_finder_include_ignored_an_enum,
        )],
        &r#"
            {
                "profiles": {
                    "work": {
                        "file_finder": {
                            "include_ignored": false
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
                            "file_finder": {
                                "include_ignored": "indexed"
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
fn test_make_relative_line_numbers_an_enum() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_21::make_relative_line_numbers_an_enum,
        )],
        &r#"{ }"#.unindent(),
        None,
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_21::make_relative_line_numbers_an_enum,
        )],
        &r#"{
                "relative_line_numbers": true
            }"#
        .unindent(),
        Some(
            &r#"{
                    "relative_line_numbers": "enabled"
                }"#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_21::make_relative_line_numbers_an_enum,
        )],
        &r#"{
                "relative_line_numbers": false
            }"#
        .unindent(),
        Some(
            &r#"{
                    "relative_line_numbers": "disabled"
                }"#
            .unindent(),
        ),
    );

    // Platform key: settings nested inside "macos" should be migrated
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_21::make_relative_line_numbers_an_enum,
        )],
        &r#"
            {
                "macos": {
                    "relative_line_numbers": true
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "macos": {
                        "relative_line_numbers": "enabled"
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Profile: settings nested inside profiles should be migrated
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_21::make_relative_line_numbers_an_enum,
        )],
        &r#"
            {
                "profiles": {
                    "dev": {
                        "relative_line_numbers": false
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "profiles": {
                        "dev": {
                            "relative_line_numbers": "disabled"
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );
}

#[test]
fn test_make_play_sound_when_agent_done_an_enum() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_03_30::make_play_sound_when_agent_done_an_enum,
        )],
        &r#"{ }"#.unindent(),
        None,
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_03_30::make_play_sound_when_agent_done_an_enum,
        )],
        &r#"{
                "agent": {
                    "play_sound_when_agent_done": true
                }
            }"#
        .unindent(),
        Some(
            &r#"{
                    "agent": {
                        "play_sound_when_agent_done": "always"
                    }
                }"#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_03_30::make_play_sound_when_agent_done_an_enum,
        )],
        &r#"{
                "agent": {
                    "play_sound_when_agent_done": false
                }
            }"#
        .unindent(),
        Some(
            &r#"{
                    "agent": {
                        "play_sound_when_agent_done": "never"
                    }
                }"#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_03_30::make_play_sound_when_agent_done_an_enum,
        )],
        &r#"{
                "agent": {
                    "play_sound_when_agent_done": "when_hidden"
                }
            }"#
        .unindent(),
        None,
    );

    // Platform key: settings nested inside "macos" should be migrated
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_03_30::make_play_sound_when_agent_done_an_enum,
        )],
        &r#"
            {
                "macos": {
                    "agent": {
                        "play_sound_when_agent_done": true
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
                            "play_sound_when_agent_done": "always"
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Profile: settings nested inside profiles should be migrated
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_03_30::make_play_sound_when_agent_done_an_enum,
        )],
        &r#"
            {
                "profiles": {
                    "work": {
                        "agent": {
                            "play_sound_when_agent_done": false
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
                                "play_sound_when_agent_done": "never"
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
fn test_remove_context_server_source() {
    assert_migrate_settings(
        &r#"
            {
                "context_servers": {
                    "extension_server": {
                        "source": "extension",
                        "settings": {
                            "foo": "bar"
                        }
                    },
                    "custom_server": {
                        "source": "custom",
                        "command": "foo",
                        "args": ["bar"],
                        "env": {
                            "FOO": "BAR"
                        }
                    },
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "context_servers": {
                        "extension_server": {
                            "settings": {
                                "foo": "bar"
                            }
                        },
                        "custom_server": {
                            "command": "foo",
                            "args": ["bar"],
                            "env": {
                                "FOO": "BAR"
                            }
                        },
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Platform key: settings nested inside "linux" should be migrated
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_11_25::remove_context_server_source,
        )],
        &r#"
            {
                "linux": {
                    "context_servers": {
                        "my_server": {
                            "source": "extension",
                            "settings": {
                                "key": "value"
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
                        "context_servers": {
                            "my_server": {
                                "settings": {
                                    "key": "value"
                                }
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Profile: settings nested inside profiles should be migrated
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_11_25::remove_context_server_source,
        )],
        &r#"
            {
                "profiles": {
                    "work": {
                        "context_servers": {
                            "my_server": {
                                "source": "custom",
                                "command": "foo",
                                "args": ["bar"]
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
                            "context_servers": {
                                "my_server": {
                                    "command": "foo",
                                    "args": ["bar"]
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
fn test_project_panel_open_file_on_paste_migration() {
    assert_migrate_settings(
        &r#"
            {
                "project_panel": {
                    "open_file_on_paste": true
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "project_panel": {
                        "auto_open": { "on_paste": true }
                    }
                }
                "#
            .unindent(),
        ),
    );

    assert_migrate_settings(
        &r#"
            {
                "project_panel": {
                    "open_file_on_paste": false
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "project_panel": {
                        "auto_open": { "on_paste": false }
                    }
                }
                "#
            .unindent(),
        ),
    );
}

#[test]
fn test_enable_preview_from_code_navigation_migration() {
    assert_migrate_settings(
        &r#"
            {
                "other_setting_1": 1,
                "preview_tabs": {
                    "other_setting_2": 2,
                    "enable_preview_from_code_navigation": false
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "other_setting_1": 1,
                    "preview_tabs": {
                        "other_setting_2": 2,
                        "enable_keep_preview_on_code_navigation": false
                    }
                }
                "#
            .unindent(),
        ),
    );

    assert_migrate_settings(
        &r#"
            {
                "other_setting_1": 1,
                "preview_tabs": {
                    "other_setting_2": 2,
                    "enable_preview_from_code_navigation": true
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "other_setting_1": 1,
                    "preview_tabs": {
                        "other_setting_2": 2,
                        "enable_keep_preview_on_code_navigation": true
                    }
                }
                "#
            .unindent(),
        ),
    );
}

#[test]
fn test_make_auto_indent_an_enum() {
    // Empty settings should not change
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_01_27::make_auto_indent_an_enum,
        )],
        &r#"{ }"#.unindent(),
        None,
    );

    // true should become "syntax_aware"
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_01_27::make_auto_indent_an_enum,
        )],
        &r#"{
                "auto_indent": true
            }"#
        .unindent(),
        Some(
            &r#"{
                "auto_indent": "syntax_aware"
            }"#
            .unindent(),
        ),
    );

    // false should become "none"
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_01_27::make_auto_indent_an_enum,
        )],
        &r#"{
                "auto_indent": false
            }"#
        .unindent(),
        Some(
            &r#"{
                "auto_indent": "none"
            }"#
            .unindent(),
        ),
    );

    // Already valid enum values should not change
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_01_27::make_auto_indent_an_enum,
        )],
        &r#"{
                "auto_indent": "preserve_indent"
            }"#
        .unindent(),
        None,
    );

    // Should also work inside languages
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_01_27::make_auto_indent_an_enum,
        )],
        &r#"{
                "auto_indent": true,
                "languages": {
                    "Python": {
                        "auto_indent": false
                    }
                }
            }"#
        .unindent(),
        Some(
            &r#"{
                    "auto_indent": "syntax_aware",
                    "languages": {
                        "Python": {
                            "auto_indent": "none"
                        }
                    }
                }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_move_edit_prediction_provider_to_edit_predictions() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_02::move_edit_prediction_provider_to_edit_predictions,
        )],
        &r#"{ }"#.unindent(),
        None,
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_02::move_edit_prediction_provider_to_edit_predictions,
        )],
        &r#"
            {
                "features": {
                    "edit_prediction_provider": "copilot"
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "edit_predictions": {
                        "provider": "copilot"
                    }
                }
                "#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_02::move_edit_prediction_provider_to_edit_predictions,
        )],
        &r#"
            {
                "features": {
                    "edit_prediction_provider": "mav"
                },
                "edit_predictions": {
                    "mode": "eager"
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "edit_predictions": {
                        "provider": "mav",
                        "mode": "eager"
                    }
                }
                "#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_02::move_edit_prediction_provider_to_edit_predictions,
        )],
        &r#"
            {
                "features": {
                    "edit_prediction_provider": "supermaven"
                },
                "edit_predictions": {
                    "provider": "copilot"
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "edit_predictions": {
                        "provider": "copilot"
                    }
                }
                "#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_02::move_edit_prediction_provider_to_edit_predictions,
        )],
        &r#"
            {
                "edit_predictions": {
                    "provider": "mav"
                }
            }
            "#
        .unindent(),
        None,
    );

    // Non-object edit_predictions (e.g. true) should gracefully skip
    // instead of bail!-ing and aborting the entire migration chain.
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_02::move_edit_prediction_provider_to_edit_predictions,
        )],
        &r#"
            {
                "features": {
                    "edit_prediction_provider": "copilot"
                },
                "edit_predictions": true
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "edit_predictions": true
                }
                "#
            .unindent(),
        ),
    );

    // Platform key: settings nested inside "macos" should be migrated
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_02::move_edit_prediction_provider_to_edit_predictions,
        )],
        &r#"
            {
                "macos": {
                    "features": {
                        "edit_prediction_provider": "copilot"
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "macos": {
                        "edit_predictions": {
                            "provider": "copilot"
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Profile: settings nested inside profiles should be migrated
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_02::move_edit_prediction_provider_to_edit_predictions,
        )],
        &r#"
            {
                "profiles": {
                    "work": {
                        "features": {
                            "edit_prediction_provider": "copilot"
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
                            "edit_predictions": {
                                "provider": "copilot"
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Combined: root + platform + profile should all be migrated simultaneously
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_02::move_edit_prediction_provider_to_edit_predictions,
        )],
        &r#"
            {
                "features": {
                    "edit_prediction_provider": "copilot"
                },
                "macos": {
                    "features": {
                        "edit_prediction_provider": "mav"
                    }
                },
                "profiles": {
                    "work": {
                        "features": {
                            "edit_prediction_provider": "supermaven"
                        }
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "edit_predictions": {
                        "provider": "copilot"
                    },
                    "macos": {
                        "edit_predictions": {
                            "provider": "mav"
                        }
                    },
                    "profiles": {
                        "work": {
                            "edit_predictions": {
                                "provider": "supermaven"
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
fn test_migrate_experimental_sweep_mercury() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_03::migrate_experimental_sweep_mercury,
        )],
        &r#"{ }"#.unindent(),
        None,
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_03::migrate_experimental_sweep_mercury,
        )],
        &r#"
            {
                "edit_predictions": {
                    "provider": {
                        "experimental": "sweep"
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "edit_predictions": {
                        "provider": "sweep"
                    }
                }
                "#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_03::migrate_experimental_sweep_mercury,
        )],
        &r#"
            {
                "edit_predictions": {
                    "provider": {
                        "experimental": "mercury"
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "edit_predictions": {
                        "provider": "mercury"
                    }
                }
                "#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_03::migrate_experimental_sweep_mercury,
        )],
        &r#"
            {
                "features": {
                    "edit_prediction_provider": {
                        "experimental": "sweep"
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "features": {
                        "edit_prediction_provider": "sweep"
                    }
                }
                "#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_03::migrate_experimental_sweep_mercury,
        )],
        &r#"
            {
                "edit_predictions": {
                    "provider": "mav"
                }
            }
            "#
        .unindent(),
        None,
    );

    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_03::migrate_experimental_sweep_mercury,
        )],
        &r#"
            {
                "edit_predictions": {
                    "provider": {
                        "experimental": "zeta2"
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "edit_predictions": {
                        "provider": "mav"
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Platform key: settings nested inside "linux" should be migrated
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_03::migrate_experimental_sweep_mercury,
        )],
        &r#"
            {
                "linux": {
                    "edit_predictions": {
                        "provider": {
                            "experimental": "sweep"
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
                        "edit_predictions": {
                            "provider": "sweep"
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Profile: settings nested inside profiles should be migrated
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_03::migrate_experimental_sweep_mercury,
        )],
        &r#"
            {
                "profiles": {
                    "dev": {
                        "edit_predictions": {
                            "provider": {
                                "experimental": "mercury"
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
                        "dev": {
                            "edit_predictions": {
                                "provider": "mercury"
                            }
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );

    // Combined: root + platform + profile should all be migrated simultaneously
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2026_02_03::migrate_experimental_sweep_mercury,
        )],
        &r#"
            {
                "edit_predictions": {
                    "provider": {
                        "experimental": "sweep"
                    }
                },
                "linux": {
                    "edit_predictions": {
                        "provider": {
                            "experimental": "mercury"
                        }
                    }
                },
                "profiles": {
                    "dev": {
                        "edit_predictions": {
                            "provider": {
                                "experimental": "sweep"
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
                    "edit_predictions": {
                        "provider": "sweep"
                    },
                    "linux": {
                        "edit_predictions": {
                            "provider": "mercury"
                        }
                    },
                    "profiles": {
                        "dev": {
                            "edit_predictions": {
                                "provider": "sweep"
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
fn test_migrate_always_allow_tool_actions_to_default() {
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

#[test]
fn test_migrate_edit_prediction_conflict_context() {
    assert_migrate_with_migrations(
            &[MigrationType::TreeSitter(
                migrations::m_2026_03_23::KEYMAP_PATTERNS,
                &KEYMAP_QUERY_2026_03_23,
            )],
            &r#"
            [
                {
                    "context": "Editor && edit_prediction_conflict",
                    "bindings": {
                        "ctrl-enter": "editor::AcceptEditPrediction" // Example of a modified keybinding
                    }
                }
            ]
            "#.unindent(),
            Some(
                &r#"
                [
                    {
                        "context": "Editor && (edit_prediction && (showing_completions || in_leading_whitespace))",
                        "bindings": {
                            "ctrl-enter": "editor::AcceptEditPrediction" // Example of a modified keybinding
                        }
                    }
                ]
                "#.unindent(),
            ),
        );

    assert_migrate_with_migrations(
            &[MigrationType::TreeSitter(
                migrations::m_2026_03_23::KEYMAP_PATTERNS,
                &KEYMAP_QUERY_2026_03_23,
            )],
            &r#"
            [
                {
                    "context": "Editor && edit_prediction_conflict && !showing_completions",
                    "bindings": {
                        // Here we don't require a modifier unless there's a language server completion
                        "tab": "editor::AcceptEditPrediction"
                    }
                }
            ]
            "#.unindent(),
            Some(
                &r#"
                [
                    {
                        "context": "Editor && (edit_prediction && in_leading_whitespace)",
                        "bindings": {
                            // Here we don't require a modifier unless there's a language server completion
                            "tab": "editor::AcceptEditPrediction"
                        }
                    }
                ]
                "#.unindent(),
            ),
        );

    assert_migrate_with_migrations(
        &[MigrationType::TreeSitter(
            migrations::m_2026_03_23::KEYMAP_PATTERNS,
            &KEYMAP_QUERY_2026_03_23,
        )],
        &r#"
            [
                {
                    "context": "Editor && edit_prediction_conflict && showing_completions",
                    "bindings": {
                        "tab": "editor::AcceptEditPrediction"
                    }
                }
            ]
            "#
        .unindent(),
        Some(
            &r#"
                [
                    {
                        "context": "Editor && (edit_prediction && showing_completions)",
                        "bindings": {
                            "tab": "editor::AcceptEditPrediction"
                        }
                    }
                ]
                "#
            .unindent(),
        ),
    );

    assert_migrate_with_migrations(
            &[MigrationType::TreeSitter(
                migrations::m_2026_03_23::KEYMAP_PATTERNS,
                &KEYMAP_QUERY_2026_03_23,
            )],
            &r#"
            [
                {
                    "context": "Editor && edit_prediction",
                    "bindings": {
                        "tab": "editor::AcceptEditPrediction",
                        // Optional: This makes the default `alt-l` binding do nothing.
                        "alt-l": null
                    }
                },
                {
                    "context": "Editor && edit_prediction_conflict",
                    "bindings": {
                        "alt-tab": "editor::AcceptEditPrediction",
                        // Optional: This makes the default `alt-l` binding do nothing.
                        "alt-l": null
                    }
                },
            ]
            "#
            .unindent(),
            Some(
                &r#"
                    [
                        {
                            "context": "Editor && edit_prediction",
                            "bindings": {
                                "tab": "editor::AcceptEditPrediction",
                                // Optional: This makes the default `alt-l` binding do nothing.
                                "alt-l": null
                            }
                        },
                        {
                            "context": "Editor && (edit_prediction && (showing_completions || in_leading_whitespace))",
                            "bindings": {
                                "alt-tab": "editor::AcceptEditPrediction",
                                // Optional: This makes the default `alt-l` binding do nothing.
                                "alt-l": null
                            }
                        },
                    ]
                "#
                .unindent(),
            ),
        );
}

#[test]
fn test_restructure_profiles_with_settings_key() {
    assert_migrate_settings(
        &r#"
                {
                    "buffer_font_size": 14,
                    "profiles": {
                        "Presenting": {
                            "buffer_font_size": 20,
                            "theme": "One Light"
                        },
                        "Minimal": {
                            "vim_mode": true
                        }
                    }
                }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "buffer_font_size": 14,
                    "profiles": {
                        "Presenting": {
                            "settings": {
                                "buffer_font_size": 20,
                                "theme": "One Light"
                            }
                        },
                        "Minimal": {
                            "settings": {
                                "vim_mode": true
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
fn test_restructure_profiles_with_settings_key_already_migrated() {
    assert_migrate_settings(
        &r#"
                {
                    "profiles": {
                        "Presenting": {
                            "settings": {
                                "buffer_font_size": 20
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
fn test_restructure_profiles_with_settings_key_no_profiles() {
    assert_migrate_settings(
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

#[test]
fn test_mcp_settings_migration_adds_settings_to_extension_servers() {
    assert_migrate_settings(
        r#"{
    "context_servers": {
        "extension_server": {},
        "stdio_server": {
            "command": "npx",
            "args": ["-y", "some-server"]
        },
        "http_server": {
            "url": "https://example.com/mcp"
        },
        "http_server_with_headers": {
            "url": "https://example.com/mcp",
            "headers": {
                "Authorization": "Bearer token"
            }
        }
    }
}"#,
        Some(
            r#"{
    "context_servers": {
        "extension_server": {
            "settings": {}
        },
        "stdio_server": {
            "command": "npx",
            "args": ["-y", "some-server"]
        },
        "http_server": {
            "url": "https://example.com/mcp"
        },
        "http_server_with_headers": {
            "url": "https://example.com/mcp",
            "headers": {
                "Authorization": "Bearer token"
            }
        }
    }
}"#,
        ),
    );
}

#[test]
fn test_promote_show_branch_icon_true_to_show_branch_status_icon_at_root() {
    assert_migrate_settings(
        &r#"
            {
                "sidebar": {
                    "show_branch_icon": true,
                    "show_branch_name": true
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "sidebar": {
                        "show_branch_status_icon": true,
                        "show_branch_name": true
                    }
                }
                "#
            .unindent(),
        ),
    );
}

#[test]
fn test_drop_show_branch_icon_false_without_setting_status_icon() {
    assert_migrate_settings(
        &r#"
            {
                "sidebar": {
                    "show_branch_icon": false,
                    "show_branch_name": true
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "sidebar": {
                        "show_branch_name": true
                    }
                }
                "#
            .unindent(),
        ),
    );
}

#[test]
fn test_promote_show_branch_icon_true_to_show_branch_status_icon_in_platform_override() {
    assert_migrate_settings(
        &r#"
            {
                "macos": {
                    "sidebar": {
                        "show_branch_icon": true,
                        "show_branch_name": true
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "macos": {
                        "sidebar": {
                            "show_branch_status_icon": true,
                            "show_branch_name": true
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );
}

#[test]
fn test_promote_show_branch_icon_true_to_show_branch_status_icon_in_release_override() {
    assert_migrate_settings(
        &r#"
            {
                "preview": {
                    "sidebar": {
                        "show_branch_icon": true,
                        "show_branch_name": true
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "preview": {
                        "sidebar": {
                            "show_branch_status_icon": true,
                            "show_branch_name": true
                        }
                    }
                }
                "#
            .unindent(),
        ),
    );
}

#[test]
fn test_promote_show_branch_icon_true_to_show_branch_status_icon_in_profiles() {
    assert_migrate_settings(
        &r#"
            {
                "profiles": {
                    "work": {
                        "sidebar": {
                            "show_branch_icon": true,
                            "show_branch_name": true
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
                            "settings": {
                                "sidebar": {
                                    "show_branch_status_icon": true,
                                    "show_branch_name": true
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
fn test_promote_show_branch_icon_true_to_show_branch_status_icon_across_all_scopes() {
    assert_migrate_settings(
        &r#"
            {
                "sidebar": {
                    "show_branch_icon": true,
                    "show_branch_name": true
                },
                "macos": {
                    "sidebar": {
                        "show_branch_icon": true,
                        "show_branch_name": true
                    }
                },
                "preview": {
                    "sidebar": {
                        "show_branch_icon": true,
                        "show_branch_name": true
                    }
                },
                "profiles": {
                    "work": {
                        "sidebar": {
                            "show_branch_icon": true,
                            "show_branch_name": true
                        }
                    }
                }
            }
            "#
        .unindent(),
        Some(
            &r#"
                {
                    "sidebar": {
                        "show_branch_status_icon": true,
                        "show_branch_name": true
                    },
                    "macos": {
                        "sidebar": {
                            "show_branch_status_icon": true,
                            "show_branch_name": true
                        }
                    },
                    "preview": {
                        "sidebar": {
                            "show_branch_status_icon": true,
                            "show_branch_name": true
                        }
                    },
                    "profiles": {
                        "work": {
                            "settings": {
                                "sidebar": {
                                    "show_branch_status_icon": true,
                                    "show_branch_name": true
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
fn test_rename_hide_mouse_on_typing_and_movement_to_on_typing_and_action() {
    assert_migrate_settings(
        r#"
                {
                    "hide_mouse": "on_typing_and_movement"
                }
            "#,
        Some(
            r#"
                {
                    "hide_mouse": "on_typing_and_action"
                }
            "#,
        ),
    );
}

#[test]
fn test_chain_hide_mouse_while_typing_to_on_typing_and_action() {
    assert_migrate_settings(
        r#"
                {
                    "hide_mouse_while_typing": true
                }
            "#,
        Some(
            r#"
                {
                    "hide_mouse": "on_typing_and_action"
                }
            "#,
        ),
    );
}

#[test]
fn test_promote_show_branch_icon_true_to_show_branch_status_icon_no_change_when_already_migrated() {
    assert_migrate_settings(
        &r#"
            {
                "sidebar": {
                    "show_branch_status_icon": true,
                    "show_branch_name": true
                }
            }
            "#
        .unindent(),
        None,
    );

    // No sidebar key — should be unchanged
    assert_migrate_settings(&r#"{ "theme": "One Dark" }"#.unindent(), None);

    // sidebar without show_branch_icon — should be unchanged
    assert_migrate_settings(
        &r#"
            {
                "sidebar": {
                    "show_branch_name": true
                }
            }
            "#
        .unindent(),
        None,
    );
}
