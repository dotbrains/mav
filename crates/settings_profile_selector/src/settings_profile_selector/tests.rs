use super::*;
use super::*;
use editor;
use gpui::{TestAppContext, UpdateGlobal, VisualTestContext};
use mav_actions::settings_profile_selector;
use menu::{Cancel, Confirm, SelectNext, SelectPrevious};
use project::{FakeFs, Project};
use serde_json::json;
use settings::Settings;
use theme_settings::ThemeSettings;
use workspace::{self, AppState, MultiWorkspace};

async fn init_test(
    user_settings_json: serde_json::Value,
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    cx.update(|cx| {
        let state = AppState::test(cx);
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        settings::init(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        super::init(cx);
        editor::init(cx);
        state
    });

    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store
                .set_user_settings(&user_settings_json.to_string(), cx)
                .unwrap();
        });
    });

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, ["/test".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let cx = VisualTestContext::from_window(*window, cx).into_mut();
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    cx.update(|_, cx| {
        assert!(!cx.has_global::<ActiveSettingsProfileName>());
    });

    (workspace, cx)
}

#[track_caller]
fn active_settings_profile_picker(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Entity<Picker<SettingsProfileSelectorDelegate>> {
    workspace.update(cx, |workspace, cx| {
        workspace
            .active_modal::<SettingsProfileSelector>(cx)
            .expect("settings profile selector is not open")
            .read(cx)
            .picker
            .clone()
    })
}

#[gpui::test]
async fn test_settings_profile_selector_state(cx: &mut TestAppContext) {
    let classroom_and_streaming_profile_name = "Classroom / Streaming".to_string();
    let demo_videos_profile_name = "Demo Videos".to_string();

    let user_settings_json = json!({
        "buffer_font_size": 10.0,
        "profiles": {
            classroom_and_streaming_profile_name.clone(): {
                "settings": {
                    "buffer_font_size": 20.0,
                }
            },
            demo_videos_profile_name.clone(): {
                "settings": {
                    "buffer_font_size": 15.0
                }
            }
        }
    });
    let (workspace, cx) = init_test(user_settings_json, cx).await;

    cx.dispatch_action(settings_profile_selector::Toggle);
    let picker = active_settings_profile_picker(&workspace, cx);

    picker.read_with(cx, |picker, cx| {
        assert_eq!(picker.delegate.matches.len(), 3);
        assert_eq!(picker.delegate.matches[0].string, display_name(&None));
        assert_eq!(
            picker.delegate.matches[1].string,
            classroom_and_streaming_profile_name
        );
        assert_eq!(picker.delegate.matches[2].string, demo_videos_profile_name);
        assert_eq!(picker.delegate.matches.get(3), None);

        assert_eq!(picker.delegate.selected_index, 0);
        assert_eq!(picker.delegate.selected_profile_name, None);

        assert_eq!(cx.try_global::<ActiveSettingsProfileName>(), None);
        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(10.0));
    });

    cx.dispatch_action(Confirm);

    cx.update(|_, cx| {
        assert_eq!(cx.try_global::<ActiveSettingsProfileName>(), None);
    });

    cx.dispatch_action(settings_profile_selector::Toggle);
    let picker = active_settings_profile_picker(&workspace, cx);
    cx.dispatch_action(SelectNext);

    picker.read_with(cx, |picker, cx| {
        assert_eq!(picker.delegate.selected_index, 1);
        assert_eq!(
            picker.delegate.selected_profile_name,
            Some(classroom_and_streaming_profile_name.clone())
        );

        assert_eq!(
            cx.try_global::<ActiveSettingsProfileName>()
                .map(|p| p.0.clone()),
            Some(classroom_and_streaming_profile_name.clone())
        );

        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(20.0));
    });

    cx.dispatch_action(Cancel);

    cx.update(|_, cx| {
        assert_eq!(cx.try_global::<ActiveSettingsProfileName>(), None);
        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(10.0));
    });

    cx.dispatch_action(settings_profile_selector::Toggle);
    let picker = active_settings_profile_picker(&workspace, cx);

    cx.dispatch_action(SelectNext);

    picker.read_with(cx, |picker, cx| {
        assert_eq!(picker.delegate.selected_index, 1);
        assert_eq!(
            picker.delegate.selected_profile_name,
            Some(classroom_and_streaming_profile_name.clone())
        );

        assert_eq!(
            cx.try_global::<ActiveSettingsProfileName>()
                .map(|p| p.0.clone()),
            Some(classroom_and_streaming_profile_name.clone())
        );

        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(20.0));
    });

    cx.dispatch_action(SelectNext);

    picker.read_with(cx, |picker, cx| {
        assert_eq!(picker.delegate.selected_index, 2);
        assert_eq!(
            picker.delegate.selected_profile_name,
            Some(demo_videos_profile_name.clone())
        );

        assert_eq!(
            cx.try_global::<ActiveSettingsProfileName>()
                .map(|p| p.0.clone()),
            Some(demo_videos_profile_name.clone())
        );

        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(15.0));
    });

    cx.dispatch_action(Confirm);

    cx.update(|_, cx| {
        assert_eq!(
            cx.try_global::<ActiveSettingsProfileName>()
                .map(|p| p.0.clone()),
            Some(demo_videos_profile_name.clone())
        );
        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(15.0));
    });

    cx.dispatch_action(settings_profile_selector::Toggle);
    let picker = active_settings_profile_picker(&workspace, cx);

    picker.read_with(cx, |picker, cx| {
        assert_eq!(picker.delegate.selected_index, 2);
        assert_eq!(
            picker.delegate.selected_profile_name,
            Some(demo_videos_profile_name.clone())
        );

        assert_eq!(
            cx.try_global::<ActiveSettingsProfileName>()
                .map(|p| p.0.clone()),
            Some(demo_videos_profile_name.clone())
        );
        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(15.0));
    });

    cx.dispatch_action(SelectPrevious);

    picker.read_with(cx, |picker, cx| {
        assert_eq!(picker.delegate.selected_index, 1);
        assert_eq!(
            picker.delegate.selected_profile_name,
            Some(classroom_and_streaming_profile_name.clone())
        );

        assert_eq!(
            cx.try_global::<ActiveSettingsProfileName>()
                .map(|p| p.0.clone()),
            Some(classroom_and_streaming_profile_name.clone())
        );

        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(20.0));
    });

    cx.dispatch_action(Cancel);

    cx.update(|_, cx| {
        assert_eq!(
            cx.try_global::<ActiveSettingsProfileName>()
                .map(|p| p.0.clone()),
            Some(demo_videos_profile_name.clone())
        );

        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(15.0));
    });

    cx.dispatch_action(settings_profile_selector::Toggle);
    let picker = active_settings_profile_picker(&workspace, cx);

    picker.read_with(cx, |picker, cx| {
        assert_eq!(picker.delegate.selected_index, 2);
        assert_eq!(
            picker.delegate.selected_profile_name,
            Some(demo_videos_profile_name.clone())
        );

        assert_eq!(
            cx.try_global::<ActiveSettingsProfileName>()
                .map(|p| p.0.clone()),
            Some(demo_videos_profile_name)
        );

        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(15.0));
    });

    cx.dispatch_action(SelectPrevious);

    picker.read_with(cx, |picker, cx| {
        assert_eq!(picker.delegate.selected_index, 1);
        assert_eq!(
            picker.delegate.selected_profile_name,
            Some(classroom_and_streaming_profile_name.clone())
        );

        assert_eq!(
            cx.try_global::<ActiveSettingsProfileName>()
                .map(|p| p.0.clone()),
            Some(classroom_and_streaming_profile_name)
        );

        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(20.0));
    });

    cx.dispatch_action(SelectPrevious);

    picker.read_with(cx, |picker, cx| {
        assert_eq!(picker.delegate.selected_index, 0);
        assert_eq!(picker.delegate.selected_profile_name, None);

        assert_eq!(
            cx.try_global::<ActiveSettingsProfileName>()
                .map(|p| p.0.clone()),
            None
        );

        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(10.0));
    });

    cx.dispatch_action(Confirm);

    cx.update(|_, cx| {
        assert_eq!(cx.try_global::<ActiveSettingsProfileName>(), None);
        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(10.0));
    });
}

