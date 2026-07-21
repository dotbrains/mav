use super::*;

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
