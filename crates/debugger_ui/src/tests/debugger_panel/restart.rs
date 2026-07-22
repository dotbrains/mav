use super::*;

#[gpui::test]
async fn test_restart_request_is_not_sent_more_than_once_until_response(
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

    let session = start_debug_session(&workspace, cx, move |client| {
        client.on_request::<dap::requests::Initialize, _>(move |_, _| {
            Ok(dap::Capabilities {
                supports_restart_request: Some(true),
                ..Default::default()
            })
        });
    })
    .unwrap();

    let client = session.update(cx, |session, _| session.adapter_client().unwrap());

    let restart_count = Arc::new(AtomicUsize::new(0));

    client.on_request::<dap::requests::Restart, _>({
        let restart_count = restart_count.clone();
        move |_, _| {
            restart_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });

    // This works because the restart request sender is on the foreground thread
    // so it will start running after the gpui update stack is cleared
    session.update(cx, |session, cx| {
        session.restart(None, cx);
        session.restart(None, cx);
        session.restart(None, cx);
    });

    cx.run_until_parked();

    assert_eq!(
        restart_count.load(Ordering::SeqCst),
        1,
        "Only one restart request should be sent while a restart is in-flight"
    );

    session.update(cx, |session, cx| {
        session.restart(None, cx);
    });

    cx.run_until_parked();

    assert_eq!(
        restart_count.load(Ordering::SeqCst),
        2,
        "A second restart should be allowed after the first one completes"
    );
}
