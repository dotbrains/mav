use super::*;

#[gpui::test]
async fn test_data_collection_disabled_by_default(cx: &mut TestAppContext) {
    let (ep_store, _channels) = init_test_with_fake_client(cx);

    cx.update(|cx| {
        assert!(!ep_store.read(cx).is_data_collection_enabled(cx));
    });
}

#[gpui::test]
async fn test_data_collection_enabled_via_legacy_kv_store(cx: &mut TestAppContext) {
    let (ep_store, _channels) =
        init_test_with_fake_client_and_legacy_data_collection(cx, Some("true"));

    cx.update(|cx| {
        assert!(ep_store.read(cx).is_data_collection_enabled(cx));
    });
}

#[gpui::test]
async fn test_data_collection_default_uses_cached_legacy_value(cx: &mut TestAppContext) {
    let (ep_store, _channels) =
        init_test_with_fake_client_and_legacy_data_collection(cx, Some("true"));

    cx.update(|cx| {
        assert!(ep_store.read(cx).is_data_collection_enabled(cx));
    });

    cx.update(|cx| KeyValueStore::global(cx))
        .delete_kvp(MAV_PREDICT_DATA_COLLECTION_CHOICE.into())
        .await
        .unwrap();

    cx.update(|cx| {
        assert!(ep_store.read(cx).is_data_collection_enabled(cx));
    });
}

#[gpui::test]
async fn test_data_collection_setting_overrides_kv_store(cx: &mut TestAppContext) {
    let (ep_store, _channels) =
        init_test_with_fake_client_and_legacy_data_collection(cx, Some("true"));

    // An explicit false in settings.json wins over the KV store.
    cx.update_global::<SettingsStore, _>(|settings, cx| {
        settings.update_user_settings(cx, |content| {
            content
                .project
                .all_languages
                .edit_predictions
                .get_or_insert_default()
                .allow_data_collection = Some(EditPredictionDataCollectionChoice::No);
        });
    });

    cx.update(|cx| {
        assert!(!ep_store.read(cx).is_data_collection_enabled(cx));
    });
}

#[gpui::test]
async fn test_data_collection_enabled_via_setting(cx: &mut TestAppContext) {
    let (ep_store, _channels) = init_test_with_fake_client(cx);

    cx.update_global::<SettingsStore, _>(|settings, cx| {
        settings.update_user_settings(cx, |content| {
            content
                .project
                .all_languages
                .edit_predictions
                .get_or_insert_default()
                .allow_data_collection = Some(EditPredictionDataCollectionChoice::Yes);
        });
    });

    cx.update(|cx| {
        assert!(ep_store.read(cx).is_data_collection_enabled(cx));
    });
}

#[gpui::test]
async fn test_data_collection_always_enabled_for_staff(cx: &mut TestAppContext) {
    let (ep_store, _channels) = init_test_with_fake_client(cx);

    cx.update(|cx| {
        cx.set_staff(true);
        assert!(ep_store.read(cx).is_data_collection_enabled(cx));
    });
}

#[gpui::test]
async fn test_data_collection_disabled_by_organization_configuration(cx: &mut TestAppContext) {
    let (ep_store, _channels) = init_test_with_fake_client(cx);

    cx.update_global::<SettingsStore, _>(|settings, cx| {
        settings.update_user_settings(cx, |content| {
            content
                .project
                .all_languages
                .edit_predictions
                .get_or_insert_default()
                .allow_data_collection = Some(EditPredictionDataCollectionChoice::Yes);
        });
    });

    let user_store = cx.update(|cx| ep_store.read(cx).user_store.clone());
    cx.update(|cx| {
        user_store.update(cx, |user_store, cx| {
            user_store.set_current_organization_configuration_for_test(
                Arc::new(Organization {
                    id: OrganizationId("org-1".into()),
                    name: "Org 1".into(),
                    is_personal: false,
                }),
                OrganizationConfiguration {
                    is_mav_model_provider_enabled: true,
                    is_agent_thread_feedback_enabled: true,
                    is_collaboration_enabled: true,
                    edit_prediction: OrganizationEditPredictionConfiguration {
                        is_enabled: true,
                        is_feedback_enabled: false,
                    },
                },
                cx,
            );
        });

        assert!(!ep_store.read(cx).is_data_collection_enabled(cx));
    });
}

