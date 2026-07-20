use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_subagent_tool_call_end_to_end(cx: &mut TestAppContext) {
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

    // Ensure empty threads are not saved, even if they get mutated.
    thread.update(cx, |thread, cx| {
        thread.set_model(model.clone(), cx);
    });
    cx.run_until_parked();

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

    let subagent_session_id = thread.read_with(cx, |thread, cx| {
        thread
            .running_subagent_ids(cx)
            .get(0)
            .expect("subagent thread should be running")
            .clone()
    });

    let subagent_thread = agent.read_with(cx, |agent, _cx| {
        agent
            .sessions
            .get(&subagent_session_id)
            .expect("subagent session should exist")
            .acp_thread
            .clone()
    });

    model.send_last_completion_stream_text_chunk("subagent task response");
    model.end_last_completion_stream();

    cx.run_until_parked();

    assert_eq!(
        subagent_thread.read_with(cx, |thread, cx| thread.to_markdown(cx)),
        indoc! {"
            ## User

            subagent task prompt

            ## Assistant

            subagent task response

        "}
    );

    model.send_last_completion_stream_text_chunk("Response");
    model.end_last_completion_stream();

    send.await.unwrap();

    assert_eq!(
        acp_thread.read_with(cx, |thread, cx| thread.to_markdown(cx)),
        indoc! {r#"
            ## User

            Prompt

            ## Assistant

            spawning subagent

            **Tool Call: label**
            Status: Completed

            subagent task response

            ## Assistant

            Response

        "#},
    );
}

#[gpui::test]
async fn test_subagent_tool_output_does_not_include_thinking(cx: &mut TestAppContext) {
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

    // Ensure empty threads are not saved, even if they get mutated.
    thread.update(cx, |thread, cx| {
        thread.set_model(model.clone(), cx);
    });
    cx.run_until_parked();

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

    let subagent_session_id = thread.read_with(cx, |thread, cx| {
        thread
            .running_subagent_ids(cx)
            .get(0)
            .expect("subagent thread should be running")
            .clone()
    });

    let subagent_thread = agent.read_with(cx, |agent, _cx| {
        agent
            .sessions
            .get(&subagent_session_id)
            .expect("subagent session should exist")
            .acp_thread
            .clone()
    });

    model.send_last_completion_stream_text_chunk("subagent task response 1");
    model.send_last_completion_stream_event(LanguageModelCompletionEvent::Thinking {
        text: "thinking more about the subagent task".into(),
        signature: None,
    });
    model.send_last_completion_stream_text_chunk("subagent task response 2");
    model.end_last_completion_stream();

    cx.run_until_parked();

    assert_eq!(
        subagent_thread.read_with(cx, |thread, cx| thread.to_markdown(cx)),
        indoc! {"
            ## User

            subagent task prompt

            ## Assistant

            subagent task response 1

            <thinking>
            thinking more about the subagent task
            </thinking>

            subagent task response 2

        "}
    );

    model.send_last_completion_stream_text_chunk("Response");
    model.end_last_completion_stream();

    send.await.unwrap();

    assert_eq!(
        acp_thread.read_with(cx, |thread, cx| thread.to_markdown(cx)),
        indoc! {r#"
            ## User

            Prompt

            ## Assistant

            spawning subagent

            **Tool Call: label**
            Status: Completed

            subagent task response 1

            subagent task response 2

            ## Assistant

            Response

        "#},
    );
}