#[gpui::test]
async fn test_settings_profile_with_user_base(cx: &mut TestAppContext) {
    let user_settings_json = json!({
        "buffer_font_size": 10.0,
        "profiles": {
            "Explicit User": {
                "base": "user",
                "settings": {
                    "buffer_font_size": 20.0
                }
            },
            "Implicit User": {
                "settings": {
                    "buffer_font_size": 20.0
                }
            }
        }
    });
    let (workspace, cx) = init_test(user_settings_json, cx).await;

    // Select "Explicit User" (index 1) — profile applies on top of user settings.
    cx.dispatch_action(settings_profile_selector::Toggle);
    let picker = active_settings_profile_picker(&workspace, cx);
    cx.dispatch_action(SelectNext);

    picker.read_with(cx, |picker, cx| {
        assert_eq!(
            picker.delegate.selected_profile_name.as_deref(),
            Some("Explicit User")
        );
        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(20.0));
    });

    cx.dispatch_action(Confirm);

    // Select "Implicit User" (index 2) — no base specified, same behavior.
    cx.dispatch_action(settings_profile_selector::Toggle);
    let picker = active_settings_profile_picker(&workspace, cx);
    cx.dispatch_action(SelectNext);

    picker.read_with(cx, |picker, cx| {
        assert_eq!(
            picker.delegate.selected_profile_name.as_deref(),
            Some("Implicit User")
        );
        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(20.0));
    });

    cx.dispatch_action(Confirm);
}

