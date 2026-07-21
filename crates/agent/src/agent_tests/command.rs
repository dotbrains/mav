#[gpui::test]
async fn test_compact_command_is_available(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs.clone(), [], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

    let connection = NativeAgentConnection(agent.clone());
    let acp_thread = cx
        .update(|cx| {
            Rc::new(connection.clone()).new_session(
                project.clone(),
                PathList::new(&[Path::new("/")]),
                cx,
            )
        })
        .await
        .unwrap();
    cx.run_until_parked();

    cx.update(|cx| {
        let commands = acp_thread.read(cx).available_commands();

        let compact = commands.iter().find(|command| command.name == "compact");
        let compact = compact.expect("compact command should be available");
        assert_eq!(
            acp_thread::command_category_from_meta(&compact.meta),
            Some(acp_thread::CommandCategory::Native),
        );
    });
}

#[gpui::test]
async fn test_compact_prompt_routes_to_manual_compaction(cx: &mut TestAppContext) {
    init_test(cx);
    let (connection, agent, project, acp_thread) = setup_native_agent_session(cx).await;
    let session_id = cx.update(|cx| acp_thread.read(cx).session_id().clone());
    let thread = cx.update(|cx| native_thread_for_session(&agent, &session_id, cx));
    let model = Arc::new(FakeLanguageModel::default());
    let old_message_id = ClientUserMessageId::new();

    cx.update(|cx| {
        let path_style = project.read(cx).path_style(cx);
        thread.update(cx, |thread, cx| {
            thread.set_model(model.clone(), cx);
            thread.push_acp_user_block(
                old_message_id,
                [acp::ContentBlock::from("old user")],
                path_style,
                cx,
            );
            thread.push_acp_agent_block("old assistant".into(), cx);
        });
    });

    let compact_message_id = ClientUserMessageId::new();
    let prompt_task = cx.update(|cx| {
        acp_thread::AgentSessionClientUserMessageIds::prompt(
            connection.as_ref(),
            compact_message_id,
            acp::PromptRequest::new(session_id.clone(), vec!["/compact".into()]),
            cx,
        )
    });
    cx.run_until_parked();

    let request = model.pending_completions().pop().unwrap();
    assert_eq!(
        request.intent,
        Some(CompletionIntent::ThreadContextSummarization)
    );
    assert_eq!(
        request_texts_after_system(&request.messages),
        vec![
            "old user".to_string(),
            "old assistant".to_string(),
            COMPACTION_PROMPT.to_string(),
        ]
    );

    model.send_completion_stream_text_chunk(&request, "summary");
    model.end_completion_stream(&request);
    cx.run_until_parked();
    prompt_task.await.unwrap();
}

#[gpui::test]
async fn test_threads_flushed_to_database_on_app_quit(cx: &mut TestAppContext) {
    init_test(cx);

    let (connection, agent, project, acp_thread) = setup_native_agent_session(cx).await;
    let session_id = cx.update(|cx| acp_thread.read(cx).session_id().clone());
    let thread = cx.update(|cx| native_thread_for_session(&agent, &session_id, cx));

    // A second session whose thread stays empty must be skipped by the
    // quit flush rather than persisted as an empty row.
    let empty_acp_thread = cx
        .update(|cx| {
            connection.clone().new_session(
                project.clone(),
                PathList::new(&[Path::new("/a")]),
                cx,
            )
        })
        .await
        .unwrap();
    let empty_session_id = cx.update(|cx| empty_acp_thread.read(cx).session_id().clone());

    // Give the first thread content so it's no longer an empty draft, plus
    // an in-progress draft prompt that the flush must capture.
    cx.update(|cx| {
        let path_style = project.read(cx).path_style(cx);
        thread.update(cx, |thread, cx| {
            thread.push_acp_user_block(
                ClientUserMessageId::new(),
                [acp::ContentBlock::from("hello from the user")],
                path_style,
                cx,
            );
        });
        acp_thread.update(cx, |acp_thread, cx| {
            acp_thread
                .set_draft_prompt(Some(vec![acp::ContentBlock::from("draft in progress")]), cx);
        });
    });
    cx.run_until_parked();

    // Reproduce the orphaned state from the bug: the sidebar metadata and
    // serialized panel still reference the session, but the per-session
    // async content save never landed, so the content row is absent.
    let database = cx.update(|cx| ThreadsDatabase::connect(cx)).await.unwrap();
    database.delete_thread(session_id.clone()).await.unwrap();
    assert!(
        database
            .load_thread(session_id.clone())
            .await
            .unwrap()
            .is_none(),
        "precondition: content row should be missing before the quit flush"
    );

    // Quit through the real shutdown path so the `on_app_quit`
    // registration is exercised, not just the flush itself.
    cx.update(|cx| cx.shutdown());

    let restored = database
        .load_thread(session_id.clone())
        .await
        .unwrap()
        .expect("thread content should be persisted to the database on quit");
    assert_eq!(
        restored.messages.len(),
        1,
        "the user message should survive the quit flush"
    );
    assert_eq!(
        restored.draft_prompt,
        Some(vec![acp::ContentBlock::from("draft in progress")]),
        "the current draft prompt should be captured by the quit flush"
    );
    assert!(
        database
            .load_thread(empty_session_id)
            .await
            .unwrap()
            .is_none(),
        "empty threads should not be persisted by the quit flush"
    );
}

#[test]
fn test_ambiguous_mcp_prompt_names() {
    // Reserving the built-in `/compact` forces a same-named MCP prompt to be
    // server-qualified so it stays reachable; unique names stay bare.
    let ambiguous = ambiguous_mcp_prompt_names([COMPACT_COMMAND_NAME], ["compact", "deploy"]);
    assert!(ambiguous.contains("compact"));
    assert!(!ambiguous.contains("deploy"));

    // Without the reservation, a unique MCP prompt is left bare.
    let ambiguous = ambiguous_mcp_prompt_names([], ["compact", "deploy"]);
    assert!(ambiguous.is_empty());

    // Two MCP prompts sharing a name are both qualified regardless of
    // reservation.
    let ambiguous = ambiguous_mcp_prompt_names([], ["dup", "dup", "unique"]);
    assert!(ambiguous.contains("dup"));
    assert!(!ambiguous.contains("unique"));
}

#[test]
fn test_qualified_compact_commands_are_not_native_compact() {
    let unqualified_blocks = [acp::ContentBlock::from("/compact")];
    let unqualified = Command::parse(&unqualified_blocks).unwrap();
    assert!(unqualified.is_unqualified("compact"));

    let mcp_blocks = [acp::ContentBlock::from("/server.compact")];
    let mcp_qualified = Command::parse(&mcp_blocks).unwrap();
    assert_eq!(mcp_qualified.prompt_name, "compact");
    assert_eq!(mcp_qualified.explicit_server_id, Some("server"));
    assert!(!mcp_qualified.is_unqualified("compact"));

    let skill_blocks = [acp::ContentBlock::from("/:compact")];
    let skill_qualified = Command::parse(&skill_blocks).unwrap();
    assert_eq!(skill_qualified.prompt_name, "compact");
    assert_eq!(skill_qualified.skill_scope, Some(""));
    assert!(!skill_qualified.is_unqualified("compact"));
}

