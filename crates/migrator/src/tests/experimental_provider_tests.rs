use super::*;

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
