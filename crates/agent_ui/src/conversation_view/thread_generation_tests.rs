use super::tests::*;
use super::*;

#[gpui::test]
async fn test_rewind_views(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            "test1.txt": "old content 1",
            "test2.txt": "old content 2"
        }),
    )
    .await;
    let project = Project::test(fs, [Path::new("/project")], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
    let connection_store =
        cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

    let connection = Rc::new(StubAgentConnection::new());
    let conversation_view = cx.update(|window, cx| {
        cx.new(|cx| {
            ConversationView::new(
                Rc::new(StubAgentServer::new(connection.as_ref().clone())),
                connection_store,
                Agent::Custom { id: "Test".into() },
                None,
                None,
                None,
                None,
                None,
                workspace.downgrade(),
                project.clone(),
                Some(thread_store.clone()),
                AgentThreadSource::AgentPanel,
                window,
                cx,
            )
        })
    });

    cx.run_until_parked();

    let thread = conversation_view
        .read_with(cx, |view, cx| {
            view.active_thread().map(|r| r.read(cx).thread.clone())
        })
        .unwrap();

    // First user message
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(
        acp::ToolCall::new("tool1", "Edit file 1")
            .kind(acp::ToolKind::Edit)
            .status(acp::ToolCallStatus::Completed)
            .content(vec![acp::ToolCallContent::Diff(
                acp::Diff::new("/project/test1.txt", "new content 1").old_text("old content 1"),
            )]),
    )]);

    thread
        .update(cx, |thread, cx| thread.send_raw("Give me a diff", cx))
        .await
        .unwrap();
    cx.run_until_parked();

    thread.read_with(cx, |thread, _cx| {
        assert_eq!(thread.entries().len(), 2);
    });

    conversation_view.read_with(cx, |view, cx| {
        let entry_view_state = view
            .active_thread()
            .map(|active| active.read(cx).entry_view_state.clone())
            .unwrap();
        entry_view_state.read_with(cx, |entry_view_state, _| {
            assert!(
                entry_view_state
                    .entry(0)
                    .unwrap()
                    .message_editor()
                    .is_some()
            );
            assert!(entry_view_state.entry(1).unwrap().has_content());
        });
    });

    // Second user message
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(
        acp::ToolCall::new("tool2", "Edit file 2")
            .kind(acp::ToolKind::Edit)
            .status(acp::ToolCallStatus::Completed)
            .content(vec![acp::ToolCallContent::Diff(
                acp::Diff::new("/project/test2.txt", "new content 2").old_text("old content 2"),
            )]),
    )]);

    thread
        .update(cx, |thread, cx| thread.send_raw("Another one", cx))
        .await
        .unwrap();
    cx.run_until_parked();

    let second_user_message_id = thread.read_with(cx, |thread, _| {
        assert_eq!(thread.entries().len(), 4);
        let AgentThreadEntry::UserMessage(user_message) = &thread.entries()[2] else {
            panic!();
        };
        user_message.client_id.clone().unwrap()
    });

    conversation_view.read_with(cx, |view, cx| {
        let entry_view_state = view
            .active_thread()
            .unwrap()
            .read(cx)
            .entry_view_state
            .clone();
        entry_view_state.read_with(cx, |entry_view_state, _| {
            assert!(
                entry_view_state
                    .entry(0)
                    .unwrap()
                    .message_editor()
                    .is_some()
            );
            assert!(entry_view_state.entry(1).unwrap().has_content());
            assert!(
                entry_view_state
                    .entry(2)
                    .unwrap()
                    .message_editor()
                    .is_some()
            );
            assert!(entry_view_state.entry(3).unwrap().has_content());
        });
    });

    // Rewind to first message
    thread
        .update(cx, |thread, cx| thread.rewind(second_user_message_id, cx))
        .await
        .unwrap();

    cx.run_until_parked();

    thread.read_with(cx, |thread, _| {
        assert_eq!(thread.entries().len(), 2);
    });

    conversation_view.read_with(cx, |view, cx| {
        let active = view.active_thread().unwrap();
        active
            .read(cx)
            .entry_view_state
            .read_with(cx, |entry_view_state, _| {
                assert!(
                    entry_view_state
                        .entry(0)
                        .unwrap()
                        .message_editor()
                        .is_some()
                );
                assert!(entry_view_state.entry(1).unwrap().has_content());

                // Old views should be dropped
                assert!(entry_view_state.entry(2).is_none());
                assert!(entry_view_state.entry(3).is_none());
            });
    });
}

