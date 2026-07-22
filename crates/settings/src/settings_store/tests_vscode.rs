use super::tests_common::*;
use super::*;

#[gpui::test]
fn test_vscode_import(cx: &mut App) {
    let mut store = SettingsStore::new(cx, &test_settings());
    store.register_setting::<DefaultLanguageSettings>();
    store.register_setting::<ItemSettings>();
    store.register_setting::<AutoUpdateSetting>();
    store.register_setting::<ThemeSettings>();

    // create settings that werent present
    check_vscode_import(
        &mut store,
        r#"{
            }
            "#
        .unindent(),
        r#" { "editor.tabSize": 37 } "#.to_owned(),
        r#"{
              "base_keymap": "VSCode",
              "minimap": {
                "show": "always"
              },
              "tab_size": 37
            }
            "#
        .unindent(),
        cx,
    );

    // persist settings that were present
    check_vscode_import(
        &mut store,
        r#"{
                "preferred_line_length": 99,
            }
            "#
        .unindent(),
        r#"{ "editor.tabSize": 42 }"#.to_owned(),
        r#"{
                "base_keymap": "VSCode",
                "minimap": {
                    "show": "always"
                },
                "tab_size": 42,
                "preferred_line_length": 99,
            }
            "#
        .unindent(),
        cx,
    );

    // don't clobber settings that aren't present in vscode
    check_vscode_import(
        &mut store,
        r#"{
                "preferred_line_length": 99,
                "tab_size": 42
            }
            "#
        .unindent(),
        r#"{}"#.to_owned(),
        r#"{
                "base_keymap": "VSCode",
                "minimap": {
                    "show": "always"
                },
                "preferred_line_length": 99,
                "tab_size": 42
            }
            "#
        .unindent(),
        cx,
    );

    // custom enum
    check_vscode_import(
        &mut store,
        r#"{
            }
            "#
        .unindent(),
        r#"{ "git.decorations.enabled": true }"#.to_owned(),
        r#"{
              "project_panel": {
                "git_status": true
              },
              "outline_panel": {
                "git_status": true
              },
              "base_keymap": "VSCode",
              "tabs": {
                "git_status": true
              },
              "minimap": {
                "show": "always"
              }
            }
            "#
        .unindent(),
        cx,
    );

    // explorer sort settings
    check_vscode_import(
        &mut store,
        r#"{
            }
            "#
        .unindent(),
        r#"{
              "explorer.sortOrder": "mixed",
              "explorer.sortOrderLexicographicOptions": "lower"
            }"#
        .unindent(),
        r#"{
              "project_panel": {
                "sort_mode": "mixed",
                "sort_order": "lower"
              },
              "base_keymap": "VSCode",
              "minimap": {
                "show": "always"
              }
            }
            "#
        .unindent(),
        cx,
    );

    // font-family
    check_vscode_import(
        &mut store,
        r#"{
            }
            "#
        .unindent(),
        r#"{ "editor.fontFamily": "Cascadia Code, 'Consolas', Courier New" }"#.to_owned(),
        r#"{
              "base_keymap": "VSCode",
              "minimap": {
                "show": "always"
              },
              "buffer_font_fallbacks": [
                "Consolas",
                "Courier New"
              ],
              "buffer_font_family": "Cascadia Code"
            }
            "#
        .unindent(),
        cx,
    );

    // terminal bell settings - newer accessibility setting
    check_vscode_import(
        &mut store,
        r#"{
            }
            "#
        .unindent(),
        r#"{ "accessibility.signals.terminalBell": { "sound": "on" } }"#.to_owned(),
        r#"{
              "terminal": {
                "bell": "system"
              },
              "base_keymap": "VSCode",
              "minimap": {
                "show": "always"
              }
            }
            "#
        .unindent(),
        cx,
    );

    // terminal bell settings - newer accessibility setting disabled
    check_vscode_import(
        &mut store,
        r#"{
            }
            "#
        .unindent(),
        r#"{ "accessibility.signals.terminalBell": { "sound": "off" } }"#.to_owned(),
        r#"{
              "terminal": {
                "bell": "off"
              },
              "base_keymap": "VSCode",
              "minimap": {
                "show": "always"
              }
            }
            "#
        .unindent(),
        cx,
    );

    // terminal bell settings - older enableBell setting (true)
    check_vscode_import(
        &mut store,
        r#"{
            }
            "#
        .unindent(),
        r#"{ "terminal.integrated.enableBell": true }"#.to_owned(),
        r#"{
              "terminal": {
                "bell": "system"
              },
              "base_keymap": "VSCode",
              "minimap": {
                "show": "always"
              }
            }
            "#
        .unindent(),
        cx,
    );

    // terminal bell settings - older enableBell setting (false)
    check_vscode_import(
        &mut store,
        r#"{
            }
            "#
        .unindent(),
        r#"{ "terminal.integrated.enableBell": false }"#.to_owned(),
        r#"{
              "terminal": {
                "bell": "off"
              },
              "base_keymap": "VSCode",
              "minimap": {
                "show": "always"
              }
            }
            "#
        .unindent(),
        cx,
    );

    // newer accessibility setting takes precedence over older enableBell
    check_vscode_import(
        &mut store,
        r#"{
            }
            "#
        .unindent(),
        r#"{
              "accessibility.signals.terminalBell": { "sound": "off" },
              "terminal.integrated.enableBell": true
            }"#
        .to_owned(),
        r#"{
              "terminal": {
                "bell": "off"
              },
              "base_keymap": "VSCode",
              "minimap": {
                "show": "always"
              }
            }
            "#
        .unindent(),
        cx,
    );

    // hover sticky settings
    check_vscode_import(
        &mut store,
        r#"{
            }
            "#
        .unindent(),
        r#"{
              "editor.hover.sticky": false,
              "editor.hover.hidingDelay": 500
            }"#
        .to_owned(),
        r#"{
              "base_keymap": "VSCode",
              "minimap": {
                "show": "always"
              },
              "hover_popover_hiding_delay": 500,
              "hover_popover_sticky": false
            }
            "#
        .unindent(),
        cx,
    );
}

#[track_caller]
fn check_vscode_import(
    store: &mut SettingsStore,
    old: String,
    vscode: String,
    expected: String,
    cx: &mut App,
) {
    store.set_user_settings(&old, cx).ok();
    let new = store
        .get_vscode_edits(
            old,
            &VsCodeSettings::from_str(&vscode, VsCodeSettingsSource::VsCode).unwrap(),
        )
        .unwrap();
    pretty_assertions::assert_eq!(new, expected);
}
