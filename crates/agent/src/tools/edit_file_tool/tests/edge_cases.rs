use super::*;

#[gpui::test]
async fn test_streaming_overlapping_edits_resolved_sequentially(cx: &mut TestAppContext) {
    // Edit 1's replacement introduces text that contains edit 2's
    // old_text as a substring. Because edits resolve sequentially
    // against the current buffer, edit 2 finds a unique match in
    // the modified buffer and succeeds.
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "aaa\nbbb\nccc\nddd\neee\n"})).await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    // Setup: resolve the buffer
    sender.send_partial(json!({
        "path": "root/file.txt",
    }));
    cx.run_until_parked();

    // Edit 1 replaces "bbb\nccc" with "XXX\nccc\nddd", so the
    // buffer becomes "aaa\nXXX\nccc\nddd\nddd\neee\n".
    // Edit 2's old_text "ccc\nddd" matches the first occurrence
    // in the modified buffer and replaces it with "ZZZ".
    // Edit 3 exists only to mark edit 2 as "complete" during streaming.
    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [
            {"old_text": "bbb\nccc", "new_text": "XXX\nccc\nddd"},
            {"old_text": "ccc\nddd", "new_text": "ZZZ"},
            {"old_text": "eee", "new_text": "DUMMY"}
        ]
    }));
    cx.run_until_parked();

    // Send the final input with all three edits.
    sender.send_full(json!({
        "path": "root/file.txt",
        "edits": [
            {"old_text": "bbb\nccc", "new_text": "XXX\nccc\nddd"},
            {"old_text": "ccc\nddd", "new_text": "ZZZ"},
            {"old_text": "eee", "new_text": "DUMMY"}
        ]
    }));

    let result = task.await;
    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "aaa\nXXX\nZZZ\nddd\nDUMMY\n");
}

#[gpui::test]
async fn test_streaming_edit_json_fixer_escape_corruption(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "hello\nworld\nfoo\n"})).await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    sender.send_partial(json!({
        "path": "root/file.txt",
    }));
    cx.run_until_parked();

    // Simulate JSON fixer producing a literal backslash when the LLM
    // stream cuts in the middle of a \n escape sequence.
    // The old_text "hello\nworld" would be streamed as:
    //   partial 1: old_text = "hello\\" (fixer closes incomplete \n as \\)
    //   partial 2: old_text = "hello\nworld" (fixer corrected the escape)
    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [{"old_text": "hello\\"}]
    }));
    cx.run_until_parked();

    // Now the fixer corrects it to the real newline.
    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [{"old_text": "hello\nworld"}]
    }));
    cx.run_until_parked();

    // Send final.
    sender.send_full(json!({
        "path": "root/file.txt",
        "edits": [{"old_text": "hello\nworld", "new_text": "HELLO\nWORLD"}]
    }));

    let result = task.await;
    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "HELLO\nWORLD\nfoo\n");
}

#[gpui::test]
async fn test_streaming_final_input_stringified_edits_succeeds(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "hello\nworld\n"})).await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    sender.send_partial(json!({
        "path": "root/file.txt",
    }));
    cx.run_until_parked();

    sender.send_full(json!({
        "path": "root/file.txt",
        "edits": "[{\"old_text\": \"hello\\nworld\", \"new_text\": \"HELLO\\nWORLD\"}]"
    }));

    let result = task.await;
    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "HELLO\nWORLD\n");
}

// Verifies that after streaming_edit_file_tool edits a file, the action log
// reports changed buffers so that the Accept All / Reject All review UI appears.
#[gpui::test]
async fn test_streaming_edit_file_tool_registers_changed_buffers(cx: &mut TestAppContext) {
    let (edit_tool, _project, action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "line 1\nline 2\nline 3\n"})).await;
    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    let (event_stream, _rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        edit_tool.clone().run(
            ToolInput::resolved(EditFileToolInput {
                path: "root/file.txt".into(),
                edits: vec![Edit {
                    old_text: "line 2".into(),
                    new_text: "modified line 2".into(),
                }],
            }),
            event_stream,
            cx,
        )
    });

    let result = task.await;
    assert!(result.is_ok(), "edit should succeed: {:?}", result.err());

    cx.run_until_parked();

    let changed = action_log.read_with(cx, |log, cx| log.changed_buffers(cx).collect::<Vec<_>>());
    assert!(
        !changed.is_empty(),
        "action_log.changed_buffers() should be non-empty after streaming edit,
             but no changed buffers were found - Accept All / Reject All will not appear"
    );
}

// Same test but for Write mode (overwrite entire file).

#[gpui::test]
async fn test_streaming_edit_file_tool_fields_out_of_order_in_edit_mode(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "old_content"})).await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    sender.send_partial(json!({
        "edits": [{"old_text": "old_content"}]
    }));
    cx.run_until_parked();

    sender.send_partial(json!({
        "edits": [{"old_text": "old_content", "new_text": "new_content"}]
    }));
    cx.run_until_parked();

    sender.send_partial(json!({
        "edits": [{"old_text": "old_content", "new_text": "new_content"}],
        "path": "root"
    }));
    cx.run_until_parked();

    // Send final.
    sender.send_full(json!({
        "edits": [{"old_text": "old_content", "new_text": "new_content"}],
        "path": "root/file.txt"
    }));
    cx.run_until_parked();

    let result = task.await;
    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "new_content");
}

