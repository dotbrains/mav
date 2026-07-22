use super::*;

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
