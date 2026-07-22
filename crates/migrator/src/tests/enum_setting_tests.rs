use super::*;

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
