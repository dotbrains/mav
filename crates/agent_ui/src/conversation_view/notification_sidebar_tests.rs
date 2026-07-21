use super::tests::*;
use super::*;

#[gpui::test]
async fn test_notification_when_different_conversation_is_active_in_visible_panel(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let (project, multi_workspace_handle, workspace, cx) =
        setup_multi_workspace_with_agent_globals(cx, true).await;

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| crate::AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        workspace.focus_panel::<crate::AgentPanel>(window, cx);
        panel
    });

    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.open_external_thread_with_server(
            Rc::new(StubAgentServer::default_response()),
            window,
            cx,
        );
    });

    cx.run_until_parked();

    panel.read_with(cx, |panel, cx| {
        assert!(crate::AgentPanel::is_visible(&workspace, cx));
        assert!(panel.active_conversation_view().is_some());
    });

    let conversation_view = create_agent_panel_conversation(&project, &workspace, cx);
    cx.run_until_parked();

    panel.read_with(cx, |panel, _cx| {
        assert_ne!(
            panel
                .active_conversation_view()
                .map(|view| view.entity_id()),
            Some(conversation_view.entity_id()),
            "The visible panel should still be showing a different conversation"
        );
    });

    send_hello(&conversation_view, cx);
    assert_agent_notification_visible(cx);

    drop(multi_workspace_handle);
}

#[gpui::test]
async fn test_no_notification_when_sidebar_open_but_different_thread_focused(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let (project, multi_workspace_handle, workspace, cx) =
        setup_multi_workspace_with_agent_globals(cx, true).await;
    register_test_sidebar(true, cx);
    multi_workspace_handle
        .update(cx, |mw, _window, cx| {
            mw.open_sidebar(cx);
        })
        .unwrap();
    cx.run_until_parked();

    assert!(
        multi_workspace_handle
            .read_with(cx, |mw, _cx| mw.sidebar_open())
            .unwrap(),
        "Sidebar should be open"
    );

    let conversation_view = create_agent_panel_conversation(&project, &workspace, cx);
    cx.run_until_parked();

    send_hello(&conversation_view, cx);
    assert!(
        !cx.windows()
            .iter()
            .any(|window| window.downcast::<AgentNotification>().is_some()),
        "Expected no notification when the sidebar is open, even if focused on another thread"
    );
}

#[gpui::test]
async fn test_notification_when_sidebar_open_but_thread_list_hidden(cx: &mut TestAppContext) {
    init_test(cx);

    let (project, multi_workspace_handle, workspace, cx) =
        setup_multi_workspace_with_agent_globals(cx, true).await;
    register_test_sidebar(false, cx);
    multi_workspace_handle
        .update(cx, |mw, _window, cx| {
            mw.open_sidebar(cx);
        })
        .unwrap();
    cx.run_until_parked();

    let conversation_view = create_agent_panel_conversation(&project, &workspace, cx);
    cx.run_until_parked();

    send_hello(&conversation_view, cx);
    assert_agent_notification_visible(cx);
}

#[gpui::test]
async fn test_notification_dismissed_when_sidebar_opens(cx: &mut TestAppContext) {
    init_test(cx);

    let (project, multi_workspace_handle, workspace, cx) =
        setup_multi_workspace_with_agent_globals(cx, true).await;
    register_test_sidebar(true, cx);

    let conversation_view = create_agent_panel_conversation(&project, &workspace, cx);
    cx.run_until_parked();

    send_hello(&conversation_view, cx);
    assert_eq!(
        agent_notification_count(cx),
        1,
        "Expected a notification while the thread is not visible"
    );

    multi_workspace_handle
        .update(cx, |mw, _window, cx| {
            mw.open_sidebar(cx);
        })
        .unwrap();
    cx.run_until_parked();

    assert_eq!(
        agent_notification_count(cx),
        0,
        "Notification should auto-dismiss when the sidebar opens and makes the thread visible"
    );
}

async fn setup_multi_workspace_with_agent_globals(
    cx: &mut TestAppContext,
    agent_v2: bool,
) -> (
    Entity<Project>,
    WindowHandle<MultiWorkspace>,
    Entity<Workspace>,
    &mut VisualTestContext,
) {
    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| {
        cx.update_flags(agent_v2, vec!["agent-v2".to_string()]);
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        <dyn Fs>::set_global(fs.clone(), cx);
    });

    let project = Project::test(fs, [], cx).await;
    let multi_workspace_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace_handle
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(multi_workspace_handle.into(), cx);

    (project, multi_workspace_handle, workspace, cx)
}

fn create_agent_panel_conversation(
    project: &Entity<Project>,
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Entity<ConversationView> {
    let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
    let connection_store =
        cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

    cx.update(|window, cx| {
        cx.new(|cx| {
            ConversationView::new(
                Rc::new(StubAgentServer::default_response()),
                connection_store,
                Agent::Custom { id: "Test".into() },
                None,
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
    })
}

fn send_hello(conversation_view: &Entity<ConversationView>, cx: &mut VisualTestContext) {
    let message_editor = message_editor(conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Hello", window, cx);
    });

    active_thread(conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));
    cx.run_until_parked();
}

fn assert_agent_notification_visible(cx: &mut VisualTestContext) {
    assert!(
        cx.windows()
            .iter()
            .any(|window| window.downcast::<AgentNotification>().is_some())
    );
}

fn agent_notification_count(cx: &mut VisualTestContext) -> usize {
    cx.windows()
        .iter()
        .filter(|window| window.downcast::<AgentNotification>().is_some())
        .count()
}
