use super::tests::*;
use super::*;

#[gpui::test]
async fn test_notification_when_workspace_is_background_in_multi_workspace(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    // Enable multi-workspace feature flag and init globals needed by AgentPanel
    let fs = FakeFs::new(cx.executor());

    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        <dyn Fs>::set_global(fs.clone(), cx);
    });

    let project1 = Project::test(fs.clone(), [], cx).await;

    // Create a MultiWorkspace window with one workspace
    let multi_workspace_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project1.clone(), window, cx));

    // Get workspace 1 (the initial workspace)
    let workspace1 = multi_workspace_handle
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    let cx = &mut VisualTestContext::from_window(multi_workspace_handle.into(), cx);

    let panel = workspace1.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| crate::AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);

        // Open the dock and activate the agent panel so it's visible
        workspace.focus_panel::<crate::AgentPanel>(window, cx);
        panel
    });

    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.open_external_thread_with_server(
            Rc::new(StubAgentServer::new(RestoredAvailableCommandsConnection)),
            window,
            cx,
        );
    });

    cx.run_until_parked();

    cx.read(|cx| {
        assert!(
            crate::AgentPanel::is_visible(&workspace1, cx),
            "AgentPanel should be visible in workspace1's dock"
        );
    });

    // Set up thread view in workspace 1
    let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
    let connection_store =
        cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project1.clone(), cx)));

    let conversation_view = cx.update(|window, cx| {
        cx.new(|cx| {
            ConversationView::new(
                Rc::new(StubAgentServer::new(RestoredAvailableCommandsConnection)),
                connection_store,
                Agent::Custom { id: "Test".into() },
                None,
                None,
                None,
                None,
                None,
                workspace1.downgrade(),
                project1.clone(),
                Some(thread_store),
                AgentThreadSource::AgentPanel,
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();

    let root_session_id = conversation_view
        .read_with(cx, |view, cx| {
            view.root_thread_view()
                .map(|thread| thread.read(cx).thread.read(cx).session_id().clone())
        })
        .expect("Conversation view should have a root thread");

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Hello", window, cx);
    });

    // Create a second workspace and switch to it.
    // This makes workspace1 the "background" workspace.
    let project2 = Project::test(fs, [], cx).await;
    multi_workspace_handle
        .update(cx, |mw, window, cx| {
            mw.test_add_workspace(project2, window, cx);
        })
        .unwrap();

    cx.run_until_parked();

    // Verify workspace1 is no longer the active workspace
    multi_workspace_handle
        .read_with(cx, |mw, _cx| {
            assert_ne!(mw.workspace(), &workspace1);
        })
        .unwrap();

    // Window is active, agent panel is visible in workspace1, but workspace1
    // is in the background. The notification should show because the user
    // can't actually see the agent panel.
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    assert!(
        cx.windows()
            .iter()
            .any(|window| window.downcast::<AgentNotification>().is_some()),
        "Expected notification when workspace is in background within MultiWorkspace"
    );

    // Also verify: clicking "View Panel" should switch to workspace1.
    cx.windows()
        .iter()
        .find_map(|window| window.downcast::<AgentNotification>())
        .unwrap()
        .update(cx, |window, _, cx| window.accept(cx))
        .unwrap();

    cx.run_until_parked();

    multi_workspace_handle
        .read_with(cx, |mw, _cx| {
            assert_eq!(
                mw.workspace(),
                &workspace1,
                "Expected workspace1 to become the active workspace after accepting notification"
            );
        })
        .unwrap();

    panel.read_with(cx, |panel, cx| {
        let active_session_id = panel
            .active_agent_thread(cx)
            .map(|thread| thread.read(cx).session_id().clone());
        assert_eq!(
            active_session_id,
            Some(root_session_id),
            "Expected accepting the notification to load the notified thread in AgentPanel"
        );
    });
}

#[gpui::test]
async fn test_notification_closed_when_thread_view_dropped(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;

    let weak_view = conversation_view.downgrade();

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Hello", window, cx);
    });

    cx.deactivate_window();

    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    // Verify notification is shown
    assert!(
        cx.windows()
            .iter()
            .any(|window| window.downcast::<AgentNotification>().is_some()),
        "Expected notification to be shown"
    );

    // Drop the thread view (simulating navigation to a new thread)
    drop(conversation_view);
    drop(message_editor);
    // Trigger an update to flush effects, which will call release_dropped_entities
    cx.update(|_window, _cx| {});
    cx.run_until_parked();

    // Verify the entity was actually released
    assert!(
        !weak_view.is_upgradable(),
        "Thread view entity should be released after dropping"
    );

    // The notification should be automatically closed via on_release
    assert!(
        !cx.windows()
            .iter()
            .any(|window| window.downcast::<AgentNotification>().is_some()),
        "Notification should be closed when thread view is dropped"
    );
}
