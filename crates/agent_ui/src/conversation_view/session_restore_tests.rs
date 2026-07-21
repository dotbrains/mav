use super::tests::*;
use super::*;

#[gpui::test]
async fn test_resume_without_history_adds_notice(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
    let connection_store =
        cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

    let conversation_view = cx.update(|window, cx| {
        cx.new(|cx| {
            ConversationView::new(
                Rc::new(StubAgentServer::new(ResumeOnlyAgentConnection)),
                connection_store,
                Agent::Custom { id: "Test".into() },
                Some(acp::SessionId::new("resume-session")),
                None,
                None,
                None,
                None,
                workspace.downgrade(),
                project,
                Some(thread_store),
                AgentThreadSource::AgentPanel,
                window,
                cx,
            )
        })
    });

    cx.run_until_parked();

    conversation_view.read_with(cx, |view, cx| {
        let state = view.active_thread().unwrap();
        assert!(state.read(cx).resumed_without_history);
        assert_eq!(state.read(cx).list_state.item_count(), 0);
    });
}

#[gpui::test]
async fn test_restored_threads_keep_available_commands(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
    let connection_store =
        cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

    let conversation_view = cx.update(|window, cx| {
        cx.new(|cx| {
            ConversationView::new(
                Rc::new(StubAgentServer::new(RestoredAvailableCommandsConnection)),
                connection_store,
                Agent::Custom { id: "Test".into() },
                Some(acp::SessionId::new("restored-session")),
                None,
                None,
                None,
                None,
                workspace.downgrade(),
                project,
                Some(thread_store),
                AgentThreadSource::AgentPanel,
                window,
                cx,
            )
        })
    });

    cx.run_until_parked();

    let message_editor = message_editor(&conversation_view, cx);
    let editor = message_editor.update(cx, |message_editor, _cx| message_editor.editor().clone());
    let placeholder = editor.update(cx, |editor, cx| editor.placeholder_text(cx));

    active_thread(&conversation_view, cx).read_with(cx, |view, _cx| {
        let available_commands = view
            .session_capabilities
            .read()
            .available_commands()
            .to_vec();
        assert_eq!(available_commands.len(), 1);
        assert_eq!(available_commands[0].name.as_str(), "help");
        assert_eq!(available_commands[0].description.as_str(), "Get help");
    });

    assert_eq!(placeholder, Some("Ask anything".to_string()));

    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("/help", window, cx);
    });

    let contents_result = message_editor
        .update(cx, |editor, cx| editor.contents(false, cx))
        .await;

    assert!(contents_result.is_ok());
}

#[gpui::test]
async fn test_resume_thread_uses_session_cwd_when_inside_project(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            "subdir": {
                "file.txt": "hello"
            }
        }),
    )
    .await;
    let project = Project::test(fs, [Path::new("/project")], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let connection = CwdCapturingConnection::new();
    let captured_cwd = connection.captured_work_dirs.clone();

    let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
    let connection_store =
        cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

    let _conversation_view = cx.update(|window, cx| {
        cx.new(|cx| {
            ConversationView::new(
                Rc::new(StubAgentServer::new(connection)),
                connection_store,
                Agent::Custom { id: "Test".into() },
                Some(acp::SessionId::new("session-1")),
                None,
                Some(PathList::new(&[PathBuf::from("/project/subdir")])),
                None,
                None,
                workspace.downgrade(),
                project,
                Some(thread_store),
                AgentThreadSource::AgentPanel,
                window,
                cx,
            )
        })
    });

    cx.run_until_parked();

    assert_eq!(
        captured_cwd.lock().as_ref().unwrap(),
        &PathList::new(&[Path::new("/project/subdir")]),
        "Should use session cwd when it's inside the project"
    );
}
