use super::*;

#[gpui::test]
async fn test_succeeding_canceled_toolcall(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let id = acp::ToolCallId::new("test");

    let connection = Rc::new(FakeAgentConnection::new().on_user_message({
        let id = id.clone();
        move |_, thread, mut cx| {
            let id = id.clone();
            async move {
                thread
                    .update(&mut cx, |thread, cx| {
                        thread.handle_session_update(
                            acp::SessionUpdate::ToolCall(
                                acp::ToolCall::new(id.clone(), "Label")
                                    .kind(acp::ToolKind::Fetch)
                                    .status(acp::ToolCallStatus::InProgress),
                            ),
                            cx,
                        )
                    })
                    .unwrap()
                    .unwrap();
                Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
            }
            .boxed_local()
        }
    }));

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    let request = thread.update(cx, |thread, cx| {
        thread.send_raw("Fetch https://example.com", cx)
    });

    run_until_first_tool_call(&thread, cx).await;

    thread.read_with(cx, |thread, _| {
        assert!(matches!(
            thread.entries[1],
            AgentThreadEntry::ToolCall(ToolCall {
                status: ToolCallStatus::InProgress,
                ..
            })
        ));
    });

    thread.update(cx, |thread, cx| thread.cancel(cx)).await;

    thread.read_with(cx, |thread, _| {
        assert!(matches!(
            &thread.entries[1],
            AgentThreadEntry::ToolCall(ToolCall {
                status: ToolCallStatus::Canceled,
                ..
            })
        ));
    });

    thread
        .update(cx, |thread, cx| {
            thread.handle_session_update(
                acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
                    id,
                    acp::ToolCallUpdateFields::new().status(acp::ToolCallStatus::Completed),
                )),
                cx,
            )
        })
        .unwrap();

    request.await.unwrap();

    thread.read_with(cx, |thread, _| {
        assert!(matches!(
            thread.entries[1],
            AgentThreadEntry::ToolCall(ToolCall {
                status: ToolCallStatus::Completed,
                ..
            })
        ));
    });
}

#[gpui::test]
async fn test_tool_call_location_resolves_external_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/tmp/skills/test-skill"),
        json!({ "SKILL.md": "skill body" }),
    )
    .await;
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/project"))]), cx)
        })
        .await
        .unwrap();

    let skill_path = std::path::PathBuf::from(path!("/tmp/skills/test-skill/SKILL.md"));
    thread
        .update(cx, |thread, cx| {
            thread.handle_session_update(
                acp::SessionUpdate::ToolCall(
                    acp::ToolCall::new("write_file", "Write SKILL.md")
                        .kind(acp::ToolKind::Edit)
                        .status(acp::ToolCallStatus::Completed)
                        .locations(vec![acp::ToolCallLocation::new(skill_path.clone())]),
                ),
                cx,
            )
        })
        .unwrap();

    cx.run_until_parked();

    thread.read_with(cx, |thread, cx| {
        let (tool_call_location, agent_location) = thread.entries[0]
            .location(0)
            .expect("external tool-call location should resolve");
        assert_eq!(tool_call_location.path, skill_path);

        let buffer = agent_location
            .buffer
            .upgrade()
            .expect("resolved location should keep an open buffer");
        assert_eq!(buffer.read(cx).text(), "skill body");
    });
}
