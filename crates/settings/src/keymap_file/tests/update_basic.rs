use super::*;

use super::*;

#[test]
fn keymap_update_basic() {
    zlog::init_test();

    check_keymap_update(
        "[]",
        KeybindUpdateOperation::add(KeybindUpdateTarget {
            keystrokes: &parse_keystrokes("ctrl-a"),
            action_name: "mav::SomeAction",
            context: None,
            action_arguments: None,
        }),
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        "[]",
        KeybindUpdateOperation::add(KeybindUpdateTarget {
            keystrokes: &parse_keystrokes("\\ a"),
            action_name: "mav::SomeAction",
            context: None,
            action_arguments: None,
        }),
        r#"[
                {
                    "bindings": {
                        "\\ a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        "[]",
        KeybindUpdateOperation::add(KeybindUpdateTarget {
            keystrokes: &parse_keystrokes("ctrl-a"),
            action_name: "mav::SomeAction",
            context: None,
            action_arguments: Some(""),
        }),
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
        KeybindUpdateOperation::add(KeybindUpdateTarget {
            keystrokes: &parse_keystrokes("ctrl-b"),
            action_name: "mav::SomeOtherAction",
            context: None,
            action_arguments: None,
        }),
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                },
                {
                    "bindings": {
                        "ctrl-b": "mav::SomeOtherAction"
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
        KeybindUpdateOperation::add(KeybindUpdateTarget {
            keystrokes: &parse_keystrokes("ctrl-b"),
            action_name: "mav::SomeOtherAction",
            context: None,
            action_arguments: Some(r#"{"foo": "bar"}"#),
        }),
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                },
                {
                    "bindings": {
                        "ctrl-b": [
                            "mav::SomeOtherAction",
                            {
                                "foo": "bar"
                            }
                        ]
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
        KeybindUpdateOperation::add(KeybindUpdateTarget {
            keystrokes: &parse_keystrokes("ctrl-b"),
            action_name: "mav::SomeOtherAction",
            context: Some("Mav > Editor && some_condition = true"),
            action_arguments: Some(r#"{"foo": "bar"}"#),
        }),
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                },
                {
                    "context": "Mav > Editor && some_condition = true",
                    "bindings": {
                        "ctrl-b": [
                            "mav::SomeOtherAction",
                            {
                                "foo": "bar"
                            }
                        ]
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
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
            target_keybind_source: KeybindSource::Base,
        },
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                },
                {
                    "bindings": {
                        "ctrl-b": [
                            "mav::SomeOtherAction",
                            {
                                "foo": "bar"
                            }
                        ]
                    }
                },
                {
                    "unbind": {
                        "ctrl-a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
    );

    // Replacing a non-user binding without changing the keystroke should
    // not produce an unbind suppression entry.
    check_keymap_update(
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
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
                keystrokes: &parse_keystrokes("ctrl-a"),
                action_name: "mav::SomeOtherAction",
                context: None,
                action_arguments: None,
            },
            target_keybind_source: KeybindSource::Base,
        },
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                },
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeOtherAction"
                    }
                }
            ]"#
        .unindent(),
    );

    // Replacing a non-user binding with a context and a keystroke change
    // should produce a suppression entry that preserves the context.
    check_keymap_update(
        r#"[
                {
                    "context": "SomeContext",
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
        KeybindUpdateOperation::Replace {
            target: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("ctrl-a"),
                action_name: "mav::SomeAction",
                context: Some("SomeContext"),
                action_arguments: None,
            },
            source: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("ctrl-b"),
                action_name: "mav::SomeOtherAction",
                context: Some("SomeContext"),
                action_arguments: None,
            },
            target_keybind_source: KeybindSource::Default,
        },
        r#"[
                {
                    "context": "SomeContext",
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                },
                {
                    "context": "SomeContext",
                    "bindings": {
                        "ctrl-b": "mav::SomeOtherAction"
                    }
                },
                {
                    "context": "SomeContext",
                    "unbind": {
                        "ctrl-a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "bindings": {
                        "a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
        KeybindUpdateOperation::Replace {
            target: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("a"),
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
                        "ctrl-b": [
                            "mav::SomeOtherAction",
                            {
                                "foo": "bar"
                            }
                        ]
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "bindings": {
                        "\\ a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
        KeybindUpdateOperation::Replace {
            target: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("\\ a"),
                action_name: "mav::SomeAction",
                context: None,
                action_arguments: None,
            },
            source: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("\\ b"),
                action_name: "mav::SomeOtherAction",
                context: None,
                action_arguments: Some(r#"{"foo": "bar"}"#),
            },
            target_keybind_source: KeybindSource::User,
        },
        r#"[
                {
                    "bindings": {
                        "\\ b": [
                            "mav::SomeOtherAction",
                            {
                                "foo": "bar"
                            }
                        ]
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "bindings": {
                        "\\ a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
        KeybindUpdateOperation::Replace {
            target: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("\\ a"),
                action_name: "mav::SomeAction",
                context: None,
                action_arguments: None,
            },
            source: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("\\ a"),
                action_name: "mav::SomeAction",
                context: None,
                action_arguments: None,
            },
            target_keybind_source: KeybindSource::User,
        },
        r#"[
                {
                    "bindings": {
                        "\\ a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
    );

    check_keymap_update(
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                }
            ]"#
        .unindent(),
        KeybindUpdateOperation::Replace {
            target: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("ctrl-a"),
                action_name: "mav::SomeNonexistentAction",
                context: None,
                action_arguments: None,
            },
            source: KeybindUpdateTarget {
                keystrokes: &parse_keystrokes("ctrl-b"),
                action_name: "mav::SomeOtherAction",
                context: None,
                action_arguments: None,
            },
            target_keybind_source: KeybindSource::User,
        },
        r#"[
                {
                    "bindings": {
                        "ctrl-a": "mav::SomeAction"
                    }
                },
                {
                    "bindings": {
                        "ctrl-b": "mav::SomeOtherAction"
                    }
                }
            ]"#
        .unindent(),
    );
}
