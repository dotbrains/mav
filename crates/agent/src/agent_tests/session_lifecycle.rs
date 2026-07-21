#[gpui::test]
async fn test_close_session_saves_thread(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/",
        json!({
            "a": {
                "file.txt": "hello"
            }
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent = cx
        .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
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

    // Send a message so the thread is non-empty (empty threads aren't saved).
    let send = acp_thread.update(cx, |thread, cx| thread.send(vec!["hello".into()], cx));
    let send = cx.foreground_executor().spawn(send);
    cx.run_until_parked();

    model.send_last_completion_stream_text_chunk("world");
    model.end_last_completion_stream();
    send.await.unwrap();
    cx.run_until_parked();

    // Set a draft prompt WITHOUT calling run_until_parked afterwards.
    // This means no observe-triggered save has run for this change.
    // The only way this data gets persisted is if close_session
    // itself performs the save.
    let draft_blocks = vec![acp::ContentBlock::Text(acp::TextContent::new(
        "unsaved draft",
    ))];
    acp_thread.update(cx, |thread, cx| {
        thread.set_draft_prompt(Some(draft_blocks.clone()), cx);
    });

    // Close the session immediately — no run_until_parked in between.
    cx.update(|cx| connection.clone().close_session(&session_id, cx))
        .await
        .unwrap();
    cx.run_until_parked();

    // Reopen and verify the draft prompt was saved.
    let reloaded = agent
        .update(cx, |agent, cx| {
            agent.open_thread(session_id.clone(), project.clone(), cx)
        })
        .await
        .unwrap();
    reloaded.read_with(cx, |thread, _| {
        assert_eq!(
            thread.draft_prompt(),
            Some(draft_blocks.as_slice()),
            "close_session must save the thread; draft prompt was lost"
        );
    });
}

#[gpui::test]
async fn test_thread_summary_releases_loaded_session(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/",
        json!({
            "a": {
                "file.txt": "hello"
            }
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent = cx
        .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
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
    let summary_model = Arc::new(FakeLanguageModel::default());
    thread.update(cx, |thread, cx| {
        thread.set_model(model.clone(), cx);
        thread.set_summarization_model(Some(summary_model.clone()), cx);
    });

    let send = acp_thread.update(cx, |thread, cx| thread.send(vec!["hello".into()], cx));
    let send = cx.foreground_executor().spawn(send);
    cx.run_until_parked();

    model.send_last_completion_stream_text_chunk("world");
    model.end_last_completion_stream();
    send.await.unwrap();
    cx.run_until_parked();

    let summary = agent.update(cx, |agent, cx| {
        agent.thread_summary(session_id.clone(), project.clone(), cx)
    });
    cx.run_until_parked();

    summary_model.send_last_completion_stream_text_chunk("summary");
    summary_model.end_last_completion_stream();

    assert_eq!(summary.await.unwrap(), "summary");
    cx.run_until_parked();

    agent.read_with(cx, |agent, _| {
        let session = agent
            .sessions
            .get(&session_id)
            .expect("thread_summary should not close the active session");
        assert_eq!(
            session.ref_count, 1,
            "thread_summary should release its temporary session reference"
        );
    });

    cx.update(|cx| connection.clone().close_session(&session_id, cx))
        .await
        .unwrap();
    cx.run_until_parked();

    agent.read_with(cx, |agent, _| {
        assert!(
            agent.sessions.is_empty(),
            "closing the active session after thread_summary should unload it"
        );
    });
}

#[gpui::test]
async fn test_loaded_sessions_keep_state_until_last_close(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/",
        json!({
            "a": {
                "file.txt": "hello"
            }
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent = cx
        .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
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

    let model = cx.update(|cx| {
        LanguageModelRegistry::read_global(cx)
            .default_model()
            .map(|default_model| default_model.model)
            .expect("default test model should be available")
    });
    let fake_model = model.as_fake();
    thread.update(cx, |thread, cx| {
        thread.set_model(model.clone(), cx);
    });

    let send = acp_thread.update(cx, |thread, cx| thread.send(vec!["hello".into()], cx));
    let send = cx.foreground_executor().spawn(send);
    cx.run_until_parked();

    fake_model.send_last_completion_stream_text_chunk("world");
    fake_model.end_last_completion_stream();
    send.await.unwrap();
    cx.run_until_parked();

    cx.update(|cx| connection.clone().close_session(&session_id, cx))
        .await
        .unwrap();
    drop(thread);
    drop(acp_thread);
    agent.read_with(cx, |agent, _| {
        assert!(agent.sessions.is_empty());
    });

    let first_loaded_thread = cx.update(|cx| {
        connection.clone().load_session(
            session_id.clone(),
            project.clone(),
            PathList::new(&[Path::new("")]),
            None,
            cx,
        )
    });
    let second_loaded_thread = cx.update(|cx| {
        connection.clone().load_session(
            session_id.clone(),
            project.clone(),
            PathList::new(&[Path::new("")]),
            None,
            cx,
        )
    });

    let first_loaded_thread = first_loaded_thread.await.unwrap();
    let second_loaded_thread = second_loaded_thread.await.unwrap();

    cx.run_until_parked();

    assert_eq!(
        first_loaded_thread.entity_id(),
        second_loaded_thread.entity_id(),
        "concurrent loads for the same session should share one AcpThread"
    );

    cx.update(|cx| connection.clone().close_session(&session_id, cx))
        .await
        .unwrap();

    agent.read_with(cx, |agent, _| {
        assert!(
            agent.sessions.contains_key(&session_id),
            "closing one loaded session should not drop shared session state"
        );
    });

    let follow_up = second_loaded_thread.update(cx, |thread, cx| {
        thread.send(vec!["still there?".into()], cx)
    });
    let follow_up = cx.foreground_executor().spawn(follow_up);
    cx.run_until_parked();

    fake_model.send_last_completion_stream_text_chunk("yes");
    fake_model.end_last_completion_stream();
    follow_up.await.unwrap();
    cx.run_until_parked();

    second_loaded_thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            formatdoc! {"
                ## User

                hello

                ## Assistant

                world

                ## User

                still there?

                ## Assistant

                yes

            "}
        );
    });

    cx.update(|cx| connection.clone().close_session(&session_id, cx))
        .await
        .unwrap();

    cx.run_until_parked();

    drop(first_loaded_thread);
    drop(second_loaded_thread);
    agent.read_with(cx, |agent, _| {
        assert!(agent.sessions.is_empty());
    });
}

