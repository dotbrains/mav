use super::*;

#[gpui::test]
async fn test_initialize_from_source_does_not_overwrite_existing_content(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project_a", json!({ "file.txt": "" }))
        .await;
    fs.insert_tree("/project_b", json!({ "file.txt": "" }))
        .await;
    let project_a = Project::test(fs.clone(), [Path::new("/project_a")], cx).await;
    let project_b = Project::test(fs.clone(), [Path::new("/project_b")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

    let workspace_a = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    let workspace_b = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.test_add_workspace(project_b.clone(), window, cx)
        })
        .unwrap();

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    panel_a.update_in(cx, |panel, window, cx| {
        panel.open_external_thread_with_server(
            Rc::new(StubAgentServer::default_response()),
            window,
            cx,
        );
    });
    cx.run_until_parked();

    let thread_view_a = panel_a.read_with(cx, |panel, cx| panel.active_thread_view(cx).unwrap());
    let editor_a = thread_view_a.read_with(cx, |view, _cx| view.message_editor.clone());
    editor_a.update_in(cx, |editor, window, cx| {
        editor.set_text("Draft from workspace A", window, cx);
    });

    let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    panel_b.update_in(cx, |panel, window, cx| {
        panel.open_external_thread_with_server(
            Rc::new(StubAgentServer::default_response()),
            window,
            cx,
        );
    });
    cx.run_until_parked();

    let thread_view_b = panel_b.read_with(cx, |panel, cx| panel.active_thread_view(cx).unwrap());
    let editor_b = thread_view_b.read_with(cx, |view, _cx| view.message_editor.clone());
    editor_b.update_in(cx, |editor, window, cx| {
        editor.set_text("Existing work in workspace B", window, cx);
    });

    let transferred = panel_b.update_in(cx, |panel, window, cx| {
        panel.initialize_from_source_workspace_if_needed(workspace_a.downgrade(), window, cx)
    });
    assert!(
        !transferred,
        "destination panel with existing content should not be overwritten"
    );

    panel_b.read_with(cx, |panel, cx| {
        let thread_view = panel
            .active_thread_view(cx)
            .expect("panel_b should still have its thread view");
        let text = thread_view.read(cx).message_editor.read(cx).text(cx);
        assert_eq!(
            text, "Existing work in workspace B",
            "destination panel's content should be preserved"
        );
    });
}
