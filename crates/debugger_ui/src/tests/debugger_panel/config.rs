use super::*;

#[gpui::test]
async fn test_debug_session_is_shutdown_when_attach_and_launch_request_fails(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    fs.insert_tree(
        path!("/project"),
        json!({
            "main.rs": "First line\nSecond line\nThird line\nFourth line",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let workspace = init_test_workspace(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(*workspace, cx);

    start_debug_session(&workspace, cx, |client| {
        client.on_request::<dap::requests::Initialize, _>(|_, _| {
            Err(ErrorResponse {
                error: Some(Message {
                    format: "failed to launch".to_string(),
                    id: 1,
                    variables: None,
                    send_telemetry: None,
                    show_user: None,
                    url: None,
                    url_label: None,
                }),
            })
        });
    })
    .ok();

    cx.run_until_parked();

    project.update(cx, |project, cx| {
        assert!(
            project.dap_store().read(cx).sessions().count() == 0,
            "Session wouldn't exist if it was shutdown"
        );
    });
}

#[gpui::test]
async fn test_we_send_arguments_from_user_config(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    fs.insert_tree(
        path!("/project"),
        json!({
            "main.rs": "First line\nSecond line\nThird line\nFourth line",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let workspace = init_test_workspace(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(*workspace, cx);
    let debug_definition = DebugTaskDefinition {
        adapter: "fake-adapter".into(),
        config: json!({
            "request": "launch",
            "program": "main.rs".to_owned(),
            "args": vec!["arg1".to_owned(), "arg2".to_owned()],
            "cwd": path!("/Random_path"),
            "env": json!({ "KEY": "VALUE" }),
        }),
        label: "test".into(),
        tcp_connection: None,
    };

    let launch_handler_called = Arc::new(AtomicBool::new(false));

    start_debug_session_with(&workspace, cx, debug_definition.clone(), {
        let launch_handler_called = launch_handler_called.clone();

        move |client| {
            let debug_definition = debug_definition.clone();
            let launch_handler_called = launch_handler_called.clone();

            client.on_request::<dap::requests::Launch, _>(move |_, args| {
                launch_handler_called.store(true, Ordering::SeqCst);

                assert_eq!(args.raw, debug_definition.config);

                Ok(())
            });
        }
    })
    .ok();

    cx.run_until_parked();

    assert!(
        launch_handler_called.load(Ordering::SeqCst),
        "Launch request handler was not called"
    );
}
