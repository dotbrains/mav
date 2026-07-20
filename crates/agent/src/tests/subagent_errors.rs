use super::*;

#[gpui::test]
async fn test_subagent_error_propagation(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        LanguageModelRegistry::test(cx);
    });
    cx.update(|cx| {
        cx.update_flags(true, vec!["subagents".to_string()]);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/",
        json!({
            "a": {
                "b.md": "Lorem"
            }
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
    let connection = Rc::new(NativeAgentConnection(agent.clone()));

    let acp_thread = cx
        .update(|cx| {
            connection
                .clone()
                .new_session(project.clone(), PathList::new(&[Path::new("")]), cx)
        })
        .await
        .unwrap();
    let session_id = acp_thread.read_with(cx, |thread, _| thread.session_id().clone());
    let thread = agent.read_with(cx, |agent, _| {
        agent.sessions.get(&session_id).unwrap().thread.clone()
    });
    let model = Arc::new(FakeLanguageModel::default());

    thread.update(cx, |thread, cx| {
        thread.set_model(model.clone(), cx);
    });
    cx.run_until_parked();

    // Start the parent turn
    let send = acp_thread.update(cx, |thread, cx| thread.send_raw("Prompt", cx));
    cx.run_until_parked();
    model.send_last_completion_stream_text_chunk("spawning subagent");
    let subagent_tool_input = SpawnAgentToolInput {
        label: "label".to_string(),
        message: "subagent task prompt".to_string(),
        session_id: None,
    };
    let subagent_tool_use = LanguageModelToolUse {
        id: "subagent_1".into(),
        name: SpawnAgentTool::NAME.into(),
        raw_input: serde_json::to_string(&subagent_tool_input).unwrap(),
        input: serde_json::to_value(&subagent_tool_input).unwrap(),
        is_input_complete: true,
        thought_signature: None,
    };
    model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        subagent_tool_use,
    ));
    model.end_last_completion_stream();

    cx.run_until_parked();

    // Verify subagent is running
    thread.read_with(cx, |thread, cx| {
        assert!(
            !thread.running_subagent_ids(cx).is_empty(),
            "subagent should be running"
        );
    });

    // The subagent's model returns a non-retryable error
    model.send_last_completion_stream_error(LanguageModelCompletionError::PromptTooLarge {
        tokens: None,
    });

    cx.run_until_parked();

    // The subagent should no longer be running
    thread.read_with(cx, |thread, cx| {
        assert!(
            thread.running_subagent_ids(cx).is_empty(),
            "subagent should not be running after error"
        );
    });

    // The parent model should get a new completion request to respond to the tool error
    model.send_last_completion_stream_text_chunk("Response after error");
    model.end_last_completion_stream();

    send.await.unwrap();

    // Verify the parent thread shows the error in the tool call
    let markdown = acp_thread.read_with(cx, |thread, cx| thread.to_markdown(cx));
    assert!(
        markdown.contains("Status: Failed"),
        "tool call should have Failed status after model error, got:\n{markdown}"
    );
}