#[gpui::test]
async fn test_streaming_edit_file_tool_new_and_old_text_appear_together(cx: &mut TestAppContext) {
    let (tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "old_content"})).await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| tool.clone().run(input, event_stream, cx));

    sender.send_partial(json!({
        "mode": "edit",
        "path": "root/file.txt"
    }));
    cx.run_until_parked();

    sender.send_partial(json!({
        "mode": "edit",
        "path": "root/file.txt",
        "edits": [{"new_text": "new_content", "old_text": "old"}]
    }));
    cx.run_until_parked();

    sender.send_partial(json!({
        "mode": "edit",
        "path": "root/file.txt",
        "edits": [{"new_text": "new_content", "old_text": "old_content"}]
    }));
    cx.run_until_parked();

    sender.send_full(json!({
        "mode": "edit",
        "path": "root/file.txt",
        "edits": [{"new_text": "new_content", "old_text": "old_content"}]
    }));
    cx.run_until_parked();

    let result = task.await;
    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "new_content");
}

#[gpui::test]
async fn test_streaming_edit_file_tool_new_text_before_old_text(cx: &mut TestAppContext) {
    let (tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "old_content"})).await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| tool.clone().run(input, event_stream, cx));

    sender.send_partial(json!({
        "mode": "edit",
        "path": "root/file.txt"
    }));
    cx.run_until_parked();

    sender.send_partial(json!({
        "mode": "edit",
        "path": "root/file.txt",
        "edits": [{"new_text": "new_content"}]
    }));
    cx.run_until_parked();

    sender.send_partial(json!({
        "mode": "edit",
        "path": "root/file.txt",
        "edits": [{"new_text": "new_content", "old_text": ""}]
    }));
    cx.run_until_parked();

    sender.send_partial(json!({
        "mode": "edit",
        "path": "root/file.txt",
        "edits": [{"new_text": "new_content", "old_text": "old"}]
    }));
    cx.run_until_parked();

    sender.send_full(json!({
        "mode": "edit",
        "path": "root/file.txt",
        "edits": [{"new_text": "new_content", "old_text": "old_content"}]
    }));
    cx.run_until_parked();

    let result = task.await;
    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "new_content");
}

#[gpui::test]
async fn test_streaming_edit_partial_last_line(cx: &mut TestAppContext) {
    let file_content = indoc::indoc! {r#"
            fn on_query_change(&mut self, cx: &mut Context<Self>) {
                self.filter(cx);
            }



            fn render_search(&self, cx: &mut Context<Self>) -> Div {
                div()
            }
        "#}
    .to_string();

    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.rs": file_content})).await;

    // The model sends old_text with a PARTIAL last line.
    let old_text = "}\n\n\n\nfn render_search";
    let new_text = "}\n\nfn render_search";

    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    sender.send_full(json!({
        "path": "root/file.rs",
        "edits": [{"old_text": old_text, "new_text": new_text}]
    }));

    let result = task.await;
    let EditFileToolOutput::Success {
        new_text: final_text,
        ..
    } = result.unwrap()
    else {
        panic!("expected success");
    };

    // The edit should reduce 3 blank lines to 1 blank line before
    // fn render_search, without duplicating the function signature.
    let expected = file_content.replace("}\n\n\n\nfn render_search", "}\n\nfn render_search");
    pretty_assertions::assert_eq!(
        final_text,
        expected,
        "Edit should only remove blank lines before render_search"
    );
}

#[gpui::test]
async fn test_streaming_edit_preserves_blank_line_after_trailing_newline_replacement(
    cx: &mut TestAppContext,
) {
    let file_content = "before\ntarget\n\nafter\n";
    let old_text = "target\n";
    let new_text = "one\ntwo\ntarget\n";
    let expected = "before\none\ntwo\ntarget\n\nafter\n";

    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.rs": file_content})).await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    sender.send_full(json!({
        "path": "root/file.rs",
        "edits": [{"old_text": old_text, "new_text": new_text}]
    }));

    let result = task.await;

    let EditFileToolOutput::Success {
        new_text: final_text,
        ..
    } = result.unwrap()
    else {
        panic!("expected success");
    };

    pretty_assertions::assert_eq!(
        final_text,
        expected,
        "Edit should preserve a single blank line before test_after"
    );
}

#[test]
fn test_input_deserializes_double_encoded_fields() {
    let input = serde_json::from_value::<EditFileToolInput>(json!({
        "path": "root/file.txt",
        "edits": "[{\"old_text\": \"hello\\nworld\", \"new_text\": \"HELLO\\nWORLD\"}]"
    }))
    .expect("input should deserialize");

    assert_eq!(input.edits.len(), 1);
    assert_eq!(input.edits[0].old_text, "hello\nworld");
    assert_eq!(input.edits[0].new_text, "HELLO\nWORLD");

    let input = serde_json::from_value::<EditFileToolPartialInput>(json!({
        "path": "root/file.txt",
        "edits": "[{\"old_text\": \"hello\\nworld\", \"new_text\": \"HELLO\\nWORLD\"}]"
    }))
    .expect("input should deserialize");

    let edits = input.edits.expect("edits should deserialize");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].old_text.as_deref(), Some("hello\nworld"));
    assert_eq!(edits[0].new_text.as_deref(), Some("HELLO\nWORLD"));

    let input = serde_json::from_value::<EditFileToolPartialInput>(json!({
        "path": "root/file.txt"
    }))
    .expect("input should deserialize");
    assert!(input.edits.is_none());

    let input = serde_json::from_value::<EditFileToolPartialInput>(json!({
        "path": "root/file.txt",
        "edits": null
    }))
    .expect("input should deserialize");
    assert!(input.edits.is_none());
}
