#[gpui::test]
async fn test_save_load_thread(cx: &mut TestAppContext) {
    init_test(cx);
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

    // Ensure empty threads are not saved, even if they get mutated.
    let model = Arc::new(FakeLanguageModel::default());
    let summary_model = Arc::new(FakeLanguageModel::default());
    thread.update(cx, |thread, cx| {
        thread.set_model(model.clone(), cx);
        thread.set_summarization_model(Some(summary_model.clone()), cx);
    });
    cx.run_until_parked();
    assert_eq!(thread_entries(&thread_store, cx), vec![]);

    let send = acp_thread.update(cx, |thread, cx| {
        thread.send(
            vec![
                "What does ".into(),
                acp::ContentBlock::ResourceLink(acp::ResourceLink::new(
                    "b.md",
                    MentionUri::File {
                        abs_path: path!("/a/b.md").into(),
                    }
                    .to_uri()
                    .to_string(),
                )),
                " mean?".into(),
            ],
            cx,
        )
    });
    let send = cx.foreground_executor().spawn(send);
    cx.run_until_parked();

    model.send_last_completion_stream_text_chunk("Lorem.");
    model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        language_model::TokenUsage {
            input_tokens: 150,
            output_tokens: 75,
            ..Default::default()
        },
    ));
    model.end_last_completion_stream();
    cx.run_until_parked();
    summary_model
        .send_last_completion_stream_text_chunk(&format!("Explaining {}", path!("/a/b.md")));
    summary_model.end_last_completion_stream();

    send.await.unwrap();
    let uri = MentionUri::File {
        abs_path: path!("/a/b.md").into(),
    }
    .to_uri();
    acp_thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            formatdoc! {"
                ## User

                What does [@b.md]({uri}) mean?

                ## Assistant

                Lorem.

            "}
        )
    });

    cx.run_until_parked();

    // Set a draft prompt with rich content blocks and scroll position
    // AFTER run_until_parked, so the only save that captures these
    // changes is the one performed by close_session itself.
    let draft_blocks = vec![
        acp::ContentBlock::Text(acp::TextContent::new("Check out ")),
        acp::ContentBlock::ResourceLink(acp::ResourceLink::new("b.md", uri.to_string())),
        acp::ContentBlock::Text(acp::TextContent::new(" please")),
    ];
    acp_thread.update(cx, |thread, cx| {
        thread.set_draft_prompt(Some(draft_blocks.clone()), cx);
    });
    thread.update(cx, |thread, _cx| {
        thread.set_ui_scroll_position(Some(gpui::ListOffset {
            item_ix: 5,
            offset_in_item: gpui::px(12.5),
        }));
    });

    // Close the session so it can be reloaded from disk.
    cx.update(|cx| connection.clone().close_session(&session_id, cx))
        .await
        .unwrap();
    drop(thread);
    drop(acp_thread);
    agent.read_with(cx, |agent, _| {
        assert_eq!(agent.sessions.keys().cloned().collect::<Vec<_>>(), []);
    });

    // Ensure the thread can be reloaded from disk.
    assert_eq!(
        thread_entries(&thread_store, cx),
        vec![(
            session_id.clone(),
            format!("Explaining {}", path!("/a/b.md"))
        )]
    );
    let acp_thread = agent
        .update(cx, |agent, cx| {
            agent.open_thread(session_id.clone(), project.clone(), cx)
        })
        .await
        .unwrap();
    acp_thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            formatdoc! {"
                ## User

                What does [@b.md]({uri}) mean?

                ## Assistant

                Lorem.

            "}
        )
    });

    // Ensure the draft prompt with rich content blocks survived the round-trip.
    acp_thread.read_with(cx, |thread, _| {
        assert_eq!(thread.draft_prompt(), Some(draft_blocks.as_slice()));
    });

    // Ensure token usage survived the round-trip.
    acp_thread.read_with(cx, |thread, _| {
        let usage = thread
            .token_usage()
            .expect("token usage should be restored after reload");
        assert_eq!(usage.input_tokens, 150);
        assert_eq!(usage.output_tokens, 75);
    });

    // Ensure scroll position survived the round-trip.
    acp_thread.read_with(cx, |thread, _| {
        let scroll = thread
            .ui_scroll_position()
            .expect("scroll position should be restored after reload");
        assert_eq!(scroll.item_ix, 5);
        assert_eq!(scroll.offset_in_item, gpui::px(12.5));
    });
}

