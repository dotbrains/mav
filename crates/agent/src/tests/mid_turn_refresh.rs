use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_mid_turn_model_and_settings_refresh(cx: &mut TestAppContext) {
    let ThreadTest {
        model, thread, fs, ..
    } = setup(cx, TestModel::Fake).await;
    let fake_model_a = model.as_fake();

    thread.update(cx, |thread, _cx| {
        thread.add_tool(EchoTool);
        thread.add_tool(DelayTool);
    });

    fs.insert_file(
        paths::settings_file(),
        json!({
            "agent": {
                "profiles": {
                    "profile-a": {
                        "name": "Profile A",
                        "tools": {
                            EchoTool::NAME: true,
                            DelayTool::NAME: true,
                        }
                    },
                    "profile-b": {
                        "name": "Profile B",
                        "tools": {
                            DelayTool::NAME: true,
                        }
                    }
                }
            }
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    cx.run_until_parked();

    thread.update(cx, |thread, cx| {
        thread.set_profile(AgentProfileId("profile-a".into()), cx);
        thread.set_thinking_enabled(false, cx);
    });

    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["test mid-turn refresh"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let completions = fake_model_a.pending_completions();
    assert_eq!(completions.len(), 1);
    let first_tools = tool_names_for_completion(&completions[0]);
    assert_eq!(first_tools, vec![DelayTool::NAME, EchoTool::NAME]);
    assert!(!completions[0].thinking_allowed);

    fake_model_a.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_1".into(),
            name: "echo".into(),
            raw_input: r#"{"text":"hello"}"#.into(),
            input: json!({"text": "hello"}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model_a.end_last_completion_stream();

    let fake_model_b = Arc::new(FakeLanguageModel::with_id_and_thinking(
        "test-provider",
        "model-b",
        "Model B",
        true,
    ));
    thread.update(cx, |thread, cx| {
        thread.set_profile(AgentProfileId("profile-b".into()), cx);
        thread.set_model(fake_model_b.clone() as Arc<dyn LanguageModel>, cx);
        thread.set_thinking_enabled(true, cx);
    });

    cx.run_until_parked();

    let model_b_completions = fake_model_b.pending_completions();
    assert_eq!(
        model_b_completions.len(),
        1,
        "second request should go to model B"
    );

    let second_tools = tool_names_for_completion(&model_b_completions[0]);
    assert_eq!(second_tools, vec![DelayTool::NAME]);
    assert!(model_b_completions[0].thinking_allowed);
}
