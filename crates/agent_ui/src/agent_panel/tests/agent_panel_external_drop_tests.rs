use super::*;

fn expected_terminal_drop_text(paths: &[PathBuf]) -> String {
    let mut text = String::new();
    for path in paths {
        text.push(' ');
        text.push_str(&format!("{path:?}"));
    }
    text.push(' ');
    text
}

#[gpui::test]
async fn test_terminal_external_image_drop_writes_path(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    cx.update(|_, cx| {
        cx.update_flags(true, vec!["agent-panel-terminal".to_string()]);
    });

    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Image Upload", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    let terminal = panel.read_with(&cx, |panel, cx| {
        panel
            .terminals
            .get(&terminal_id)
            .unwrap()
            .view
            .read(cx)
            .terminal()
            .clone()
    });
    terminal.update(&mut cx, |terminal, _cx| terminal.take_input_log());

    let image_path = PathBuf::from("/tmp/dropped-image.png");
    panel.update_in(&mut cx, |panel, window, cx| {
        let external_paths = ExternalPaths(vec![image_path.clone()].into());
        panel.paste_external_paths_into_active_terminal(&external_paths, window, cx);
    });

    let mut input_log = terminal.update(&mut cx, |terminal, _cx| terminal.take_input_log());
    assert_eq!(input_log.len(), 1, "expected one write to the terminal");
    let written =
        String::from_utf8(input_log.remove(0)).expect("terminal write should be valid UTF-8");
    assert_eq!(
        written,
        expected_terminal_drop_text(std::slice::from_ref(&image_path))
    );
}

#[gpui::test]
async fn test_terminal_external_paths_drop_handler_writes_image_path(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    cx.update(|_, cx| {
        cx.update_flags(true, vec!["agent-panel-terminal".to_string()]);
    });

    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Image Upload", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    let terminal = panel.read_with(&cx, |panel, cx| {
        panel
            .terminals
            .get(&terminal_id)
            .unwrap()
            .view
            .read(cx)
            .terminal()
            .clone()
    });
    terminal.update(&mut cx, |terminal, _cx| terminal.take_input_log());

    let image_path = PathBuf::from("/tmp/dropped-image.png");
    panel.update_in(&mut cx, |panel, window, cx| {
        let external_paths = ExternalPaths(vec![image_path.clone()].into());
        panel.handle_external_paths_drop(&external_paths, window, cx);
    });

    let mut input_log = terminal.update(&mut cx, |terminal, _cx| terminal.take_input_log());
    assert_eq!(input_log.len(), 1, "expected one write to the terminal");
    let written =
        String::from_utf8(input_log.remove(0)).expect("terminal write should be valid UTF-8");
    assert_eq!(
        written,
        expected_terminal_drop_text(std::slice::from_ref(&image_path))
    );
}

#[gpui::test]
async fn test_external_file_drop_on_thread_does_not_paste_into_later_terminal(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        cx.update_flags(true, vec!["agent-panel-terminal".to_string()]);
    });

    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    fs.insert_tree("/project", json!({ "file.txt": "content" }))
        .await;
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();
    let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    open_thread_with_connection(&panel, StubAgentConnection::new(), &mut cx);
    let thread_id = active_thread_id(&panel, &cx);

    let file_path = PathBuf::from("/project/file.txt");
    panel.update_in(&mut cx, |panel, window, cx| {
        let external_paths = ExternalPaths(vec![file_path.clone()].into());
        panel.handle_external_paths_drop(&external_paths, window, cx);
    });

    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Drop Target", true, window, cx)
        })
        .expect("test terminal should be inserted");
    let terminal = panel.read_with(&cx, |panel, cx| {
        panel
            .terminals
            .get(&terminal_id)
            .unwrap()
            .view
            .read(cx)
            .terminal()
            .clone()
    });
    terminal.update(&mut cx, |terminal, _cx| terminal.take_input_log());

    cx.run_until_parked();

    let input_log = terminal.update(&mut cx, |terminal, _cx| terminal.take_input_log());
    assert!(
        input_log.is_empty(),
        "thread drop completion should not write to the active terminal"
    );

    let expected_uri = MentionUri::File {
        abs_path: file_path,
    }
    .to_uri()
    .to_string();
    let expected_text = format!("[@file.txt]({expected_uri}) ");
    let actual_text = panel.read_with(&cx, |panel, cx| panel.editor_text(thread_id, cx));
    assert_eq!(actual_text.as_deref(), Some(expected_text.as_str()));
}
