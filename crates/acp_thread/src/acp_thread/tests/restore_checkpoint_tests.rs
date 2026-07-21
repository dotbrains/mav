use super::*;

#[gpui::test]
async fn test_tool_call_not_found_creates_failed_entry(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    // Try to update a tool call that doesn't exist
    let nonexistent_id = acp::ToolCallId::new("nonexistent-tool-call");
    thread.update(cx, |thread, cx| {
        let result = thread.handle_session_update(
            acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
                nonexistent_id.clone(),
                acp::ToolCallUpdateFields::new().status(acp::ToolCallStatus::Completed),
            )),
            cx,
        );

        // The update should succeed (not return an error)
        assert!(result.is_ok());

        // There should now be exactly one entry in the thread
        assert_eq!(thread.entries.len(), 1);

        // The entry should be a failed tool call
        if let AgentThreadEntry::ToolCall(tool_call) = &thread.entries[0] {
            assert_eq!(tool_call.id, nonexistent_id);
            assert!(matches!(tool_call.status, ToolCallStatus::Failed));
            assert_eq!(tool_call.kind, acp::ToolKind::Fetch);

            // Check that the content contains the error message
            assert_eq!(tool_call.content.len(), 1);
            if let ToolCallContent::ContentBlock(content_block) = &tool_call.content[0] {
                match content_block {
                    ContentBlock::Markdown { markdown } => {
                        let markdown_text = markdown.read(cx).source();
                        assert!(markdown_text.contains("Tool call not found"));
                    }
                    ContentBlock::Empty => panic!("Expected markdown content, got empty"),
                    ContentBlock::ResourceLink { .. } => {
                        panic!("Expected markdown content, got resource link")
                    }
                    ContentBlock::EmbeddedResource { .. } => {
                        panic!("Expected markdown content, got embedded resource")
                    }
                    ContentBlock::Image { .. } => {
                        panic!("Expected markdown content, got image")
                    }
                }
            } else {
                panic!("Expected ContentBlock, got: {:?}", tool_call.content[0]);
            }
        } else {
            panic!("Expected ToolCall entry, got: {:?}", thread.entries[0]);
        }
    });
}