#[gpui::test]
async fn test_settings_profile_with_default_base(cx: &mut TestAppContext) {
    let user_settings_json = json!({
        "buffer_font_size": 10.0,
        "profiles": {
            "Clean Slate": {
                "base": "default"
            },
            "Custom on Defaults": {
                "base": "default",
                "settings": {
                    "buffer_font_size": 30.0
                }
            }
        }
    });
    let (workspace, cx) = init_test(user_settings_json, cx).await;

    // User has buffer_font_size: 10, factory default is 15.
    cx.update(|_, cx| {
        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(10.0));
    });

    // "Clean Slate" has base: "default" with no settings overrides,
    // so we get the factory default (15), not the user's value (10).
    cx.dispatch_action(settings_profile_selector::Toggle);
    let picker = active_settings_profile_picker(&workspace, cx);
    cx.dispatch_action(SelectNext);

    picker.read_with(cx, |picker, cx| {
        assert_eq!(
            picker.delegate.selected_profile_name.as_deref(),
            Some("Clean Slate")
        );
        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(15.0));
    });

    // "Custom on Defaults" has base: "default" with buffer_font_size: 30,
    // so the profile's override (30) applies on top of the factory default,
    // not on top of the user's value (10).
    cx.dispatch_action(SelectNext);

    picker.read_with(cx, |picker, cx| {
        assert_eq!(
            picker.delegate.selected_profile_name.as_deref(),
            Some("Custom on Defaults")
        );
        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(30.0));
    });

    cx.dispatch_action(Confirm);

    cx.update(|_, cx| {
        assert_eq!(ThemeSettings::get_global(cx).buffer_font_size(cx), px(30.0));
    });
}

#[gpui::test]
async fn test_settings_profile_selector_is_in_user_configuration_order(cx: &mut TestAppContext) {
    // Must be unique names (HashMap)
    let user_settings_json = json!({
        "profiles": {
            "z": { "settings": {} },
            "e": { "settings": {} },
            "d": { "settings": {} },
            " ": { "settings": {} },
            "r": { "settings": {} },
            "u": { "settings": {} },
            "l": { "settings": {} },
            "3": { "settings": {} },
            "s": { "settings": {} },
            "!": { "settings": {} },
        }
    });
    let (workspace, cx) = init_test(user_settings_json, cx).await;

    cx.dispatch_action(settings_profile_selector::Toggle);
    let picker = active_settings_profile_picker(&workspace, cx);

    picker.read_with(cx, |picker, _| {
        assert_eq!(picker.delegate.matches.len(), 11);
        assert_eq!(picker.delegate.matches[0].string, display_name(&None));
        assert_eq!(picker.delegate.matches[1].string, "z");
        assert_eq!(picker.delegate.matches[2].string, "e");
        assert_eq!(picker.delegate.matches[3].string, "d");
        assert_eq!(picker.delegate.matches[4].string, " ");
        assert_eq!(picker.delegate.matches[5].string, "r");
        assert_eq!(picker.delegate.matches[6].string, "u");
        assert_eq!(picker.delegate.matches[7].string, "l");
        assert_eq!(picker.delegate.matches[8].string, "3");
        assert_eq!(picker.delegate.matches[9].string, "s");
        assert_eq!(picker.delegate.matches[10].string, "!");
    });
}
