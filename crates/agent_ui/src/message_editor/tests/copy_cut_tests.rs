use super::*;

#[gpui::test]
async fn test_paste_mention_link_with_multiple_selections(cx: &mut TestAppContext) {
    init_test(cx);

    let app_state = cx.update(AppState::test);

    cx.update(|cx| {
        editor::init(cx);
        workspace::init(app_state.clone(), cx);
    });

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/project"), json!({"file.txt": "content"}))
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/project").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let mut cx = VisualTestContext::from_window(window.into(), cx);

    let thread_store = cx.new(|cx| ThreadStore::new(cx));

    let (message_editor, editor) = workspace.update_in(&mut cx, |workspace, window, cx| {
        let workspace_handle = cx.weak_entity();
        let message_editor = cx.new(|cx| {
            MessageEditor::new(
                workspace_handle,
                project.downgrade(),
                Some(thread_store),
                Default::default(),
                "Test Agent".into(),
                "Test",
                EditorMode::AutoHeight {
                    max_lines: None,
                    min_lines: 1,
                },
                window,
                cx,
            )
        });
        workspace.active_pane().update(cx, |pane, cx| {
            pane.add_item(
                Box::new(cx.new(|_| MessageEditorItem(message_editor.clone()))),
                true,
                true,
                None,
                window,
                cx,
            );
        });
        message_editor.read(cx).focus_handle(cx).focus(window, cx);
        let editor = message_editor.read(cx).editor().clone();
        (message_editor, editor)
    });

    editor.update_in(&mut cx, |editor, window, cx| {
        editor.set_text(
            "AAAAAAAAAAAAAAAAAAAAAAAAA     AAAAAAAAAAAAAAAAAAAAAAAAA",
            window,
            cx,
        );
    });

    cx.run_until_parked();

    editor.update_in(&mut cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                MultiBufferOffset(0)..MultiBufferOffset(25), // First selection (large)
                MultiBufferOffset(30)..MultiBufferOffset(55), // Second selection (newest)
            ]);
        });
    });

    let mention_link = "[@f](file:///test.txt)";
    cx.write_to_clipboard(ClipboardItem::new_string(mention_link.into()));

    message_editor.update_in(&mut cx, |message_editor, window, cx| {
        message_editor.paste(&Paste, window, cx);
    });

    let text = editor.update(&mut cx, |editor, cx| editor.text(cx));
    assert!(
        text.contains("[@f](file:///test.txt)"),
        "Expected mention link to be pasted, got: {}",
        text
    );
}

#[gpui::test]
async fn test_copy_with_selection_mentions_serializes_links(cx: &mut TestAppContext) {
    init_test(cx);

    let (source_message_editor, _source_editor, mut cx) =
        setup_paste_test_message_editor(json!({"file.rs": "line 1\nline 2\nline 3\nline 4\n"}), cx)
            .await;

    let workspace = source_message_editor.read_with(&cx, |message_editor, _| {
        message_editor.workspace.upgrade().expect("workspace")
    });
    let project = workspace.read_with(&cx, |workspace, _| workspace.project().clone());

    let source_text = "selection needs work\nselection looks fine";
    let first_range = 0..9;
    let second_start = "selection needs work\n".len();
    let second_range = second_start..(second_start + "selection".len());
    let first_uri = MentionUri::Selection {
        abs_path: Some(path!("/project/file.rs").into()),
        line_range: 0..=1,
        column: None,
    };
    let second_uri = MentionUri::Selection {
        abs_path: Some(path!("/project/file.rs").into()),
        line_range: 2..=3,
        column: None,
    };

    source_message_editor.update_in(&mut cx, |message_editor, window, cx| {
        message_editor.set_text(source_text, window, cx);

        let snapshot = message_editor
            .editor
            .read(cx)
            .buffer()
            .read(cx)
            .snapshot(cx);
        for (range, uri, content) in [
            (
                first_range.clone(),
                first_uri.clone(),
                "line 1\nline 2\n".to_string(),
            ),
            (
                second_range.clone(),
                second_uri.clone(),
                "line 3\nline 4\n".to_string(),
            ),
        ] {
            let Some((crease_id, tx, _crease_entity)) = insert_crease_for_mention(
                snapshot
                    .anchor_to_buffer_anchor(snapshot.anchor_before(MultiBufferOffset(range.start)))
                    .expect("selection mention anchor should map to a buffer")
                    .0,
                range.len(),
                uri.name().into(),
                uri.icon_path(cx),
                uri.tooltip_text(),
                Some(uri.clone()),
                Some(message_editor.workspace.clone()),
                None,
                message_editor.editor.clone(),
                window,
                cx,
            ) else {
                panic!("expected mention crease insertion");
            };
            drop(tx);

            message_editor.mention_set.update(cx, |mention_set, cx| {
                mention_set.insert_mention(
                    crease_id,
                    uri,
                    Task::ready(Ok(Mention::Text {
                        content,
                        tracked_buffers: Vec::new(),
                    }))
                    .shared(),
                    None,
                    cx,
                );
            });
        }

        let buffer_len = snapshot.len();
        message_editor.editor.update(cx, |editor, cx| {
            editor.change_selections(Default::default(), window, cx, |selections| {
                selections.select_ranges([MultiBufferOffset(0)..buffer_len]);
            });
        });
    });

    let copied_text = source_message_editor.update(&mut cx, |message_editor, cx| {
        message_editor
            .serialize_selection_with_mentions(false, cx)
            .map(|(text, _)| text)
            .expect("selection mentions should serialize")
    });
    let expected_text = format!(
        "{} needs work\n{} looks fine",
        first_uri.as_link(),
        second_uri.as_link()
    );
    assert_eq!(copied_text, expected_text);

    let target_message_editor = workspace.update_in(&mut cx, |workspace, window, cx| {
        let workspace_handle = cx.weak_entity();
        let thread_store = cx.new(|cx| ThreadStore::new(cx));
        let message_editor = cx.new(|cx| {
            MessageEditor::new(
                workspace_handle,
                project.downgrade(),
                Some(thread_store),
                Default::default(),
                "Test Agent".into(),
                "Test",
                EditorMode::AutoHeight {
                    max_lines: None,
                    min_lines: 1,
                },
                window,
                cx,
            )
        });
        workspace.active_pane().update(cx, |pane, cx| {
            pane.add_item(
                Box::new(cx.new(|_| MessageEditorItem(message_editor.clone()))),
                true,
                true,
                None,
                window,
                cx,
            );
        });
        message_editor.read(cx).focus_handle(cx).focus(window, cx);
        message_editor
    });

    cx.write_to_clipboard(ClipboardItem::new_string(copied_text));
    target_message_editor.update_in(&mut cx, |message_editor, window, cx| {
        message_editor.paste(&Paste, window, cx);
    });
    cx.run_until_parked();

    let target_text = target_message_editor.read_with(&cx, |message_editor, cx| {
        message_editor.editor.read(cx).text(cx)
    });
    assert_eq!(target_text, expected_text);

    let contents = mention_contents(&target_message_editor, &mut cx).await;
    assert_eq!(contents.len(), 2);
    assert!(contents.iter().any(|(uri, _)| uri == &first_uri));
    assert!(contents.iter().any(|(uri, _)| uri == &second_uri));
}

struct SelectionMentionFixture {
    message_editor: Entity<MessageEditor>,
    first_uri: MentionUri,
    first_range: Range<usize>,
    second_uri: MentionUri,
    second_range: Range<usize>,
    buffer_len: MultiBufferOffset,
}

mod selection_copy_cut_tests;
