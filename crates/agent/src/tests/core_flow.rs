use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_building_request_with_pending_tools(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let _events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(ToolRequiringPermission);
            thread.add_tool(EchoTool);
            thread.send(ClientUserMessageId::new(), ["Hey!"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let permission_tool_use = LanguageModelToolUse {
        id: "tool_id_1".into(),
        name: ToolRequiringPermission::NAME.into(),
        raw_input: "{}".into(),
        input: json!({}),
        is_input_complete: true,
        thought_signature: None,
    };
    let echo_tool_use = LanguageModelToolUse {
        id: "tool_id_2".into(),
        name: EchoTool::NAME.into(),
        raw_input: json!({"text": "test"}).to_string(),
        input: json!({"text": "test"}),
        is_input_complete: true,
        thought_signature: None,
    };
    fake_model.send_last_completion_stream_text_chunk("Hi!");
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        permission_tool_use,
    ));
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        echo_tool_use.clone(),
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    // Ensure pending tools are skipped when building a request.
    let request = thread
        .read_with(cx, |thread, cx| {
            thread.build_completion_request(CompletionIntent::EditFile, cx)
        })
        .unwrap();
    assert_eq!(
        request.messages[1..],
        vec![
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Hey!".into()],
                cache: true,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![
                    MessageContent::Text("Hi!".into()),
                    MessageContent::ToolUse(echo_tool_use.clone())
                ],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec![MessageContent::ToolResult(LanguageModelToolResult {
                    tool_use_id: echo_tool_use.id.clone(),
                    tool_name: echo_tool_use.name,
                    is_error: false,
                    content: vec!["test".into()],
                    output: Some("test".into())
                })],
                cache: false,
                reasoning_details: None,
            },
        ],
    );
}

#[gpui::test]
async fn test_agent_connection(cx: &mut TestAppContext) {
    cx.update(settings::init);
    let templates = Templates::new();

    // Initialize language model system with test provider
    cx.update(|cx| {
        gpui_tokio::init(cx);

        let http_client = FakeHttpClient::with_404_response();
        let clock = Arc::new(clock::FakeSystemClock::new());
        let client = Client::new(clock, http_client, cx);
        let user_store = cx.new(|cx| UserStore::new(client.clone(), cx));
        language_model::init(cx);
        RefreshLlmTokenListener::register(client.clone(), user_store.clone(), cx);
        language_models::init(user_store, client.clone(), cx);
        LanguageModelRegistry::test(cx);
    });
    cx.executor().forbid_parking();

    // Create a project for new_thread
    let fake_fs = cx.update(|cx| fs::FakeFs::new(cx.background_executor().clone()));
    fake_fs.insert_tree(path!("/test"), json!({})).await;
    let project = Project::test(fake_fs.clone(), [Path::new("/test")], cx).await;
    let cwd = PathList::new(&[Path::new("/test")]);
    let thread_store = cx.new(|cx| ThreadStore::new(cx));

    // Create agent and connection
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, templates.clone(), fake_fs.clone(), cx));
    let connection = NativeAgentConnection(agent.clone());

    // Create a thread using new_thread
    let connection_rc = Rc::new(connection.clone());
    let acp_thread = cx
        .update(|cx| connection_rc.new_session(project, cwd, cx))
        .await
        .expect("new_thread should succeed");

    // Get the session_id from the AcpThread
    let session_id = acp_thread.read_with(cx, |thread, _| thread.session_id().clone());

    // Test model_selector returns Some
    let selector_opt = connection.model_selector(&session_id);
    assert!(
        selector_opt.is_some(),
        "agent should always support ModelSelector"
    );
    let selector = selector_opt.unwrap();

    // Test list_models
    let listed_models = cx
        .update(|cx| selector.list_models(cx))
        .await
        .expect("list_models should succeed");
    let AgentModelList::Grouped(listed_models) = listed_models else {
        panic!("Unexpected model list type");
    };
    assert!(!listed_models.is_empty(), "should have at least one model");
    assert_eq!(
        listed_models[&AgentModelGroupName("Fake".into())][0]
            .id
            .as_ref(),
        "fake/fake"
    );

    // Test selected_model returns the default
    let model = cx
        .update(|cx| selector.selected_model(cx))
        .await
        .expect("selected_model should succeed");
    let model = cx
        .update(|cx| agent.read(cx).models().model_from_id(&model.id))
        .unwrap();
    let model = model.as_fake();
    assert_eq!(model.id().0, "fake", "should return default model");

    let request = acp_thread.update(cx, |thread, cx| thread.send(vec!["abc".into()], cx));
    cx.run_until_parked();
    model.send_last_completion_stream_text_chunk("def");
    cx.run_until_parked();
    acp_thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            indoc! {"
                ## User

                abc

                ## Assistant

                def

            "}
        )
    });

    // Test cancel
    cx.update(|cx| connection.cancel(&session_id, cx));
    request.await.expect("prompt should fail gracefully");

    // Explicitly close the session and drop the ACP thread.
    cx.update(|cx| Rc::new(connection.clone()).close_session(&session_id, cx))
        .await
        .unwrap();
    drop(acp_thread);
    let result = cx
        .update(|cx| {
            acp_thread::AgentSessionClientUserMessageIds::prompt(
                &connection,
                acp_thread::ClientUserMessageId::new(),
                acp::PromptRequest::new(session_id.clone(), vec!["ghi".into()]),
                cx,
            )
        })
        .await;
    assert_eq!(
        result.as_ref().unwrap_err().to_string(),
        "Session not found",
        "unexpected result: {:?}",
        result
    );
}

#[gpui::test]
async fn test_tool_updates_to_completion(cx: &mut TestAppContext) {
    let ThreadTest { thread, model, .. } = setup(cx, TestModel::Fake).await;
    thread.update(cx, |thread, _cx| thread.add_tool(EchoTool));
    let fake_model = model.as_fake();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Echo something"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    // Simulate streaming partial input.
    let input = json!({});
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "1".into(),
            name: EchoTool::NAME.into(),
            raw_input: input.to_string(),
            input,
            is_input_complete: false,
            thought_signature: None,
        },
    ));

    // Input streaming completed
    let input = json!({ "text": "Hello!" });
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "1".into(),
            name: "echo".into(),
            raw_input: input.to_string(),
            input,
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    let tool_call = expect_tool_call(&mut events).await;
    assert_eq!(
        tool_call,
        acp::ToolCall::new("1", "Echo")
            .raw_input(json!({}))
            .meta(acp::Meta::from_iter([("tool_name".into(), "echo".into())]))
    );
    let update = expect_tool_call_update_fields(&mut events).await;
    assert_eq!(
        update,
        acp::ToolCallUpdate::new(
            "1",
            acp::ToolCallUpdateFields::new()
                .title("Echo")
                .kind(acp::ToolKind::Other)
                .raw_input(json!({ "text": "Hello!"}))
        )
    );
    let update = expect_tool_call_update_fields(&mut events).await;
    assert_eq!(
        update,
        acp::ToolCallUpdate::new(
            "1",
            acp::ToolCallUpdateFields::new().status(acp::ToolCallStatus::InProgress)
        )
    );
    let update = expect_tool_call_update_fields(&mut events).await;
    assert_eq!(
        update,
        acp::ToolCallUpdate::new(
            "1",
            acp::ToolCallUpdateFields::new()
                .status(acp::ToolCallStatus::Completed)
                .raw_output("Hello!")
        )
    );
}