// When a user had data collection enabled via the legacy KV store (with no explicit
// setting in settings.json), toggle_data_collection must read the *resolved* state
// (true) and write Some(false).
#[gpui::test]
async fn test_toggle_data_collection_from_kv_enabled_state(cx: &mut TestAppContext) {
    let (ep_store, _channels) =
        init_test_with_fake_client_and_legacy_data_collection(cx, Some("true"));

    cx.update(|cx| {
        assert!(
            ep_store.read(cx).is_data_collection_enabled(cx),
            "data collection should be enabled via KV store before toggle"
        );
    });

    // Simulate what toggle_data_collection does: capture the resolved current
    // state, then write its inverse.
    let is_currently_enabled = cx.update(|cx| ep_store.read(cx).is_data_collection_enabled(cx));
    cx.update_global::<SettingsStore, _>(|settings, cx| {
        settings.update_user_settings(cx, |content| {
            content
                .project
                .all_languages
                .edit_predictions
                .get_or_insert_default()
                .allow_data_collection = Some(if is_currently_enabled {
                EditPredictionDataCollectionChoice::No
            } else {
                EditPredictionDataCollectionChoice::Yes
            });
        });
    });

    cx.update(|cx| {
        assert!(
            !ep_store.read(cx).is_data_collection_enabled(cx),
            "data collection should be disabled after toggling off from KV-enabled state"
        );
    });
}

#[gpui::test]
async fn test_upsell_shown_by_default(cx: &mut TestAppContext) {
    init_test(cx);
    let kvp = cx.update(|cx| KeyValueStore::global(cx));
    kvp.delete_kvp(MAV_PREDICT_DATA_COLLECTION_CHOICE.into())
        .await
        .ok();
    kvp.delete_kvp(MavPredictUpsell::KEY.into()).await.ok();

    cx.update(|cx| assert!(should_show_upsell_modal(cx)));
}

#[gpui::test]
async fn test_upsell_dismissed_when_data_collection_choice_in_kv_store(cx: &mut TestAppContext) {
    init_test(cx);

    // Any value for the data collection key means the old upsell was already
    // shown, regardless of whether data collection was accepted or declined.
    for value in &["true", "false"] {
        cx.update(|cx| KeyValueStore::global(cx))
            .write_kvp(MAV_PREDICT_DATA_COLLECTION_CHOICE.into(), value.to_string())
            .await
            .unwrap();

        cx.update(|cx| {
            assert!(
                !should_show_upsell_modal(cx),
                "upsell should be suppressed when data collection choice is '{value}'"
            );
        });
    }

    cx.update(|cx| KeyValueStore::global(cx))
        .delete_kvp(MAV_PREDICT_DATA_COLLECTION_CHOICE.into())
        .await
        .unwrap();
}

#[gpui::test]
async fn test_upsell_dismissed_when_dismissed_key_set(cx: &mut TestAppContext) {
    init_test(cx);
    let kvp = cx.update(|cx| KeyValueStore::global(cx));
    kvp.delete_kvp(MAV_PREDICT_DATA_COLLECTION_CHOICE.into())
        .await
        .ok();
    kvp.write_kvp(MavPredictUpsell::KEY.into(), "1".into())
        .await
        .unwrap();

    cx.update(|cx| assert!(!should_show_upsell_modal(cx)));

    kvp.delete_kvp(MavPredictUpsell::KEY.into()).await.unwrap();
}

#[gpui::test]
async fn test_upsell_dismissed_via_dismissable_api(cx: &mut TestAppContext) {
    init_test(cx);
    let kvp = cx.update(|cx| KeyValueStore::global(cx));
    kvp.delete_kvp(MAV_PREDICT_DATA_COLLECTION_CHOICE.into())
        .await
        .ok();
    kvp.delete_kvp(MavPredictUpsell::KEY.into()).await.ok();

    cx.update(|cx| {
        assert!(should_show_upsell_modal(cx));
        MavPredictUpsell::set_dismissed(true, cx);
    });
    cx.run_until_parked();

    cx.update(|cx| assert!(!should_show_upsell_modal(cx)));

    kvp.delete_kvp(MavPredictUpsell::KEY.into()).await.unwrap();
}
