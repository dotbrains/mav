use super::*;

#[gpui::test]
async fn test_terminal_tool_timeout_kills_handle(cx: &mut TestAppContext) {
    init_test(cx);
    always_allow_tools(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

    let environment = Rc::new(cx.update(|cx| {
        FakeThreadEnvironment::default().with_terminal(FakeTerminalHandle::new_never_exits(cx))
    }));
    let handle = environment.terminal_handle.clone().unwrap();

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::TerminalTool::new(project, environment));
    let (event_stream, mut rx) = crate::ToolCallEventStream::test();

    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(crate::TerminalToolInput {
                command: "sleep 1000".to_string(),
                cd: ".".to_string(),
                timeout_ms: Some(5),
                ..Default::default()
            }),
            event_stream,
            cx,
        )
    });

    let update = rx.expect_update_fields().await;
    assert!(
        update.content.iter().any(|blocks| {
            blocks
                .iter()
                .any(|c| matches!(c, acp::ToolCallContent::Terminal(_)))
        }),
        "expected tool call update to include terminal content"
    );

    let mut task_future: Pin<Box<Fuse<Task<Result<String, String>>>>> = Box::pin(task.fuse());

    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    loop {
        if let Some(result) = task_future.as_mut().now_or_never() {
            let result = result.expect("terminal tool task should complete");

            assert!(
                handle.was_killed(),
                "expected terminal handle to be killed on timeout"
            );
            assert!(
                result.contains("partial output"),
                "expected result to include terminal output, got: {result}"
            );
            return;
        }

        if std::time::Instant::now() >= deadline {
            panic!("timed out waiting for terminal tool task to complete");
        }

        cx.run_until_parked();
        cx.background_executor.timer(Duration::from_millis(1)).await;
    }
}

#[gpui::test]
#[ignore]
async fn test_terminal_tool_without_timeout_does_not_kill_handle(cx: &mut TestAppContext) {
    init_test(cx);
    always_allow_tools(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

    let environment = Rc::new(cx.update(|cx| {
        FakeThreadEnvironment::default().with_terminal(FakeTerminalHandle::new_never_exits(cx))
    }));
    let handle = environment.terminal_handle.clone().unwrap();

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::TerminalTool::new(project, environment));
    let (event_stream, mut rx) = crate::ToolCallEventStream::test();

    let _task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(crate::TerminalToolInput {
                command: "sleep 1000".to_string(),
                cd: ".".to_string(),
                timeout_ms: None,
                ..Default::default()
            }),
            event_stream,
            cx,
        )
    });

    let update = rx.expect_update_fields().await;
    assert!(
        update.content.iter().any(|blocks| {
            blocks
                .iter()
                .any(|c| matches!(c, acp::ToolCallContent::Terminal(_)))
        }),
        "expected tool call update to include terminal content"
    );

    cx.background_executor
        .timer(Duration::from_millis(25))
        .await;

    assert!(
        !handle.was_killed(),
        "did not expect terminal handle to be killed without a timeout"
    );
}