/// Tests that restoring a checkpoint properly cleans up terminals that were
/// created after that checkpoint, and cancels any in-progress generation.
///
/// Reproduces issue #35142: When a checkpoint is restored, any terminal processes
/// that were started after that checkpoint should be terminated, and any in-progress
/// AI generation should be canceled.
#[gpui::test]
async fn test_restore_checkpoint_kills_terminal(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    // Send first user message to create a checkpoint
    cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.send(vec!["first message".into()], cx)
        })
    })
    .await
    .unwrap();

    // Send second message (creates another checkpoint) - we'll restore to this one
    cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.send(vec!["second message".into()], cx)
        })
    })
    .await
    .unwrap();

    // Create 2 terminals BEFORE the checkpoint that have completed running
    let terminal_id_1 = acp::TerminalId::new(uuid::Uuid::new_v4().to_string());
    let mock_terminal_1 = cx.new(|cx| {
        let builder = ::terminal::TerminalBuilder::new_display_only(
            ::terminal::terminal_settings::CursorShape::default(),
            ::terminal::terminal_settings::AlternateScroll::On,
            None,
            0,
            cx.background_executor(),
            PathStyle::local(),
        );
        builder.subscribe(cx)
    });

    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Created {
                terminal_id: terminal_id_1.clone(),
                label: "echo 'first'".to_string(),
                cwd: Some(PathBuf::from("/test")),
                output_byte_limit: None,
                terminal: mock_terminal_1.clone(),
            },
            cx,
        );
    });

    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Output {
                terminal_id: terminal_id_1.clone(),
                data: b"first\n".to_vec(),
            },
            cx,
        );
    });

    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Exit {
                terminal_id: terminal_id_1.clone(),
                status: acp::TerminalExitStatus::new().exit_code(0),
            },
            cx,
        );
    });

    let terminal_id_2 = acp::TerminalId::new(uuid::Uuid::new_v4().to_string());
    let mock_terminal_2 = cx.new(|cx| {
        let builder = ::terminal::TerminalBuilder::new_display_only(
            ::terminal::terminal_settings::CursorShape::default(),
            ::terminal::terminal_settings::AlternateScroll::On,
            None,
            0,
            cx.background_executor(),
            PathStyle::local(),
        );
        builder.subscribe(cx)
    });

    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Created {
                terminal_id: terminal_id_2.clone(),
                label: "echo 'second'".to_string(),
                cwd: Some(PathBuf::from("/test")),
                output_byte_limit: None,
                terminal: mock_terminal_2.clone(),
            },
            cx,
        );
    });

    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Output {
                terminal_id: terminal_id_2.clone(),
                data: b"second\n".to_vec(),
            },
            cx,
        );
    });

    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Exit {
                terminal_id: terminal_id_2.clone(),
                status: acp::TerminalExitStatus::new().exit_code(0),
            },
            cx,
        );
    });

    // Get the second message ID to restore to
    let second_message_id = thread.read_with(cx, |thread, _| {
        // At this point we have:
        // - Index 0: First user message (with checkpoint)
        // - Index 1: Second user message (with checkpoint)
        // No assistant responses because FakeAgentConnection just returns EndTurn
        let AgentThreadEntry::UserMessage(message) = &thread.entries[1] else {
            panic!("expected user message at index 1");
        };
        message.client_id.clone().unwrap()
    });

    // Create a terminal AFTER the checkpoint we'll restore to.
    // This simulates the AI agent starting a long-running terminal command.
    let terminal_id = acp::TerminalId::new(uuid::Uuid::new_v4().to_string());
    let mock_terminal = cx.new(|cx| {
        let builder = ::terminal::TerminalBuilder::new_display_only(
            ::terminal::terminal_settings::CursorShape::default(),
            ::terminal::terminal_settings::AlternateScroll::On,
            None,
            0,
            cx.background_executor(),
            PathStyle::local(),
        );
        builder.subscribe(cx)
    });

    // Register the terminal as created
    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Created {
                terminal_id: terminal_id.clone(),
                label: "sleep 1000".to_string(),
                cwd: Some(PathBuf::from("/test")),
                output_byte_limit: None,
                terminal: mock_terminal.clone(),
            },
            cx,
        );
    });

    // Simulate the terminal producing output (still running)
    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Output {
                terminal_id: terminal_id.clone(),
                data: b"terminal is running...\n".to_vec(),
            },
            cx,
        );
    });

    // Create a tool call entry that references this terminal
    // This represents the agent requesting a terminal command
    thread.update(cx, |thread, cx| {
        thread
            .handle_session_update(
                acp::SessionUpdate::ToolCall(
                    acp::ToolCall::new("terminal-tool-1", "Running command")
                        .kind(acp::ToolKind::Execute)
                        .status(acp::ToolCallStatus::InProgress)
                        .content(vec![acp::ToolCallContent::Terminal(acp::Terminal::new(
                            terminal_id.clone(),
                        ))])
                        .raw_input(serde_json::json!({"command": "sleep 1000", "cd": "/test"})),
                ),
                cx,
            )
            .unwrap();
    });

    // Verify terminal exists and is in the thread
    let terminal_exists_before =
        thread.read_with(cx, |thread, _| thread.terminals.contains_key(&terminal_id));
    assert!(
        terminal_exists_before,
        "Terminal should exist before checkpoint restore"
    );

    // Verify the terminal's underlying task is still running (not completed)
    let terminal_running_before = thread.read_with(cx, |thread, _cx| {
        let terminal_entity = thread.terminals.get(&terminal_id).unwrap();
        terminal_entity.read_with(cx, |term, _cx| {
            term.output().is_none() // output is None means it's still running
        })
    });
    assert!(
        terminal_running_before,
        "Terminal should be running before checkpoint restore"
    );

    // Verify we have the expected entries before restore
    let entry_count_before = thread.read_with(cx, |thread, _| thread.entries.len());
    assert!(
        entry_count_before > 1,
        "Should have multiple entries before restore"
    );

    // Restore the checkpoint to the second message.
    // This should:
    // 1. Cancel any in-progress generation (via the cancel() call)
    // 2. Remove the terminal that was created after that point
    thread
        .update(cx, |thread, cx| {
            thread.restore_checkpoint(second_message_id, cx)
        })
        .await
        .unwrap();

    // Verify that no send_task is in progress after restore
    // (cancel() clears the send_task)
    let has_send_task_after = thread.read_with(cx, |thread, _| thread.running_turn.is_some());
    assert!(
        !has_send_task_after,
        "Should not have a send_task after restore (cancel should have cleared it)"
    );

    // Verify the entries were truncated (restoring to index 1 truncates at 1, keeping only index 0)
    let entry_count = thread.read_with(cx, |thread, _| thread.entries.len());
    assert_eq!(
        entry_count, 1,
        "Should have 1 entry after restore (only the first user message)"
    );

    // Verify the 2 completed terminals from before the checkpoint still exist
    let terminal_1_exists = thread.read_with(cx, |thread, _| {
        thread.terminals.contains_key(&terminal_id_1)
    });
    assert!(
        terminal_1_exists,
        "Terminal 1 (from before checkpoint) should still exist"
    );

    let terminal_2_exists = thread.read_with(cx, |thread, _| {
        thread.terminals.contains_key(&terminal_id_2)
    });
    assert!(
        terminal_2_exists,
        "Terminal 2 (from before checkpoint) should still exist"
    );

    // Verify they're still in completed state
    let terminal_1_completed = thread.read_with(cx, |thread, _cx| {
        let terminal_entity = thread.terminals.get(&terminal_id_1).unwrap();
        terminal_entity.read_with(cx, |term, _cx| term.output().is_some())
    });
    assert!(terminal_1_completed, "Terminal 1 should still be completed");

    let terminal_2_completed = thread.read_with(cx, |thread, _cx| {
        let terminal_entity = thread.terminals.get(&terminal_id_2).unwrap();
        terminal_entity.read_with(cx, |term, _cx| term.output().is_some())
    });
    assert!(terminal_2_completed, "Terminal 2 should still be completed");

    // Verify the running terminal (created after checkpoint) was removed
    let terminal_3_exists =
        thread.read_with(cx, |thread, _| thread.terminals.contains_key(&terminal_id));
    assert!(
        !terminal_3_exists,
        "Terminal 3 (created after checkpoint) should have been removed"
    );

    // Verify total count is 2 (the two from before the checkpoint)
    let terminal_count = thread.read_with(cx, |thread, _| thread.terminals.len());
    assert_eq!(
        terminal_count, 2,
        "Should have exactly 2 terminals (the completed ones from before checkpoint)"
    );
}

