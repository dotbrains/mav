use super::tests::*;
use super::*;

#[gpui::test]
async fn test_message_editing_insert_selections(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Response".into()),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Original message to edit", window, cx)
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));
    cx.run_until_parked();

    let user_message_editor = conversation_view.read_with(cx, |conversation_view, cx| {
        conversation_view
            .active_thread()
            .map(|active| &active.read(cx).entry_view_state)
            .as_ref()
            .unwrap()
            .read(cx)
            .entry(0)
            .expect("Should have at least one entry")
            .message_editor()
            .expect("Should have message editor")
            .clone()
    });

    cx.focus(&user_message_editor);
    conversation_view.read_with(cx, |view, cx| {
        assert_eq!(
            view.active_thread()
                .and_then(|active| active.read(cx).editing_message),
            Some(0)
        );
    });

    // The edited message must differ from the sent content before focus moves.
    user_message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Original message to edit with ", window, cx)
    });

    let (workspace, project) = conversation_view.read_with(cx, |conversation_view, _cx| {
        (
            conversation_view.workspace.clone(),
            conversation_view.project.clone(),
        )
    });
    let buffer = project.update(cx, |project, cx| {
        project.create_local_buffer("let a = 10 + 10;", None, false, cx)
    });

    workspace
        .update_in(cx, |workspace, window, cx| {
            let editor = cx.new(|cx| {
                let mut editor =
                    Editor::for_buffer(buffer.clone(), Some(project.clone()), window, cx);

                editor.change_selections(Default::default(), window, cx, |selections| {
                    selections.select_ranges([MultiBufferOffset(8)..MultiBufferOffset(15)]);
                });

                editor
            });
            workspace.add_item_to_active_pane(Box::new(editor), None, false, window, cx);
        })
        .unwrap();

    conversation_view.update_in(cx, |view, window, cx| {
        assert_eq!(
            view.active_thread()
                .and_then(|active| active.read(cx).editing_message),
            Some(0)
        );
        let workspace = workspace.upgrade().unwrap();
        let selection = workspace
            .update(cx, |workspace, cx| {
                AgentContextSource::from_active(workspace, cx)?.read_selection(workspace, false, cx)
            })
            .unwrap();
        view.insert_selection(selection, window, cx);
    });

    user_message_editor.read_with(cx, |editor, cx| {
        let text = editor.editor().read(cx).text(cx);
        let expected_text = String::from("Original message to edit with selection ");

        assert_eq!(text, expected_text);
    });
}

#[gpui::test]
async fn test_insert_selections(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Response".into()),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Can you review this snippet ", window, cx)
    });

    let (workspace, project) = conversation_view.read_with(cx, |conversation_view, _cx| {
        (
            conversation_view.workspace.clone(),
            conversation_view.project.clone(),
        )
    });
    let buffer = project.update(cx, |project, cx| {
        project.create_local_buffer("let a = 10 + 10;", None, false, cx)
    });

    workspace
        .update_in(cx, |workspace, window, cx| {
            let editor = cx.new(|cx| {
                let mut editor =
                    Editor::for_buffer(buffer.clone(), Some(project.clone()), window, cx);

                editor.change_selections(Default::default(), window, cx, |selections| {
                    selections.select_ranges([MultiBufferOffset(8)..MultiBufferOffset(15)]);
                });

                editor
            });
            workspace.add_item_to_active_pane(Box::new(editor), None, false, window, cx);
        })
        .unwrap();

    conversation_view.update_in(cx, |view, window, cx| {
        assert_eq!(
            view.active_thread()
                .and_then(|active| active.read(cx).editing_message),
            None
        );
        let workspace = view.workspace.upgrade().unwrap();
        let selection = workspace
            .update(cx, |workspace, cx| {
                AgentContextSource::from_active(workspace, cx)?.read_selection(workspace, false, cx)
            })
            .unwrap();
        view.insert_selection(selection, window, cx);
    });

    message_editor.read_with(cx, |editor, cx| {
        let text = editor.text(cx);
        let expected_txt = String::from("Can you review this snippet selection ");

        assert_eq!(text, expected_txt);
    })
}
