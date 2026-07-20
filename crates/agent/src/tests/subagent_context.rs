use super::*;

#[gpui::test]
async fn test_subagent_context_window_warning(cx: &mut TestAppContext) {
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
    let subagent_session_id = thread.read_with(cx, |thread, cx| {
        thread
            .running_subagent_ids(cx)
            .get(0)
            .expect("subagent thread should be running")
            .clone()
    });

    // Send a usage update that crosses the warning threshold (80% of 1,000,000)
    model.send_last_completion_stream_text_chunk("partial work");
    model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        TokenUsage {
            input_tokens: 850_000,
            output_tokens: 0,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    ));

    cx.run_until_parked();

    // The subagent should no longer be running
    thread.read_with(cx, |thread, cx| {
        assert!(
            thread.running_subagent_ids(cx).is_empty(),
            "subagent should be stopped after context window warning"
        );
    });

    // The parent model should get a new completion request to respond to the tool error
    model.send_last_completion_stream_text_chunk("Response after warning");
    model.end_last_completion_stream();

    send.await.unwrap();

    // Verify the parent thread shows the warning error in the tool call
    let markdown = acp_thread.read_with(cx, |thread, cx| thread.to_markdown(cx));
    assert!(
        markdown.contains("nearing the end of its context window"),
        "tool output should contain context window warning message, got:\n{markdown}"
    );
    assert!(
        markdown.contains("Status: Failed"),
        "tool call should have Failed status, got:\n{markdown}"
    );

    // Verify the subagent session still exists (can be resumed)
    agent.read_with(cx, |agent, _cx| {
        assert!(
            agent.sessions.contains_key(&subagent_session_id),
            "subagent session should still exist for potential resume"
        );
    });
}

#[gpui::test]
async fn test_subagent_no_context_window_warning_when_already_at_warning(cx: &mut TestAppContext) {
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

    // === First turn: create subagent, trigger context window warning ===
    let send = acp_thread.update(cx, |thread, cx| thread.send_raw("First prompt", cx));
    cx.run_until_parked();
    model.send_last_completion_stream_text_chunk("spawning subagent");
    let subagent_tool_input = SpawnAgentToolInput {
        label: "initial task".to_string(),
        message: "do the first task".to_string(),
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

    let subagent_session_id = thread.read_with(cx, |thread, cx| {
        thread
            .running_subagent_ids(cx)
            .get(0)
            .expect("subagent thread should be running")
            .clone()
    });

    // Subagent sends a usage update that crosses the warning threshold.
    // This triggers Normal→Warning, stopping the subagent.
    model.send_last_completion_stream_text_chunk("partial work");
    model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        TokenUsage {
            input_tokens: 850_000,
            output_tokens: 0,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    ));

    cx.run_until_parked();

    // Verify the first turn was stopped with a context window warning
    thread.read_with(cx, |thread, cx| {
        assert!(
            thread.running_subagent_ids(cx).is_empty(),
            "subagent should be stopped after context window warning"
        );
    });

    // Parent model responds to complete first turn
    model.send_last_completion_stream_text_chunk("First response");
    model.end_last_completion_stream();

    send.await.unwrap();

    let markdown = acp_thread.read_with(cx, |thread, cx| thread.to_markdown(cx));
    assert!(
        markdown.contains("nearing the end of its context window"),
        "first turn should have context window warning, got:\n{markdown}"
    );

    // === Second turn: resume the same subagent (now at Warning level) ===
    let send2 = acp_thread.update(cx, |thread, cx| thread.send_raw("Follow up", cx));
    cx.run_until_parked();
    model.send_last_completion_stream_text_chunk("resuming subagent");
    let resume_tool_input = SpawnAgentToolInput {
        label: "follow-up task".to_string(),
        message: "do the follow-up task".to_string(),
        session_id: Some(subagent_session_id.clone()),
    };
    let resume_tool_use = LanguageModelToolUse {
        id: "subagent_2".into(),
        name: SpawnAgentTool::NAME.into(),
        raw_input: serde_json::to_string(&resume_tool_input).unwrap(),
        input: serde_json::to_value(&resume_tool_input).unwrap(),
        is_input_complete: true,
        thought_signature: None,
    };
    model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(resume_tool_use));
    model.end_last_completion_stream();

    cx.run_until_parked();

    // Subagent responds with tokens still at warning level (no worse).
    // Since ratio_before_prompt was already Warning, this should NOT
    // trigger the context window warning again.
    model.send_last_completion_stream_text_chunk("follow-up task response");
    model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        TokenUsage {
            input_tokens: 870_000,
            output_tokens: 0,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    ));
    model.end_last_completion_stream();

    cx.run_until_parked();

    // Parent model responds to complete second turn
    model.send_last_completion_stream_text_chunk("Second response");
    model.end_last_completion_stream();

    send2.await.unwrap();

    // The resumed subagent should have completed normally since the ratio
    // didn't transition (it was Warning before and stayed at Warning)
    let markdown = acp_thread.read_with(cx, |thread, cx| thread.to_markdown(cx));
    assert!(
        markdown.contains("follow-up task response"),
        "resumed subagent should complete normally when already at warning, got:\n{markdown}"
    );
    // The second tool call should NOT have a context window warning
    let second_tool_pos = markdown
        .find("follow-up task")
        .expect("should find follow-up tool call");
    let after_second_tool = &markdown[second_tool_pos..];
    assert!(
        !after_second_tool.contains("nearing the end of its context window"),
        "should NOT contain context window warning for resumed subagent at same level, got:\n{after_second_tool}"
    );
}
