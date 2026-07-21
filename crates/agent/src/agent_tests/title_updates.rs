#[gpui::test]
async fn test_rapid_title_changes_do_not_loop(cx: &mut TestAppContext) {
    // Regression test: rapid title changes must not cause a propagation loop
    // between Thread and AcpThread via handle_thread_title_updated.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/", json!({ "a": {} })).await;
    let project = Project::test(fs.clone(), [], cx).await;
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

    let title_updated_count = Rc::new(std::cell::RefCell::new(0usize));
    cx.update(|cx| {
        let count = title_updated_count.clone();
        cx.subscribe(
            &thread,
            move |_entity: Entity<Thread>, _event: &TitleUpdated, _cx: &mut App| {
                let new_count = {
                    let mut count = count.borrow_mut();
                    *count += 1;
                    *count
                };
                assert!(
                    new_count <= 2,
                    "TitleUpdated fired {new_count} times; \
                     title updates are looping"
                );
            },
        )
        .detach();
    });

    thread.update(cx, |thread, cx| thread.set_title("first".into(), cx));
    thread.update(cx, |thread, cx| thread.set_title("second".into(), cx));

    cx.run_until_parked();

    thread.read_with(cx, |thread, _| {
        assert_eq!(thread.title(), Some("second".into()));
    });
    acp_thread.read_with(cx, |acp_thread, _| {
        assert_eq!(acp_thread.title(), Some("second".into()));
    });

    assert_eq!(*title_updated_count.borrow(), 2);
}

