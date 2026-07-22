use super::*;

#[test]
fn can_deserialize_keymap_with_trailing_comma() {
    let json = indoc::indoc! {"[
              // Standard macOS bindings
              {
                \"bindings\": {
                  \"up\": \"menu::SelectPrevious\",
                },
              },
            ]
                  "
    };
    KeymapFile::parse(json).unwrap();
}

#[gpui::test]
fn keymap_section_unbinds_are_loaded_before_bindings(cx: &mut App) {
    let key_bindings = match KeymapFile::load(
        indoc::indoc! {r#"
                [
                    {
                        "unbind": {
                            "ctrl-a": "test_keymap_file::StringAction",
                            "ctrl-b": ["test_keymap_file::InputAction", {}]
                        },
                        "bindings": {
                            "ctrl-c": "test_keymap_file::StringAction"
                        }
                    }
                ]
            "#},
        cx,
    ) {
        crate::keymap_file::KeymapFileLoadResult::Success { key_bindings } => key_bindings,
        crate::keymap_file::KeymapFileLoadResult::SomeFailedToLoad { error_message, .. } => {
            panic!("{error_message}");
        }
        crate::keymap_file::KeymapFileLoadResult::JsonParseFailure { error } => {
            panic!("JSON parse error: {error}");
        }
    };

    assert_eq!(key_bindings.len(), 3);
    assert!(
        key_bindings[0]
            .action()
            .partial_eq(&Unbind("test_keymap_file::StringAction".into()))
    );
    assert_eq!(key_bindings[0].action_input(), None);
    assert!(
        key_bindings[1]
            .action()
            .partial_eq(&Unbind("test_keymap_file::InputAction".into()))
    );
    assert_eq!(
        key_bindings[1]
            .action_input()
            .as_ref()
            .map(ToString::to_string),
        Some("{}".to_string())
    );
    assert_eq!(
        key_bindings[2].action().name(),
        "test_keymap_file::StringAction"
    );
}

#[gpui::test]
fn keymap_unbind_loads_valid_target_action_with_input(cx: &mut App) {
    let key_bindings = match KeymapFile::load(
        indoc::indoc! {r#"
                [
                    {
                        "unbind": {
                            "ctrl-a": ["test_keymap_file::InputAction", {}]
                        }
                    }
                ]
            "#},
        cx,
    ) {
        crate::keymap_file::KeymapFileLoadResult::Success { key_bindings } => key_bindings,
        other => panic!("expected Success, got {other:?}"),
    };

    assert_eq!(key_bindings.len(), 1);
    assert!(
        key_bindings[0]
            .action()
            .partial_eq(&Unbind("test_keymap_file::InputAction".into()))
    );
    assert_eq!(
        key_bindings[0]
            .action_input()
            .as_ref()
            .map(ToString::to_string),
        Some("{}".to_string())
    );
}

#[gpui::test]
fn keymap_unbind_rejects_null(cx: &mut App) {
    match KeymapFile::load(
        indoc::indoc! {r#"
                [
                    {
                        "unbind": {
                            "ctrl-a": null
                        }
                    }
                ]
            "#},
        cx,
    ) {
        crate::keymap_file::KeymapFileLoadResult::SomeFailedToLoad {
            key_bindings,
            error_message,
        } => {
            assert!(key_bindings.is_empty());
            assert!(
                error_message
                    .0
                    .contains("expected action name string or [name, input] array.")
            );
        }
        other => panic!("expected SomeFailedToLoad, got {other:?}"),
    }
}

#[gpui::test]
fn keymap_unbind_rejects_unbind_action(cx: &mut App) {
    match KeymapFile::load(
        indoc::indoc! {r#"
                [
                    {
                        "unbind": {
                            "ctrl-a": ["mav::Unbind", "test_keymap_file::StringAction"]
                        }
                    }
                ]
            "#},
        cx,
    ) {
        crate::keymap_file::KeymapFileLoadResult::SomeFailedToLoad {
            key_bindings,
            error_message,
        } => {
            assert!(key_bindings.is_empty());
            assert!(
                error_message
                    .0
                    .contains("can't use `\"mav::Unbind\"` as an unbind target.")
            );
        }
        other => panic!("expected SomeFailedToLoad, got {other:?}"),
    }
}
