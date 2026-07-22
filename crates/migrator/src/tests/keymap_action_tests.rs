use super::*;

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
