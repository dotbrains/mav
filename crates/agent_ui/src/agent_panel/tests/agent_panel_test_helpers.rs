use super::*;

/// Extracts the text from a Text content block, panicking if it's not Text.
pub(super) fn expect_text_block(block: &acp::ContentBlock) -> &str {
    match block {
        acp::ContentBlock::Text(t) => t.text.as_str(),
        other => panic!("expected Text block, got {:?}", other),
    }
}

/// Extracts the (text_content, uri) from a Resource content block, panicking
/// if it's not a TextResourceContents resource.
pub(super) fn expect_resource_block(block: &acp::ContentBlock) -> (&str, &str) {
    match block {
        acp::ContentBlock::Resource(r) => match &r.resource {
            acp::EmbeddedResourceResource::TextResourceContents(t) => {
                (t.text.as_str(), t.uri.as_str())
            }
            other => panic!("expected TextResourceContents, got {:?}", other),
        },
        other => panic!("expected Resource block, got {:?}", other),
    }
}

pub(super) fn open_generating_thread_with_loadable_connection(
    panel: &Entity<AgentPanel>,
    connection: &StubAgentConnection,
    cx: &mut VisualTestContext,
) -> (acp::SessionId, ThreadId) {
    open_thread_with_custom_connection(panel, connection.clone(), cx);
    let session_id = active_session_id(panel, cx);
    let thread_id = active_thread_id(panel, cx);
    send_message(panel, cx);
    cx.update(|_, cx| {
        connection.send_update(
            session_id.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("done".into())),
            cx,
        );
    });
    cx.run_until_parked();
    (session_id, thread_id)
}

pub(super) fn open_idle_thread_with_non_loadable_connection(
    panel: &Entity<AgentPanel>,
    connection: &StubAgentConnection,
    cx: &mut VisualTestContext,
) -> (acp::SessionId, ThreadId) {
    open_thread_with_custom_connection(panel, connection.clone(), cx);
    let session_id = active_session_id(panel, cx);
    let thread_id = active_thread_id(panel, cx);

    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("done".into()),
    )]);
    send_message(panel, cx);

    (session_id, thread_id)
}

pub(super) async fn setup_panel(
    cx: &mut TestAppContext,
) -> (Entity<AgentPanel>, VisualTestContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    fs.insert_tree("/project", json!({ "file.txt": "" })).await;
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let workspace = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
        cx.new(|cx| AgentPanel::new(workspace, window, cx))
    });

    (panel, cx)
}

pub(super) async fn setup_visible_panel(
    cx: &mut TestAppContext,
) -> (Entity<AgentPanel>, VisualTestContext) {
    setup_visible_panel_with_sidebar(cx, true).await
}

pub(super) async fn setup_visible_panel_with_sidebar(
    cx: &mut TestAppContext,
    threads_list_active: bool,
) -> (Entity<AgentPanel>, VisualTestContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        AgentSettings::override_global(
            AgentSettings {
                notify_when_agent_waiting: NotifyWhenAgentWaiting::PrimaryScreen,
                ..AgentSettings::get_global(cx).clone()
            },
            cx,
        );
    });

    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    fs.insert_tree("/project", json!({ "file.txt": "" })).await;
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let workspace = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();

    let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);
    register_test_sidebar(threads_list_active, &mut cx);

    let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        workspace.focus_panel::<AgentPanel>(window, cx);
        panel
    });

    (panel, cx)
}

pub(super) async fn setup_workspace_panel(
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, Entity<AgentPanel>, VisualTestContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({ "file.txt": "" })).await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let workspace = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    (workspace, panel, cx)
}
