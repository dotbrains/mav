use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_truncate_first_message(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let message_id = ClientUserMessageId::new();
    thread
        .update(cx, |thread, cx| {
            thread.send(message_id.clone(), ["Hello"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.to_markdown(),
            indoc! {"
                ## User

                Hello
            "}
        );
        assert_eq!(thread.latest_token_usage(), None);
    });

    fake_model.send_last_completion_stream_text_chunk("Hey!");
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        language_model::TokenUsage {
            input_tokens: 32_000,
            output_tokens: 16_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    ));
    cx.run_until_parked();
    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.to_markdown(),
            indoc! {"
                ## User

                Hello

                ## Assistant

                Hey!
            "}
        );
        assert_eq!(
            thread.latest_token_usage(),
            Some(acp_thread::TokenUsage {
                used_tokens: 32_000 + 16_000,
                max_tokens: 1_000_000,
                max_output_tokens: None,
                input_tokens: 32_000,
                output_tokens: 16_000,
            })
        );
    });

    thread
        .update(cx, |thread, cx| thread.truncate(message_id, cx))
        .unwrap();
    cx.run_until_parked();
    thread.read_with(cx, |thread, _| {
        assert_eq!(thread.to_markdown(), "");
        assert_eq!(thread.latest_token_usage(), None);
    });

    // Ensure we can still send a new message after truncation.
    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hi"], cx)
        })
        .unwrap();
    thread.update(cx, |thread, _cx| {
        assert_eq!(
            thread.to_markdown(),
            indoc! {"
                ## User

                Hi
            "}
        );
    });
    cx.run_until_parked();
    fake_model.send_last_completion_stream_text_chunk("Ahoy!");
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        language_model::TokenUsage {
            input_tokens: 40_000,
            output_tokens: 20_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    ));
    cx.run_until_parked();
    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.to_markdown(),
            indoc! {"
                ## User

                Hi

                ## Assistant

                Ahoy!
            "}
        );

        assert_eq!(
            thread.latest_token_usage(),
            Some(acp_thread::TokenUsage {
                used_tokens: 40_000 + 20_000,
                max_tokens: 1_000_000,
                max_output_tokens: None,
                input_tokens: 40_000,
                output_tokens: 20_000,
            })
        );
    });
}

#[gpui::test]
async fn test_latest_token_usage_counts_cached_input_tokens(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let message_1_id = ClientUserMessageId::new();
    thread
        .update(cx, |thread, cx| {
            thread.send(message_1_id, ["Message 1"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_text_chunk("Response 1");
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        language_model::TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: 25,
            cache_read_input_tokens: 75,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.latest_token_usage(),
            Some(acp_thread::TokenUsage {
                used_tokens: 250,
                max_tokens: 1_000_000,
                max_output_tokens: None,
                input_tokens: 200,
                output_tokens: 50,
            })
        );
    });

    let message_2_id = ClientUserMessageId::new();
    thread
        .update(cx, |thread, cx| {
            thread.send(message_2_id.clone(), ["Message 2"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    thread.read_with(cx, |thread, _| {
        assert_eq!(thread.tokens_before_message(&message_2_id), Some(200));
    });
}

#[gpui::test]
async fn test_cumulative_token_usage(cx: &mut TestAppContext) {
    let ThreadTest {
        model,
        thread,
        project_context,
        ..
    } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread
        .update(cx, |thread, cx| {
            thread.add_tool(EchoTool);
            thread.send(ClientUserMessageId::new(), ["Use the echo tool"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    // The first request emits two cumulative snapshots; only the final values
    // must be counted, exactly once.
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        TokenUsage {
            input_tokens: 100,
            output_tokens: 10,
            ..Default::default()
        },
    ));
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            ..Default::default()
        },
    ));
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_1".into(),
            name: EchoTool::NAME.into(),
            raw_input: json!({"text": "hello"}).to_string(),
            input: json!({"text": "hello"}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    // The second request (after the tool call) is counted in addition to the first.
    fake_model.send_last_completion_stream_text_chunk("Done");
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        TokenUsage {
            input_tokens: 200,
            output_tokens: 30,
            ..Default::default()
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    let expected = TokenUsage {
        input_tokens: 300,
        output_tokens: 80,
        ..Default::default()
    };
    thread.read_with(cx, |thread, _| {
        assert_eq!(thread.cumulative_token_usage(), expected);
    });

    let db_thread = thread.read_with(cx, |thread, cx| thread.to_db(cx)).await;
    assert_eq!(db_thread.cumulative_token_usage, expected);

    cx.update(|cx| {
        LanguageModelRegistry::test(cx);
    });
    let restored = cx.update(|cx| {
        let thread = thread.read(cx);
        let project = thread.project.clone();
        let context_server_registry = thread.context_server_registry.clone();
        let templates = thread.templates.clone();
        cx.new(|cx| {
            Thread::from_db(
                acp::SessionId::new("restored"),
                db_thread,
                project,
                project_context.clone(),
                context_server_registry,
                templates,
                cx,
            )
        })
    });
    restored.read_with(cx, |thread, _| {
        assert_eq!(thread.cumulative_token_usage(), expected);
    });
}

#[gpui::test]
async fn test_cumulative_token_usage_keeps_accounted_usage_monotonic(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["hello"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        TokenUsage {
            input_tokens: 100,
            output_tokens: 10,
            ..Default::default()
        },
    ));
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        TokenUsage::default(),
    ));
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            ..Default::default()
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.cumulative_token_usage(),
            TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                ..Default::default()
            }
        );
    });
}

#[gpui::test]
async fn test_truncate_second_message(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Message 1"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_text_chunk("Message 1 response");
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        language_model::TokenUsage {
            input_tokens: 32_000,
            output_tokens: 16_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    let assert_first_message_state = |cx: &mut TestAppContext| {
        thread.clone().read_with(cx, |thread, _| {
            assert_eq!(
                thread.to_markdown(),
                indoc! {"
                    ## User

                    Message 1

                    ## Assistant

                    Message 1 response
                "}
            );

            assert_eq!(
                thread.latest_token_usage(),
                Some(acp_thread::TokenUsage {
                    used_tokens: 32_000 + 16_000,
                    max_tokens: 1_000_000,
                    max_output_tokens: None,
                    input_tokens: 32_000,
                    output_tokens: 16_000,
                })
            );
        });
    };

    assert_first_message_state(cx);

    let second_message_id = ClientUserMessageId::new();
    thread
        .update(cx, |thread, cx| {
            thread.send(second_message_id.clone(), ["Message 2"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_text_chunk("Message 2 response");
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        language_model::TokenUsage {
            input_tokens: 40_000,
            output_tokens: 20_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.to_markdown(),
            indoc! {"
                ## User

                Message 1

                ## Assistant

                Message 1 response

                ## User

                Message 2

                ## Assistant

                Message 2 response
            "}
        );

        assert_eq!(
            thread.latest_token_usage(),
            Some(acp_thread::TokenUsage {
                used_tokens: 40_000 + 20_000,
                max_tokens: 1_000_000,
                max_output_tokens: None,
                input_tokens: 40_000,
                output_tokens: 20_000,
            })
        );
    });

    thread
        .update(cx, |thread, cx| thread.truncate(second_message_id, cx))
        .unwrap();
    cx.run_until_parked();

    assert_first_message_state(cx);
}
