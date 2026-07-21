use super::*;
use gpui::TestAppContext;
use language_model::fake_provider::FakeLanguageModel;

#[gpui::test]
async fn test_compaction_threshold_uses_percentage_setting(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let model = Arc::new(FakeLanguageModel::default());
    let user_message_id = ClientUserMessageId::new();

    cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.set_model(model, cx);
            thread.messages.push(tests::user_text_message(
                user_message_id.clone(),
                "below limit",
            ));
            thread.request_token_usage.insert(
                user_message_id.clone(),
                language_model::TokenUsage {
                    input_tokens: 899_999,
                    ..Default::default()
                },
            );

            assert_eq!(thread.compaction_message_target_ix(cx), None);

            thread.request_token_usage.insert(
                user_message_id.clone(),
                language_model::TokenUsage {
                    input_tokens: 900_000,
                    ..Default::default()
                },
            );

            assert_eq!(thread.compaction_message_target_ix(cx), Some(1));
        });
    });
}

#[gpui::test]
async fn test_compaction_threshold_accounts_for_max_output_tokens(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let model = Arc::new(FakeLanguageModel::default());
    model.set_max_output_tokens(Some(32_000));
    let user_message_id = ClientUserMessageId::new();

    cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.set_model(model, cx);
            thread.messages.push(tests::user_text_message(
                user_message_id.clone(),
                "near input limit",
            ));
            thread.request_token_usage.insert(
                user_message_id.clone(),
                language_model::TokenUsage {
                    input_tokens: 871_199,
                    ..Default::default()
                },
            );

            assert_eq!(thread.compaction_message_target_ix(cx), None);

            thread.request_token_usage.insert(
                user_message_id.clone(),
                language_model::TokenUsage {
                    input_tokens: 871_200,
                    ..Default::default()
                },
            );

            assert_eq!(thread.compaction_message_target_ix(cx), Some(1));

            tests::set_auto_compact_settings(
                cx,
                agent_settings::AutoCompactSettings {
                    enabled: true,
                    threshold: AutoCompactThreshold::TokensRemaining(20_000),
                },
            );
            thread.request_token_usage.insert(
                user_message_id.clone(),
                language_model::TokenUsage {
                    input_tokens: 948_000,
                    ..Default::default()
                },
            );

            assert_eq!(thread.compaction_message_target_ix(cx), None);

            thread.request_token_usage.insert(
                user_message_id.clone(),
                language_model::TokenUsage {
                    input_tokens: 948_001,
                    ..Default::default()
                },
            );

            assert_eq!(thread.compaction_message_target_ix(cx), Some(1));
        });
    });
}

#[gpui::test]
async fn test_compaction_threshold_respects_enabled_setting(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let model = Arc::new(FakeLanguageModel::default());
    let user_message_id = ClientUserMessageId::new();

    cx.update(|cx| {
        tests::set_auto_compact_settings(
            cx,
            agent_settings::AutoCompactSettings {
                enabled: false,
                threshold: AutoCompactThreshold::Percentage(0.9),
            },
        );
        thread.update(cx, |thread, cx| {
            thread.set_model(model, cx);
            thread.messages.push(tests::user_text_message(
                user_message_id.clone(),
                "near limit",
            ));
            thread.request_token_usage.insert(
                user_message_id.clone(),
                language_model::TokenUsage {
                    input_tokens: 960_000,
                    ..Default::default()
                },
            );

            assert_eq!(thread.compaction_message_target_ix(cx), None);
        });
    });
}

#[gpui::test]
async fn test_compaction_threshold_respects_token_settings(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let model = Arc::new(FakeLanguageModel::default());
    let user_message_id = ClientUserMessageId::new();

    cx.update(|cx| {
        tests::set_auto_compact_settings(
            cx,
            agent_settings::AutoCompactSettings {
                enabled: true,
                threshold: AutoCompactThreshold::TokensUsed(100_000),
            },
        );
        thread.update(cx, |thread, cx| {
            thread.set_model(model, cx);
            thread.messages.push(tests::user_text_message(
                user_message_id.clone(),
                "fixed token limit",
            ));
            thread.request_token_usage.insert(
                user_message_id.clone(),
                language_model::TokenUsage {
                    input_tokens: 99_999,
                    ..Default::default()
                },
            );

            assert_eq!(thread.compaction_message_target_ix(cx), None);

            thread.request_token_usage.insert(
                user_message_id.clone(),
                language_model::TokenUsage {
                    input_tokens: 100_000,
                    ..Default::default()
                },
            );

            assert_eq!(thread.compaction_message_target_ix(cx), Some(1));

            tests::set_auto_compact_settings(
                cx,
                agent_settings::AutoCompactSettings {
                    enabled: true,
                    threshold: AutoCompactThreshold::TokensRemaining(20_000),
                },
            );
            thread.request_token_usage.insert(
                user_message_id.clone(),
                language_model::TokenUsage {
                    input_tokens: 980_000,
                    ..Default::default()
                },
            );

            assert_eq!(thread.compaction_message_target_ix(cx), None);

            thread.request_token_usage.insert(
                user_message_id.clone(),
                language_model::TokenUsage {
                    input_tokens: 980_001,
                    ..Default::default()
                },
            );

            assert_eq!(thread.compaction_message_target_ix(cx), Some(1));
        });
    });
}

#[gpui::test]
async fn test_compaction_unavailable_for_small_context_window(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let model = Arc::new(FakeLanguageModel::default());
    model.set_max_token_count(MIN_COMPACTION_CONTEXT_WINDOW - 1);
    let user_message_id = ClientUserMessageId::new();

    cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.set_model(model, cx);
            thread.messages.push(tests::user_text_message(
                user_message_id.clone(),
                "near limit",
            ));
            thread.request_token_usage.insert(
                user_message_id.clone(),
                language_model::TokenUsage {
                    input_tokens: u64::MAX,
                    ..Default::default()
                },
            );

            assert_eq!(thread.compaction_message_target_ix(cx), None);
        });
    });
}
