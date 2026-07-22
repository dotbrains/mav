use super::*;

#[test]
fn test_keymap_remove() {
    zlog::init_test();

    check_keymap_update(
        r#"
            [
              {
                "context": "Editor",
                "bindings": {
                  "cmd-k cmd-u": "editor::ConvertToUpperCase",
                  "cmd-k cmd-l": "editor::ConvertToLowerCase",
                  "cmd-[": "pane::GoBack",
                }
              },
            ]
            "#,
        KeybindUpdateOperation::Remove {
            target: KeybindUpdateTarget {
                context: Some("Editor"),
                keystrokes: &parse_keystrokes("cmd-k cmd-l"),
                action_name: "editor::ConvertToLowerCase",
                action_arguments: None,
            },
            target_keybind_source: KeybindSource::User,
        },
        r#"
            [
              {
                "context": "Editor",
                "bindings": {
                  "cmd-k cmd-u": "editor::ConvertToUpperCase",
                  "cmd-[": "pane::GoBack",
                }
              },
            ]
            "#,
    );
}
