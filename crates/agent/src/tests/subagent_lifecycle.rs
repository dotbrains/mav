use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_subagent_tool_call_cancellation_during_task_prompt(cx: &mut TestAppContext) {
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
    let subagent_acp_thread = agent.read_with(cx, |agent, _cx| {
        agent
            .sessions
            .get(&subagent_session_id)
            .expect("subagent session should exist")
            .acp_thread
            .clone()
    });

    // model.send_last_completion_stream_text_chunk("subagent task response");
    // model.end_last_completion_stream();

    // cx.run_until_parked();

    acp_thread.update(cx, |thread, cx| thread.cancel(cx)).await;

    cx.run_until_parked();

    send.await.unwrap();

    acp_thread.read_with(cx, |thread, cx| {
        assert_eq!(thread.status(), ThreadStatus::Idle);
        assert_eq!(
            thread.to_markdown(cx),
            indoc! {"
                ## User

                Prompt

                ## Assistant

                spawning subagent

                **Tool Call: label**
                Status: Canceled

            "}
        );
    });
    subagent_acp_thread.read_with(cx, |thread, cx| {
        assert_eq!(thread.status(), ThreadStatus::Idle);
        assert_eq!(
            thread.to_markdown(cx),
            indoc! {"
                ## User

                subagent task prompt

            "}
        );
    });
}

#[gpui::test]
async fn test_subagent_tool_resume_session(cx: &mut TestAppContext) {
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

    // === First turn: create subagent ===
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

    let subagent_acp_thread = agent.read_with(cx, |agent, _cx| {
        agent
            .sessions
            .get(&subagent_session_id)
            .expect("subagent session should exist")
            .acp_thread
            .clone()
    });

    // Subagent responds
    model.send_last_completion_stream_text_chunk("first task response");
    model.end_last_completion_stream();

    cx.run_until_parked();

    // Parent model responds to complete first turn
    model.send_last_completion_stream_text_chunk("First response");
    model.end_last_completion_stream();

    send.await.unwrap();

    // Verify subagent is no longer running
    thread.read_with(cx, |thread, cx| {
        assert!(
            thread.running_subagent_ids(cx).is_empty(),
            "subagent should not be running after completion"
        );
    });

    // === Second turn: resume subagent with session_id ===
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

    // Subagent should be running again with the same session
    thread.read_with(cx, |thread, cx| {
        let running = thread.running_subagent_ids(cx);
        assert_eq!(running.len(), 1, "subagent should be running");
        assert_eq!(running[0], subagent_session_id, "should be same session");
    });

    // Subagent responds to follow-up
    model.send_last_completion_stream_text_chunk("follow-up task response");
    model.end_last_completion_stream();

    cx.run_until_parked();

    // Parent model responds to complete second turn
    model.send_last_completion_stream_text_chunk("Second response");
    model.end_last_completion_stream();

    send2.await.unwrap();

    // Verify subagent is no longer running
    thread.read_with(cx, |thread, cx| {
        assert!(
            thread.running_subagent_ids(cx).is_empty(),
            "subagent should not be running after resume completion"
        );
    });

    // Verify the subagent's acp thread has both conversation turns
    assert_eq!(
        subagent_acp_thread.read_with(cx, |thread, cx| thread.to_markdown(cx)),
        indoc! {"
            ## User

            do the first task

            ## Assistant

            first task response

            ## User

            do the follow-up task

            ## Assistant

            follow-up task response

        "}
    );
}

#[gpui::test]
async fn test_subagent_thread_inherits_parent_thread_properties(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        cx.update_flags(true, vec!["subagents".to_string()]);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/test"), json!({})).await;
    let project = Project::test(fs, [path!("/test").as_ref()], cx).await;
    let project_context = cx.new(|_cx| ProjectContext::default());
    let context_server_store = project.read_with(cx, |project, _| project.context_server_store());
    let context_server_registry =
        cx.new(|cx| ContextServerRegistry::new(context_server_store.clone(), cx));
    let model = Arc::new(FakeLanguageModel::default());

    let parent_thread = cx.new(|cx| {
        Thread::new(
            project.clone(),
            project_context,
            context_server_registry,
            Templates::new(),
            Some(model.clone()),
            cx,
        )
    });

    let subagent_thread = cx.new(|cx| Thread::new_subagent(&parent_thread, cx));
    subagent_thread.read_with(cx, |subagent_thread, cx| {
        assert!(subagent_thread.is_subagent());
        assert_eq!(subagent_thread.depth(), 1);
        assert_eq!(
            subagent_thread.model().map(|model| model.id()),
            Some(model.id())
        );
        assert_eq!(
            subagent_thread.parent_thread_id(),
            Some(parent_thread.read(cx).id().clone())
        );

        let request = subagent_thread
            .build_completion_request(CompletionIntent::UserPrompt, cx)
            .unwrap();
        assert_eq!(request.intent, Some(CompletionIntent::Subagent));
    });
}

#[gpui::test]
async fn test_subagent_thread_uses_configured_subagent_model(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/test"), json!({})).await;
    let project = Project::test(fs, [path!("/test").as_ref()], cx).await;
    let project_context = cx.new(|_cx| ProjectContext::default());
    let context_server_store = project.read_with(cx, |project, _| project.context_server_store());
    let context_server_registry =
        cx.new(|cx| ContextServerRegistry::new(context_server_store.clone(), cx));
    let parent_model = Arc::new(FakeLanguageModel::default());
    let subagent_model = Arc::new(FakeLanguageModel::with_id_and_thinking(
        "fake-corp",
        "subagent-model",
        "Subagent Model",
        true,
    ));

    cx.update(|cx| {
        LanguageModelRegistry::test(cx);

        let provider = Arc::new(
            FakeLanguageModelProvider::new(
                LanguageModelProviderId::from("fake-corp".to_string()),
                LanguageModelProviderName::from("Fake Corp".to_string()),
            )
            .with_models(vec![subagent_model.clone()]),
        );
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry.register_provider(provider, cx);
        });

        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.subagent_model = Some(LanguageModelSelection {
            provider: LanguageModelProviderSetting("fake-corp".to_string()),
            model: "subagent-model".to_string(),
            enable_thinking: true,
            effort: Some("high".to_string()),
            speed: None,
        });
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    let parent_thread = cx.new(|cx| {
        Thread::new(
            project.clone(),
            project_context,
            context_server_registry,
            Templates::new(),
            Some(parent_model.clone()),
            cx,
        )
    });

    let subagent_thread = cx.new(|cx| Thread::new_subagent(&parent_thread, cx));
    subagent_thread.read_with(cx, |subagent_thread, _cx| {
        assert_eq!(
            subagent_thread.model().map(|model| model.id()),
            Some(subagent_model.id())
        );
        assert!(subagent_thread.thinking_enabled());
        assert_eq!(subagent_thread.thinking_effort(), Some(&"high".to_string()));
    });

    parent_thread.update(cx, |parent_thread, _cx| {
        parent_thread.register_running_subagent(subagent_thread.downgrade());
    });
    parent_thread.update(cx, |parent_thread, cx| {
        parent_thread.set_model(parent_model.clone(), cx);
        parent_thread.set_thinking_enabled(false, cx);
        parent_thread.set_thinking_effort(None, cx);
    });

    subagent_thread.read_with(cx, |subagent_thread, _cx| {
        assert_eq!(
            subagent_thread.model().map(|model| model.id()),
            Some(subagent_model.id())
        );
        assert!(subagent_thread.thinking_enabled());
        assert_eq!(subagent_thread.thinking_effort(), Some(&"high".to_string()));
    });
}

#[gpui::test]
async fn test_max_subagent_depth_prevents_tool_registration(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        cx.update_flags(true, vec!["subagents".to_string()]);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/test"), json!({})).await;
    let project = Project::test(fs, [path!("/test").as_ref()], cx).await;
    let project_context = cx.new(|_cx| ProjectContext::default());
    let context_server_store = project.read_with(cx, |project, _| project.context_server_store());
    let context_server_registry =
        cx.new(|cx| ContextServerRegistry::new(context_server_store.clone(), cx));
    let model = Arc::new(FakeLanguageModel::default());
    let environment = Rc::new(cx.update(|cx| {
        FakeThreadEnvironment::default().with_terminal(FakeTerminalHandle::new_never_exits(cx))
    }));

    let deep_parent_thread = cx.new(|cx| {
        let mut thread = Thread::new(
            project.clone(),
            project_context,
            context_server_registry,
            Templates::new(),
            Some(model.clone()),
            cx,
        );
        thread.set_subagent_context(SubagentContext {
            parent_thread_id: acp::SessionId::new("parent-id"),
            depth: MAX_SUBAGENT_DEPTH - 1,
        });
        thread
    });
    let deep_subagent_thread = cx.new(|cx| {
        let mut thread = Thread::new_subagent(&deep_parent_thread, cx);
        thread.add_default_tools(environment, cx);
        thread
    });

    deep_subagent_thread.read_with(cx, |thread, _| {
        assert_eq!(thread.depth(), MAX_SUBAGENT_DEPTH);
        assert!(
            !thread.has_registered_tool(SpawnAgentTool::NAME),
            "subagent tool should not be present at max depth"
        );
    });
}