/// Tests that update_last_checkpoint correctly updates the original message's checkpoint
/// even when a new user message is added while the async checkpoint comparison is in progress.
///
/// This is a regression test for a bug where update_last_checkpoint would fail with
/// "no checkpoint" if a new user message (without a checkpoint) was added between when
/// update_last_checkpoint started and when its async closure ran.
#[gpui::test]
async fn test_update_last_checkpoint_with_new_message_added(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/test"), json!({".git": {}, "file.txt": "content"}))
        .await;
    let project = Project::test(fs.clone(), [Path::new(path!("/test"))], cx).await;

    let handler_done = Arc::new(AtomicBool::new(false));
    let handler_done_clone = handler_done.clone();
    let connection = Rc::new(
        FakeAgentConnection::new().on_user_message(move |_, _thread, _cx| {
            handler_done_clone.store(true, SeqCst);
            async move { Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)) }.boxed_local()
        }),
    );

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    let send_future = thread.update(cx, |thread, cx| thread.send_raw("First message", cx));
    let send_task = cx.background_executor.spawn(send_future);

    // Tick until handler completes, then a few more to let update_last_checkpoint start
    while !handler_done.load(SeqCst) {
        cx.executor().tick();
    }
    for _ in 0..5 {
        cx.executor().tick();
    }

    thread.update(cx, |thread, cx| {
        thread.push_entry(
            AgentThreadEntry::UserMessage(UserMessage {
                protocol_id: None,
                client_id: Some(ClientUserMessageId::new()),
                is_optimistic: true,
                content: ContentBlock::Empty,
                chunks: vec!["Injected message (no checkpoint)".into()],
                checkpoint: None,
                indented: false,
            }),
            cx,
        );
    });

    cx.run_until_parked();
    let result = send_task.await;

    assert!(
        result.is_ok(),
        "send should succeed even when new message added during update_last_checkpoint: {:?}",
        result.err()
    );
}
