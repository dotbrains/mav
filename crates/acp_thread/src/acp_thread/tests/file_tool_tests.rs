use super::*;

#[gpui::test]
async fn test_edits_concurrently_to_user(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/tmp"), json!({"foo": "one\ntwo\nthree\n"}))
        .await;
    let project = Project::test(fs.clone(), [], cx).await;
    let (read_file_tx, read_file_rx) = oneshot::channel::<()>();
    let read_file_tx = Rc::new(RefCell::new(Some(read_file_tx)));
    let connection = Rc::new(FakeAgentConnection::new().on_user_message(
        move |_, thread, mut cx| {
            let read_file_tx = read_file_tx.clone();
            async move {
                let content = thread
                    .update(&mut cx, |thread, cx| {
                        thread.read_text_file(path!("/tmp/foo").into(), None, None, false, cx)
                    })
                    .unwrap()
                    .await
                    .unwrap();
                assert_eq!(content, "one\ntwo\nthree\n");
                read_file_tx.take().unwrap().send(()).unwrap();
                thread
                    .update(&mut cx, |thread, cx| {
                        thread.write_text_file(
                            path!("/tmp/foo").into(),
                            "one\ntwo\nthree\nfour\nfive\n".to_string(),
                            cx,
                        )
                    })
                    .unwrap()
                    .await
                    .unwrap();
                Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
            }
            .boxed_local()
        },
    ));

    let (worktree, pathbuf) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/tmp/foo"), true, cx)
        })
        .await
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree.read(cx).id(), pathbuf), cx)
        })
        .await
        .unwrap();

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/tmp"))]), cx)
        })
        .await
        .unwrap();

    let request = thread.update(cx, |thread, cx| {
        thread.send_raw("Extend the count in /tmp/foo", cx)
    });
    read_file_rx.await.ok();
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "zero\n".to_string())], None, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "zero\none\ntwo\nthree\nfour\nfive\n"
    );
    assert_eq!(
        String::from_utf8(fs.read_file_sync(path!("/tmp/foo")).unwrap()).unwrap(),
        "zero\none\ntwo\nthree\nfour\nfive\n"
    );
    request.await.unwrap();
}

#[gpui::test]
async fn test_reading_from_line(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/tmp"), json!({"foo": "one\ntwo\nthree\nfour\n"}))
        .await;
    let project = Project::test(fs.clone(), [], cx).await;
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/tmp/foo"), true, cx)
        })
        .await
        .unwrap();

    let connection = Rc::new(FakeAgentConnection::new());

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/tmp"))]), cx)
        })
        .await
        .unwrap();

    // Whole file
    let content = thread
        .update(cx, |thread, cx| {
            thread.read_text_file(path!("/tmp/foo").into(), None, None, false, cx)
        })
        .await
        .unwrap();

    assert_eq!(content, "one\ntwo\nthree\nfour\n");

    // Only start line
    let content = thread
        .update(cx, |thread, cx| {
            thread.read_text_file(path!("/tmp/foo").into(), Some(3), None, false, cx)
        })
        .await
        .unwrap();

    assert_eq!(content, "three\nfour\n");

    // Only limit
    let content = thread
        .update(cx, |thread, cx| {
            thread.read_text_file(path!("/tmp/foo").into(), None, Some(2), false, cx)
        })
        .await
        .unwrap();

    assert_eq!(content, "one\ntwo\n");

    // Range
    let content = thread
        .update(cx, |thread, cx| {
            thread.read_text_file(path!("/tmp/foo").into(), Some(2), Some(2), false, cx)
        })
        .await
        .unwrap();

    assert_eq!(content, "two\nthree\n");

    // Invalid
    let err = thread
        .update(cx, |thread, cx| {
            thread.read_text_file(path!("/tmp/foo").into(), Some(6), Some(2), false, cx)
        })
        .await
        .unwrap_err();

    assert_eq!(
        err.to_string(),
        "Invalid params: \"Attempting to read beyond the end of the file, line 5:0\""
    );
}

#[gpui::test]
async fn test_reading_empty_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/tmp"), json!({"foo": ""})).await;
    let project = Project::test(fs.clone(), [], cx).await;
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/tmp/foo"), true, cx)
        })
        .await
        .unwrap();

    let connection = Rc::new(FakeAgentConnection::new());

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/tmp"))]), cx)
        })
        .await
        .unwrap();

    // Whole file
    let content = thread
        .update(cx, |thread, cx| {
            thread.read_text_file(path!("/tmp/foo").into(), None, None, false, cx)
        })
        .await
        .unwrap();

    assert_eq!(content, "");

    // Only start line
    let content = thread
        .update(cx, |thread, cx| {
            thread.read_text_file(path!("/tmp/foo").into(), Some(1), None, false, cx)
        })
        .await
        .unwrap();

    assert_eq!(content, "");

    // Only limit
    let content = thread
        .update(cx, |thread, cx| {
            thread.read_text_file(path!("/tmp/foo").into(), None, Some(2), false, cx)
        })
        .await
        .unwrap();

    assert_eq!(content, "");

    // Range
    let content = thread
        .update(cx, |thread, cx| {
            thread.read_text_file(path!("/tmp/foo").into(), Some(1), Some(1), false, cx)
        })
        .await
        .unwrap();

    assert_eq!(content, "");

    // Invalid
    let err = thread
        .update(cx, |thread, cx| {
            thread.read_text_file(path!("/tmp/foo").into(), Some(5), Some(2), false, cx)
        })
        .await
        .unwrap_err();

    assert_eq!(
        err.to_string(),
        "Invalid params: \"Attempting to read beyond the end of the file, line 1:0\""
    );
}
#[gpui::test]
async fn test_reading_non_existing_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/tmp"), json!({})).await;
    let project = Project::test(fs.clone(), [], cx).await;
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/tmp"), true, cx)
        })
        .await
        .unwrap();

    let connection = Rc::new(FakeAgentConnection::new());

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/tmp"))]), cx)
        })
        .await
        .unwrap();

    // Out of project file
    let err = thread
        .update(cx, |thread, cx| {
            thread.read_text_file(path!("/foo").into(), None, None, false, cx)
        })
        .await
        .unwrap_err();

    assert_eq!(err.code, acp::ErrorCode::ResourceNotFound);
}
