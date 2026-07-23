use super::*;

mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    async fn test_copilot_settings_url_with_enterprise_uri(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });

        cx.update_global(|settings_store: &mut SettingsStore, cx| {
            settings_store
                .set_user_settings(
                    r#"{"edit_predictions":{"copilot":{"enterprise_uri":"https://my-company.ghe.com"}}}"#,
                    cx,
                )
                .unwrap();
        });

        let url = cx.update(|cx| {
            let all_language_settings = all_language_settings(None, cx);
            copilot_settings_url(
                all_language_settings
                    .edit_predictions
                    .copilot
                    .enterprise_uri
                    .as_deref(),
            )
        });

        assert_eq!(url.as_ref(), "https://my-company.ghe.com/settings/copilot");
    }

    #[gpui::test]
    async fn test_copilot_settings_url_with_enterprise_uri_trailing_slash(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });

        cx.update_global(|settings_store: &mut SettingsStore, cx| {
            settings_store
                .set_user_settings(
                    r#"{"edit_predictions":{"copilot":{"enterprise_uri":"https://my-company.ghe.com/"}}}"#,
                    cx,
                )
                .unwrap();
        });

        let url = cx.update(|cx| {
            let all_language_settings = all_language_settings(None, cx);
            copilot_settings_url(
                all_language_settings
                    .edit_predictions
                    .copilot
                    .enterprise_uri
                    .as_deref(),
            )
        });

        assert_eq!(url.as_ref(), "https://my-company.ghe.com/settings/copilot");
    }

    #[gpui::test]
    async fn test_copilot_settings_url_without_enterprise_uri(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });

        let url = cx.update(|cx| {
            let all_language_settings = all_language_settings(None, cx);
            copilot_settings_url(
                all_language_settings
                    .edit_predictions
                    .copilot
                    .enterprise_uri
                    .as_deref(),
            )
        });

        assert_eq!(url.as_ref(), "https://github.com/settings/copilot");
    }
}
