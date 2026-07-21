use super::*;

#[gpui::test(iterations = 10)]
async fn test_checkpoints(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/test"),
        json!({
            ".git": {}
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;

    let simulate_changes = Arc::new(AtomicBool::new(true));
    let next_filename = Arc::new(AtomicUsize::new(0));
    let connection = Rc::new(FakeAgentConnection::new().on_user_message({
        let simulate_changes = simulate_changes.clone();
        let next_filename = next_filename.clone();
        let fs = fs.clone();
        move |request, thread, mut cx| {
            let fs = fs.clone();
            let simulate_changes = simulate_changes.clone();
            let next_filename = next_filename.clone();
            async move {
                if simulate_changes.load(SeqCst) {
                    let filename = format!("/test/file-{}", next_filename.fetch_add(1, SeqCst));
                    fs.write(Path::new(&filename), b"").await?;
                }

                let acp::ContentBlock::Text(content) = &request.prompt[0] else {
                    panic!("expected text content block");
                };
                thread.update(&mut cx, |thread, cx| {
                    thread
                        .handle_session_update(
                            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                                content.text.to_uppercase().into(),
                            )),
                            cx,
                        )
                        .unwrap();
                })?;
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

    cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["Lorem".into()], cx)))
        .await
        .unwrap();
    thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            indoc! {"
                ## User (checkpoint)

                Lorem

                ## Assistant

                LOREM

            "}
        );
    });
    assert_eq!(fs.files(), vec![Path::new(path!("/test/file-0"))]);

    cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["ipsum".into()], cx)))
        .await
        .unwrap();
    thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            indoc! {"
                ## User (checkpoint)

                Lorem

                ## Assistant

                LOREM

                ## User (checkpoint)

                ipsum

                ## Assistant

                IPSUM

            "}
        );
    });
    assert_eq!(
        fs.files(),
        vec![
            Path::new(path!("/test/file-0")),
            Path::new(path!("/test/file-1"))
        ]
    );

    // Checkpoint isn't stored when there are no changes.
    simulate_changes.store(false, SeqCst);
    cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["dolor".into()], cx)))
        .await
        .unwrap();
    thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            indoc! {"
                ## User (checkpoint)

                Lorem

                ## Assistant

                LOREM

                ## User (checkpoint)

                ipsum

                ## Assistant

                IPSUM

                ## User

                dolor

                ## Assistant

                DOLOR

            "}
        );
    });
    assert_eq!(
        fs.files(),
        vec![
            Path::new(path!("/test/file-0")),
            Path::new(path!("/test/file-1"))
        ]
    );

    // Rewinding the conversation truncates the history and restores the checkpoint.
    thread
        .update(cx, |thread, cx| {
            let AgentThreadEntry::UserMessage(message) = &thread.entries[2] else {
                panic!("unexpected entries {:?}", thread.entries)
            };
            thread.restore_checkpoint(message.client_id.clone().unwrap(), cx)
        })
        .await
        .unwrap();
    thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            indoc! {"
                ## User (checkpoint)

                Lorem

                ## Assistant

                LOREM

            "}
        );
    });
    assert_eq!(fs.files(), vec![Path::new(path!("/test/file-0"))]);
}

#[gpui::test(iterations = 10)]
async fn test_checkpoint_shows_when_file_changes_during_pending_message(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/test"),
        json!({
            ".git": {}
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/test").as_ref()], cx).await;

    let (request_started_tx, request_started_rx) = oneshot::channel::<()>();
    let request_started_tx = Rc::new(RefCell::new(Some(request_started_tx)));
    let (write_file_tx, write_file_rx) = oneshot::channel::<()>();
    let write_file_rx = Rc::new(RefCell::new(Some(write_file_rx)));
    let (file_written_tx, file_written_rx) = oneshot::channel::<()>();
    let file_written_tx = Rc::new(RefCell::new(Some(file_written_tx)));
    let (finish_response_tx, finish_response_rx) = oneshot::channel::<()>();
    let finish_response_tx = Rc::new(RefCell::new(Some(finish_response_tx)));
    let finish_response_rx = Rc::new(RefCell::new(Some(finish_response_rx)));
    let connection = Rc::new(FakeAgentConnection::new().on_user_message({
        let request_started_tx = request_started_tx.clone();
        let write_file_rx = write_file_rx.clone();
        let file_written_tx = file_written_tx.clone();
        let finish_response_rx = finish_response_rx.clone();
        move |_request, thread, mut cx| {
            let write_file_rx = write_file_rx.borrow_mut().take();
            let finish_response_rx = finish_response_rx.borrow_mut().take();
            let request_started_tx = request_started_tx.borrow_mut().take();
            let file_written_tx = file_written_tx.borrow_mut().take();
            async move {
                if let Some(request_started_tx) = request_started_tx {
                    request_started_tx.send(()).ok();
                }
                if let Some(write_file_rx) = write_file_rx {
                    write_file_rx.await.ok();
                }

                thread
                    .update(&mut cx, |thread, cx| {
                        thread.write_text_file(
                            PathBuf::from(path!("/test/file")),
                            String::new(),
                            cx,
                        )
                    })?
                    .await?;

                if let Some(file_written_tx) = file_written_tx {
                    file_written_tx.send(()).ok();
                }
                if let Some(finish_response_rx) = finish_response_rx {
                    finish_response_rx.await.ok();
                }

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

    let send = thread.update(cx, |thread, cx| thread.send(vec!["hello".into()], cx));
    let send_task = cx.background_executor.spawn(send);
    request_started_rx.await.unwrap();
    cx.run_until_parked();

    thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            indoc! {"
                ## User

                hello

            "}
        );
    });

    write_file_tx.send(()).ok();
    file_written_rx.await.unwrap();
    cx.run_until_parked();

    thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            indoc! {"
                ## User (checkpoint)

                hello

            "}
        );
    });

    finish_response_tx
        .borrow_mut()
        .take()
        .unwrap()
        .send(())
        .ok();
    send_task.await.unwrap();
}
