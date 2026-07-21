use super::tests::*;
use super::*;

#[gpui::test]
async fn test_acp_server_exit_transitions_conversation_to_load_error_without_panic(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let server = FakeAcpAgentServer::new();
    let close_session_count = server.close_session_count();
    let (conversation_view, cx) = setup_conversation_view(server.clone(), cx).await;

    cx.run_until_parked();

    server.simulate_server_exit();
    cx.run_until_parked();

    conversation_view.read_with(cx, |view, _cx| {
        assert!(
            matches!(view.server_state, ServerState::LoadError { .. }),
            "Conversation should transition to LoadError when an ACP thread exits"
        );
    });
    assert_eq!(
        close_session_count.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "ConversationView should close the ACP session after a thread exit"
    );
}

#[gpui::test]
async fn test_refusal_handling(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(RefusalAgentConnection), cx).await;

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Do something harmful", window, cx);
    });

    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    conversation_view.read_with(cx, |thread_view, cx| {
        let state = thread_view.active_thread().unwrap();
        assert!(
            matches!(state.read(cx).thread_error, Some(ThreadError::Refusal)),
            "Expected refusal error to be set"
        );
    });
}

#[gpui::test]
async fn test_connect_failure_transitions_to_load_error(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) = setup_conversation_view(FailingAgentServer, cx).await;

    conversation_view.read_with(cx, |view, cx| {
        let title = view.title(cx);
        assert_eq!(
            title.as_ref(),
            "Error Loading Codex CLI",
            "Tab title should show the agent name with an error prefix"
        );
        match &view.server_state {
            ServerState::LoadError {
                error: LoadError::Other(msg),
                ..
            } => {
                assert!(
                    msg.contains("Invalid gzip header"),
                    "Error callout should contain the underlying extraction error, got: {msg}"
                );
            }
            other => panic!(
                "Expected LoadError::Other, got: {}",
                match other {
                    ServerState::Loading { .. } => "Loading (stuck!)",
                    ServerState::LoadError { .. } => "LoadError (wrong variant)",
                    ServerState::Connected(_) => "Connected",
                }
            ),
        }
    });
}

#[gpui::test]
async fn test_reset_preserves_session_id_after_load_error(cx: &mut TestAppContext) {
    use crate::thread_metadata_store::{ThreadId, ThreadMetadata};
    use chrono::Utc;
    use project::{AgentId as ProjectAgentId, WorktreePaths};
    use std::sync::atomic::Ordering;

    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
    let connection_store =
        cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

    let resume_session_id = acp::SessionId::new("persistent-session");
    let stored_title: SharedString = "Persistent chat".into();
    cx.update(|_window, cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(
                ThreadMetadata {
                    thread_id: ThreadId::new(),
                    session_id: Some(resume_session_id.clone()),
                    agent_id: ProjectAgentId::new("Flaky"),
                    title: Some(stored_title.clone()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: Some(Utc::now()),
                    interacted_at: None,
                    worktree_paths: WorktreePaths::from_folder_paths(&PathList::default()),
                    remote_connection: None,
                    archived: false,
                },
                cx,
            );
        });
    });

    let connection = StubAgentConnection::new().with_supports_load_session(true);
    let (server, fail) = FlakyAgentServer::new(connection);

    let conversation_view = cx.update(|window, cx| {
        cx.new(|cx| {
            ConversationView::new(
                Rc::new(server),
                connection_store,
                Agent::Custom { id: "Flaky".into() },
                Some(resume_session_id.clone()),
                None,
                None,
                None,
                None,
                workspace.downgrade(),
                project.clone(),
                Some(thread_store),
                AgentThreadSource::AgentPanel,
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();

    conversation_view.read_with(cx, |view, _cx| {
        assert!(
            matches!(view.server_state, ServerState::LoadError { .. }),
            "expected LoadError after failed initial connect"
        );
        assert_eq!(
            view.root_session_id.as_ref(),
            Some(&resume_session_id),
            "root_session_id should still hold the original id while in LoadError"
        );
    });

    fail.store(false, Ordering::SeqCst);
    project.update(cx, |project, cx| {
        project
            .agent_server_store()
            .update(cx, |_store, cx| cx.emit(project::AgentServersUpdated));
    });
    cx.run_until_parked();

    conversation_view.read_with(cx, |view, cx| {
        let connected = view
            .as_connected()
            .expect("should be Connected after flaky server comes online");
        let active_id = connected
            .active_id
            .as_ref()
            .expect("Connected state should have an active_id");
        assert_eq!(
            active_id, &resume_session_id,
            "reset() must resume the original session id, not call new_session()"
        );
        let active_thread = view
            .active_thread()
            .expect("should have an active thread view");
        let thread_session = active_thread.read(cx).thread.read(cx).session_id().clone();
        assert_eq!(
            thread_session, resume_session_id,
            "the live AcpThread should hold the resumed session id"
        );
    });
}