#[gpui::test]
async fn test_regenerate_keeps_pending_subagent_edits(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            "file.txt": "original content"
        }),
    )
    .await;
    let project = Project::test(fs, [Path::new("/project")], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
    let connection_store =
        cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

    let connection = Rc::new(StubAgentConnection::new());
    let conversation_view = cx.update(|window, cx| {
        cx.new(|cx| {
            ConversationView::new(
                Rc::new(StubAgentServer::new(connection.as_ref().clone())),
                connection_store,
                Agent::Custom { id: "Test".into() },
                None,
                None,
                None,
                None,
                None,
                workspace.downgrade(),
                project.clone(),
                Some(thread_store.clone()),
                AgentThreadSource::AgentPanel,
                window,
                cx,
            )
        })
    });

    cx.run_until_parked();

    let thread = conversation_view
        .read_with(cx, |view, cx| {
            view.active_thread().map(|r| r.read(cx).thread.clone())
        })
        .unwrap();

    // First turn: a subagent tool call. Subagent edits never appear as
    // diffs in the parent thread's entries; they are only forwarded to the
    // parent's action log through the linked-log mechanism.
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(
        acp::ToolCall::new("spawn1", "Subagent task")
            .kind(acp::ToolKind::Other)
            .status(acp::ToolCallStatus::Completed)
            .meta(acp_thread::meta_with_tool_name("spawn_agent")),
    )]);

    thread
        .update(cx, |thread, cx| thread.send_raw("Use a subagent", cx))
        .await
        .unwrap();
    cx.run_until_parked();

    // Simulate the subagent editing a file: edits performed through a
    // child action log are forwarded to the parent thread's action log,
    // just like `Thread::new_subagent` wires it up.
    let parent_action_log = thread.read_with(cx, |thread, _| thread.action_log().clone());
    let subagent_action_log = cx.update(|_, cx| {
        cx.new(|_| {
            ActionLog::new(project.clone()).with_linked_action_log(parent_action_log.clone())
        })
    });

    let buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path("file.txt", cx).unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();
    cx.update(|_, cx| {
        subagent_action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer.set_text("edited by subagent", cx);
        });
        subagent_action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();

    parent_action_log.read_with(cx, |log, cx| {
        assert_eq!(
            log.changed_buffers(cx).count(),
            1,
            "the subagent edit should be pending review in the parent's action log"
        );
    });

    // Second turn: a plain follow-up.
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Response".into()),
    )]);
    thread
        .update(cx, |thread, cx| thread.send_raw("Follow-up", cx))
        .await
        .unwrap();
    cx.run_until_parked();

    let follow_up_ix = thread.read_with(cx, |thread, cx| {
        thread
            .entries()
            .iter()
            .position(|entry| entry.to_markdown(cx) == "## User\n\nFollow-up\n\n")
            .unwrap()
    });

    // Edit and regenerate the follow-up message.
    let user_message_editor = conversation_view.read_with(cx, |view, cx| {
        view.active_thread()
            .unwrap()
            .read(cx)
            .entry_view_state
            .read(cx)
            .entry(follow_up_ix)
            .unwrap()
            .message_editor()
            .unwrap()
            .clone()
    });
    user_message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Edited follow-up", window, cx);
    });

    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("New response".into()),
    )]);
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| {
        view.regenerate(follow_up_ix, user_message_editor.clone(), window, cx);
    });
    cx.run_until_parked();

    // The thread should have been rewound and the edited message resent.
    thread.read_with(cx, |thread, cx| {
        let entries = thread.entries();
        assert_eq!(entries.len(), 4);
        assert_eq!(
            entries[2].to_markdown(cx),
            "## User\n\nEdited follow-up\n\n"
        );
    });

    // The subagent's edits predate the regenerated prompt, so they must be
    // auto-kept rather than rejected by the rewind.
    buffer.read_with(cx, |buffer, _| {
        assert_eq!(
            buffer.text(),
            "edited by subagent",
            "pending subagent edits should be kept when regenerating a later prompt"
        );
    });
    parent_action_log.read_with(cx, |log, cx| {
        assert_eq!(
            log.changed_buffers(cx).count(),
            0,
            "the subagent edit should have been auto-kept"
        );
    });
}
