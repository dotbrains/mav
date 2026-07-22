use super::*;

use super::*;

#[test]
fn keymap_update_context_and_remove() {
    zlog::init_test();

    check_keymap_update(
        r#"[
                {
                    "bindings": {
                        // some comment
                        "ctrl-a": "mav::SomeAction"
                        // some other comment
                    }
                }
            ]"#
        .unindent(),
        KeybindUpdateOperation::Replace {
            target: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("ctrl-a"),
                action_name: "mav::SomeAction",
                context: None,
                action_arguments: None,
            },
            source: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("ctrl-b"),
                action_name: "mav::SomeOtherAction",
                context: None,
                action_arguments: Some(r#"{"foo": "bar"}"#),
            },
            target_keybind_source: KeybindSource::User,
        },
        r#"[
                {
                    "bindings": {
                        // some comment
                        "ctrl-b": [
                            "mav::SomeOtherAction",
                            {
                                "foo": "bar"
                            }
                        ]
                        // some other comment
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "context": "SomeContext",
                    "bindings": {
                        "a": "foo::bar",
                        "b": "baz::qux",
                    }
                }
            ]"#
        .unindent(),
        KeybindUpdateOperation::Replace {
            target: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("a"),
                action_name: "foo::bar",
                context: Some("SomeContext"),
                action_arguments: None,
            },
            source: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("c"),
                action_name: "foo::baz",
                context: Some("SomeOtherContext"),
                action_arguments: None,
            },
            target_keybind_source: KeybindSource::User,
        },
        r#"[
                {
                    "context": "SomeContext",
                    "bindings": {
                        "b": "baz::qux",
                    }
                },
                {
                    "context": "SomeOtherContext",
                    "bindings": {
                        "c": "foo::baz"
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "context": "SomeContext",
                    "bindings": {
                        "a": "foo::bar",
                    }
                }
            ]"#
        .unindent(),
        KeybindUpdateOperation::Replace {
            target: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("a"),
                action_name: "foo::bar",
                context: Some("SomeContext"),
                action_arguments: None,
            },
            source: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("c"),
                action_name: "foo::baz",
                context: Some("SomeOtherContext"),
                action_arguments: None,
            },
            target_keybind_source: KeybindSource::User,
        },
        r#"[
                {
                    "context": "SomeOtherContext",
                    "bindings": {
                        "c": "foo::baz",
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "context": "SomeContext",
                    "bindings": {
                        "a": "foo::bar",
                        "c": "foo::baz",
                    }
                },
            ]"#
        .unindent(),
        KeybindUpdateOperation::Remove {
            target: KeybindUpdateTarget {
                context: Some("SomeContext"),
                keystrokes: &parse_keystrokes("a"),
                action_name: "foo::bar",
                action_arguments: None,
            },
            target_keybind_source: KeybindSource::User,
        },
        r#"[
                {
                    "context": "SomeContext",
                    "bindings": {
                        "c": "foo::baz",
                    }
                },
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "context": "SomeContext",
                    "bindings": {
                        "\\ a": "foo::bar",
                        "c": "foo::baz",
                    }
                },
            ]"#
        .unindent(),
        KeybindUpdateOperation::Remove {
            target: KeybindUpdateTarget {
                context: Some("SomeContext"),
                keystrokes: &parse_keystrokes("\\ a"),
                action_name: "foo::bar",
                action_arguments: None,
            },
            target_keybind_source: KeybindSource::User,
        },
        r#"[
                {
                    "context": "SomeContext",
                    "bindings": {
                        "c": "foo::baz",
                    }
                },
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "context": "SomeContext",
                    "bindings": {
                        "a": ["foo::bar", true],
                        "c": "foo::baz",
                    }
                },
            ]"#
        .unindent(),
        KeybindUpdateOperation::Remove {
            target: KeybindUpdateTarget {
                context: Some("SomeContext"),
                keystrokes: &parse_keystrokes("a"),
                action_name: "foo::bar",
                action_arguments: Some("true"),
            },
            target_keybind_source: KeybindSource::User,
        },
        r#"[
                {
                    "context": "SomeContext",
                    "bindings": {
                        "c": "foo::baz",
                    }
                },
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "context": "SomeContext",
                    "bindings": {
                        "b": "foo::baz",
                    }
                },
                {
                    "context": "SomeContext",
                    "bindings": {
                        "a": ["foo::bar", true],
                    }
                },
                {
                    "context": "SomeContext",
                    "bindings": {
                        "c": "foo::baz",
                    }
                },
            ]"#
        .unindent(),
        KeybindUpdateOperation::Remove {
            target: KeybindUpdateTarget {
                context: Some("SomeContext"),
                keystrokes: &parse_keystrokes("a"),
                action_name: "foo::bar",
                action_arguments: Some("true"),
            },
            target_keybind_source: KeybindSource::User,
        },
        r#"[
                {
                    "context": "SomeContext",
                    "bindings": {
                        "b": "foo::baz",
                    }
                },
                {
                    "context": "SomeContext",
                    "bindings": {
                        "c": "foo::baz",
                    }
                },
            ]"#
        .unindent(),
    );
    check_keymap_update(
        r#"[
                {
                    "context": "SomeOtherContext",
                    "use_key_equivalents": true,
                    "bindings": {
                        "b": "foo::bar",
                    }
                },
            ]"#
        .unindent(),
        KeybindUpdateOperation::Add {
            source: KeybindUpdateTarget {
                context: Some("SomeContext"),
                keystrokes: &parse_keystrokes("a"),
                action_name: "foo::baz",
                action_arguments: Some("true"),
            },
            from: Some(KeybindUpdateTarget {
                context: Some("SomeOtherContext"),
                keystrokes: &parse_keystrokes("b"),
                action_name: "foo::bar",
                action_arguments: None,
            }),
        },
        r#"[
                {
                    "context": "SomeOtherContext",
                    "use_key_equivalents": true,
                    "bindings": {
                        "b": "foo::bar",
                    }
                },
                {
                    "context": "SomeContext",
                    "use_key_equivalents": true,
                    "bindings": {
                        "a": [
                            "foo::baz",
                            true
                        ]
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "context": "SomeOtherContext",
                    "use_key_equivalents": true,
                    "bindings": {
                        "b": "foo::bar",
                    }
                },
            ]"#
        .unindent(),
        KeybindUpdateOperation::Remove {
            target: KeybindUpdateTarget {
                context: Some("SomeContext"),
                keystrokes: &parse_keystrokes("a"),
                action_name: "foo::baz",
                action_arguments: Some("true"),
            },
            target_keybind_source: KeybindSource::Default,
        },
        r#"[
                {
                    "context": "SomeOtherContext",
                    "use_key_equivalents": true,
                    "bindings": {
                        "b": "foo::bar",
                    }
                },
                {
                    "context": "SomeContext",
                    "unbind": {
                        "a": [
                            "foo::baz",
                            true
                        ]
                    }
                }
            ]"#
        .unindent(),
    );
}
