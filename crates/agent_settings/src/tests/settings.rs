use gpui::UpdateGlobal;
use settings::{Settings, SettingsStore};

use crate::AgentSettings;

#[gpui::test]
fn test_terminal_init_command_filters_empty_without_trimming(cx: &mut gpui::App) {
    let store = SettingsStore::test(cx);
    cx.set_global(store);
    project::DisableAiSettings::register(cx);
    AgentSettings::register(cx);

    SettingsStore::update_global(cx, |store, cx| {
        let new_text = store
            .new_text_for_update("{}".to_string(), |settings| {
                settings.agent.get_or_insert_default().terminal_init_command =
                    Some(" claude --resume ".to_string());
            })
            .unwrap();
        assert!(
            new_text.contains(r#""terminal_init_command": " claude --resume ""#),
            "updated settings JSON should include terminal_init_command, got {new_text}"
        );
        store.set_user_settings(&new_text, cx).unwrap();
    });
    assert_eq!(
        AgentSettings::get_global(cx)
            .terminal_init_command
            .as_deref(),
        Some(" claude --resume ")
    );

    SettingsStore::update_global(cx, |store, cx| {
        store
            .set_user_settings(r#"{ "agent": { "terminal_init_command": "   " } }"#, cx)
            .unwrap();
    });
    assert!(
        AgentSettings::get_global(cx)
            .terminal_init_command
            .is_none()
    );

    SettingsStore::update_global(cx, |store, cx| {
        store
            .set_user_settings(r#"{ "agent": { "terminal_init_command": null } }"#, cx)
            .unwrap();
    });
    assert!(
        AgentSettings::get_global(cx)
            .terminal_init_command
            .is_none()
    );
}
