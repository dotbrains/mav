use super::*;

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
