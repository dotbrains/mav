use super::*;

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
