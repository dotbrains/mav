use super::tests::*;
use super::*;

#[gpui::test]
async fn test_move_queued_message_to_empty_main_editor(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;

    active_thread(&conversation_view, cx).update_in(cx, |thread, window, cx| {
        thread.add_to_queue(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "queued message".to_string(),
            ))],
            vec![],
            window,
            cx,
        );
        assert!(thread.message_editor.read(cx).is_empty(cx));
        let id = thread.message_queue.first_id().unwrap();
        thread.move_queued_message_to_main_editor(id, None, None, window, cx);
    });

    cx.run_until_parked();

    let queue_len = active_thread(&conversation_view, cx)
        .read_with(cx, |thread, _cx| thread.message_queue.len());
    assert_eq!(queue_len, 0);

    let text = message_editor(&conversation_view, cx).update(cx, |editor, cx| editor.text(cx));
    assert_eq!(text, "queued message");
}

#[gpui::test]
async fn test_move_queued_message_to_non_empty_main_editor(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;

    message_editor(&conversation_view, cx).update_in(cx, |editor, window, cx| {
        editor.set_message(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "existing content".to_string(),
            ))],
            window,
            cx,
        );
    });

    active_thread(&conversation_view, cx).update_in(cx, |thread, window, cx| {
        thread.add_to_queue(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "queued message".to_string(),
            ))],
            vec![],
            window,
            cx,
        );
        let id = thread.message_queue.first_id().unwrap();
        thread.move_queued_message_to_main_editor(id, None, None, window, cx);
    });

    cx.run_until_parked();

    let queue_len = active_thread(&conversation_view, cx)
        .read_with(cx, |thread, _cx| thread.message_queue.len());
    assert_eq!(queue_len, 0);

    let text = message_editor(&conversation_view, cx).update(cx, |editor, cx| editor.text(cx));
    assert_eq!(text, "existing content\n\nqueued message");
}

#[gpui::test]
async fn test_move_up_in_empty_editor_restores_last_queued_message(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    active_thread(&conversation_view, cx).update_in(cx, |thread, window, cx| {
        thread.add_to_queue(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "first queued".to_string(),
            ))],
            vec![],
            window,
            cx,
        );
        thread.add_to_queue(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "second queued".to_string(),
            ))],
            vec![],
            window,
            cx,
        );
    });
    cx.run_until_parked();

    let editor = message_editor(&conversation_view, cx);
    cx.focus(&editor);

    editor.update_in(cx, |_editor, window, cx| {
        window.dispatch_action(Box::new(mav_actions::editor::MoveUp), cx);
    });
    cx.run_until_parked();

    let queue_len = active_thread(&conversation_view, cx)
        .read_with(cx, |thread, _cx| thread.message_queue.len());
    assert_eq!(queue_len, 1);
    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        "second queued"
    );

    editor.update_in(cx, |_editor, window, cx| {
        window.dispatch_action(Box::new(mav_actions::editor::MoveUp), cx);
    });
    cx.run_until_parked();

    let queue_len = active_thread(&conversation_view, cx)
        .read_with(cx, |thread, _cx| thread.message_queue.len());
    assert_eq!(queue_len, 1);
    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        "second queued"
    );
}

#[gpui::test]
async fn test_paste_text_into_queued_message_promotes_to_main_editor(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        paste_into_queued_message(cx, ClipboardItem::new_string("PASTED".to_string())).await;

    let queue_len = active_thread(&conversation_view, cx)
        .read_with(cx, |thread, _cx| thread.message_queue.len());
    assert_eq!(queue_len, 0);

    let text = message_editor(&conversation_view, cx).update(cx, |editor, cx| editor.text(cx));
    assert_eq!(text, "queued PASTEDmessage");
}

