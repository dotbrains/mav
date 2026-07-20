use super::*;

#[gpui::test]
async fn test_draft_replaced_when_selected_agent_changes(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        <dyn fs::Fs>::set_global(fs.clone(), cx);
    });

    fs.insert_tree("/project", json!({ "file.txt": "" })).await;
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let workspace = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();

    workspace.update(cx, |workspace, _cx| {
        workspace.set_random_database_id();
    });

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
    });

    let first_draft_id = panel.read_with(cx, |panel, cx| {
        assert!(panel.draft_thread.is_some());
        assert_eq!(panel.selected_agent, Agent::NativeAgent);
        let draft = panel.draft_thread.as_ref().unwrap();
        assert_eq!(*draft.read(cx).agent_key(), Agent::NativeAgent);
        draft.entity_id()
    });

    let custom_agent = Agent::Custom {
        id: "my-custom-agent".into(),
    };
    panel.update_in(cx, |panel, window, cx| {
        panel.selected_agent = custom_agent.clone();
        panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
    });

    panel.read_with(cx, |panel, cx| {
        let draft = panel.draft_thread.as_ref().expect("draft should exist");
        assert_ne!(
            draft.entity_id(),
            first_draft_id,
            "a new draft should have been created"
        );
        assert_eq!(
            *draft.read(cx).agent_key(),
            custom_agent,
            "the new draft should use the custom agent"
        );
    });

    let second_draft_id = panel.read_with(cx, |panel, _cx| {
        panel.draft_thread.as_ref().unwrap().entity_id()
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
    });

    panel.read_with(cx, |panel, _cx| {
        assert_eq!(
            panel.draft_thread.as_ref().unwrap().entity_id(),
            second_draft_id,
            "draft should be reused when the agent has not changed"
        );
    });
}

#[gpui::test]
async fn test_activate_draft_preserves_typed_content(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        <dyn fs::Fs>::set_global(fs.clone(), cx);
    });

    fs.insert_tree("/project", json!({ "file.txt": "" })).await;
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let workspace = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();

    workspace.update(cx, |workspace, _cx| {
        workspace.set_random_database_id();
    });

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.selected_agent = Agent::Stub;
        panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();

    let initial_draft_id = panel.read_with(cx, |panel, _cx| {
        panel.draft_thread.as_ref().unwrap().entity_id()
    });
    let initial_thread_id = panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());

    let thread_view = panel.read_with(cx, |panel, cx| panel.active_thread_view(cx).unwrap());
    let message_editor = thread_view.read_with(cx, |view, _cx| view.message_editor.clone());
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Don't lose me!", window, cx);
    });

    cx.dispatch_action(NewThread);
    cx.run_until_parked();

    panel.read_with(cx, |panel, _cx| {
        assert!(
            panel.retained_threads.contains_key(&initial_thread_id),
            "typed draft should have been parked into retained_threads"
        );
        let active_draft_id = panel.draft_thread.as_ref().unwrap().entity_id();
        assert_ne!(
            active_draft_id, initial_draft_id,
            "cmd-n should produce a fresh ephemeral draft"
        );
    });

    let parked_text = panel.read_with(cx, |panel, cx| panel.editor_text(initial_thread_id, cx));
    assert_eq!(
        parked_text.as_deref(),
        Some("Don't lose me!"),
        "parked draft should retain the typed prompt"
    );

    let active_thread_id = panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());
    let active_text = panel.read_with(cx, |panel, cx| panel.editor_text(active_thread_id, cx));
    assert_eq!(
        active_text, None,
        "fresh ephemeral draft should start empty, not carry the parked draft's prompt"
    );
}
