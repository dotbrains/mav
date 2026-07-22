use super::*;

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
