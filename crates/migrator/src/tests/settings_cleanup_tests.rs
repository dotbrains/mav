use super::*;

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