#[gpui::test]
async fn test_paste_image_into_queued_message_promotes_to_main_editor(cx: &mut TestAppContext) {
    init_test(cx);

    use base64::Engine as _;
    use std::io::Write as _;
    let png_bytes = base64::prelude::BASE64_STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==")
        .unwrap();
    let mut image_file = tempfile::Builder::new().suffix(".png").tempfile().unwrap();
    image_file.write_all(&png_bytes).unwrap();

    let (conversation_view, cx) = paste_into_queued_message(
        cx,
        ClipboardItem {
            entries: vec![gpui::ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
                vec![image_file.path().to_path_buf()].into(),
            ))],
        },
    )
    .await;

    let queue_len = active_thread(&conversation_view, cx)
        .read_with(cx, |thread, _cx| thread.message_queue.len());
    assert_eq!(queue_len, 0);

    let text = message_editor(&conversation_view, cx).update(cx, |editor, cx| editor.text(cx));
    let image_name = image_file.path().file_name().unwrap().to_string_lossy();
    let expected_uri = acp_thread::MentionUri::PastedImage {
        name: image_name.to_string(),
    }
    .to_uri()
    .to_string();
    assert_eq!(
        text,
        format!("queued [@{image_name}]({expected_uri}) message"),
    );
}

#[gpui::test]
async fn test_queued_message_steer_defaults_off_and_toggles(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let id = active_thread(&conversation_view, cx).update_in(cx, |thread, window, cx| {
        thread.add_to_queue(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "queued".to_string(),
            ))],
            vec![],
            window,
            cx,
        );
        thread.message_queue.first_id().unwrap()
    });
    cx.run_until_parked();

    active_thread(&conversation_view, cx).read_with(cx, |thread, _cx| {
        assert!(
            !thread.message_queue.front_wants_steer(),
            "steering should default off"
        );
    });

    active_thread(&conversation_view, cx).update(cx, |thread, _cx| {
        thread.message_queue.toggle_steer(id);
    });
    active_thread(&conversation_view, cx).read_with(cx, |thread, _cx| {
        assert!(
            thread.message_queue.front_wants_steer(),
            "steering should be on after toggling"
        );
    });
}

#[gpui::test]
async fn test_queue_resumes_after_stop_and_new_message(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("first", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));
    cx.run_until_parked();

    active_thread(&conversation_view, cx).update_in(cx, |thread, window, cx| {
        thread.add_to_queue(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "queued".to_string(),
            ))],
            vec![],
            window,
            cx,
        );
    });

    active_thread(&conversation_view, cx)
        .update_in(cx, |thread, _window, cx| thread.cancel_generation(cx));
    cx.run_until_parked();

    let queue_len = active_thread(&conversation_view, cx)
        .read_with(cx, |thread, _cx| thread.message_queue.len());
    assert_eq!(queue_len, 1, "stopping must not send the queued message");

    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("second", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));
    cx.run_until_parked();

    let session_id = conversation_view.read_with(cx, |view, cx| {
        view.active_thread()
            .unwrap()
            .read(cx)
            .thread
            .read(cx)
            .session_id()
            .clone()
    });

    connection.end_turn(session_id, acp::StopReason::EndTurn);
    cx.run_until_parked();

    let queue_len = active_thread(&conversation_view, cx)
        .read_with(cx, |thread, _cx| thread.message_queue.len());
    assert_eq!(
        queue_len, 0,
        "queued message should be auto-sent after the user re-engages"
    );
}

async fn paste_into_queued_message(
    cx: &mut TestAppContext,
    clipboard: ClipboardItem,
) -> (Entity<ConversationView>, &mut VisualTestContext) {
    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    active_thread(&conversation_view, cx).update_in(cx, |thread, window, cx| {
        thread
            .session_capabilities
            .write()
            .set_prompt_capabilities(acp::PromptCapabilities::new().image(true));
        thread.add_to_queue(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "queued message".to_string(),
            ))],
            vec![],
            window,
            cx,
        );
    });
    conversation_view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();

    let queued_editor = active_thread(&conversation_view, cx).read_with(cx, |thread, _cx| {
        thread
            .message_queue
            .first()
            .map(|entry| entry.editor.clone())
            .expect("queued message editor not created")
    });

    cx.write_to_clipboard(clipboard);

    queued_editor.update_in(cx, |message_editor, window, cx| {
        message_editor.editor().update(cx, |editor, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                selections.select_ranges([MultiBufferOffset(7)..MultiBufferOffset(7)]);
            });
        });
        message_editor.paste(&Paste, window, cx);
    });
    cx.run_until_parked();

    (conversation_view, cx)
}
