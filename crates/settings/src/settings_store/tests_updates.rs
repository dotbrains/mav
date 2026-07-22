use super::tests_common::*;
use super::*;

#[track_caller]
fn check_settings_update(
    store: &mut SettingsStore,
    old_json: String,
    update: fn(&mut SettingsContent),
    expected_new_json: String,
    cx: &mut App,
) {
    store.set_user_settings(&old_json, cx).ok();
    let edits = store.edits_for_update(&old_json, update).unwrap();
    let mut new_json = old_json;
    for (range, replacement) in edits.into_iter() {
        new_json.replace_range(range, &replacement);
    }
    pretty_assertions::assert_eq!(new_json, expected_new_json);
}

#[gpui::test]
fn test_setting_store_update(cx: &mut App) {
    let mut store = SettingsStore::new(cx, &test_settings());

    // entries added and updated
    check_settings_update(
        &mut store,
        r#"{
                "languages": {
                    "JSON": {
                        "auto_indent": "syntax_aware"
                    }
                }
            }"#
        .unindent(),
        |settings| {
            settings
                .languages_mut()
                .get_mut("JSON")
                .unwrap()
                .auto_indent = Some(crate::AutoIndentMode::None);

            settings.languages_mut().insert(
                "Rust".into(),
                LanguageSettingsContent {
                    auto_indent: Some(crate::AutoIndentMode::SyntaxAware),
                    ..Default::default()
                },
            );
        },
        r#"{
                "languages": {
                    "Rust": {
                        "auto_indent": "syntax_aware"
                    },
                    "JSON": {
                        "auto_indent": "none"
                    }
                }
            }"#
        .unindent(),
        cx,
    );

    // entries removed
    check_settings_update(
        &mut store,
        r#"{
                "languages": {
                    "Rust": {
                        "language_setting_2": true
                    },
                    "JSON": {
                        "language_setting_1": false
                    }
                }
            }"#
        .unindent(),
        |settings| {
            settings.languages_mut().remove("JSON").unwrap();
        },
        r#"{
                "languages": {
                    "Rust": {
                        "language_setting_2": true
                    }
                }
            }"#
        .unindent(),
        cx,
    );

    check_settings_update(
        &mut store,
        r#"{
                "languages": {
                    "Rust": {
                        "language_setting_2": true
                    },
                    "JSON": {
                        "language_setting_1": false
                    }
                }
            }"#
        .unindent(),
        |settings| {
            settings.languages_mut().remove("Rust").unwrap();
        },
        r#"{
                "languages": {
                    "JSON": {
                        "language_setting_1": false
                    }
                }
            }"#
        .unindent(),
        cx,
    );

    // weird formatting
    check_settings_update(
        &mut store,
        r#"{
                "tabs":   { "close_position": "left", "name": "Max"  }
                }"#
        .unindent(),
        |settings| {
            settings.tabs.as_mut().unwrap().close_position = Some(ClosePosition::Left);
        },
        r#"{
                "tabs":   { "close_position": "left", "name": "Max"  }
                }"#
        .unindent(),
        cx,
    );

    // single-line formatting, other keys
    check_settings_update(
        &mut store,
        r#"{ "one": 1, "two": 2 }"#.to_owned(),
        |settings| settings.auto_update = Some(true),
        r#"{ "auto_update": true, "one": 1, "two": 2 }"#.to_owned(),
        cx,
    );

    // empty object
    check_settings_update(
        &mut store,
        r#"{
                "tabs": {}
            }"#
        .unindent(),
        |settings| settings.tabs.as_mut().unwrap().close_position = Some(ClosePosition::Left),
        r#"{
                "tabs": {
                    "close_position": "left"
                }
            }"#
        .unindent(),
        cx,
    );

    // no content
    check_settings_update(
        &mut store,
        r#""#.unindent(),
        |settings| {
            settings.tabs = Some(ItemSettingsContent {
                git_status: Some(true),
                ..Default::default()
            })
        },
        r#"{
              "tabs": {
                "git_status": true
              }
            }
            "#
        .unindent(),
        cx,
    );

    check_settings_update(
        &mut store,
        r#"{
            }
            "#
        .unindent(),
        |settings| settings.sidebar.get_or_insert_default().show_branch_name = Some(true),
        r#"{
              "sidebar": {
                "show_branch_name": true
              }
            }
            "#
        .unindent(),
        cx,
    );
}

#[gpui::test]
fn test_edits_for_update_preserves_unknown_keys(cx: &mut App) {
    let mut store = SettingsStore::new(cx, &test_settings());
    store.register_setting::<AutoUpdateSetting>();

    let old_json = r#"{
            "some_unknown_key": "should_be_preserved",
            "auto_update": false
        }"#
    .unindent();

    check_settings_update(
        &mut store,
        old_json,
        |settings| settings.auto_update = Some(true),
        r#"{
            "some_unknown_key": "should_be_preserved",
            "auto_update": true
        }"#
        .unindent(),
        cx,
    );
}

#[gpui::test]
fn test_edits_for_update_returns_error_on_invalid_json(cx: &mut App) {
    let store = SettingsStore::new(cx, &test_settings());

    let invalid_json = r#"{ this is not valid json at all !!!"#;
    let result = store.edits_for_update(invalid_json, |_| {});
    assert!(result.is_err());
}
