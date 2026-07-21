use super::*;

async fn setup_selection_mention_fixture(
    cx: &mut TestAppContext,
) -> (SelectionMentionFixture, VisualTestContext) {
    let (message_editor, _source_editor, mut cx) =
        setup_paste_test_message_editor(json!({"file.rs": "line 1\nline 2\nline 3\nline 4\n"}), cx)
            .await;

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

    let buffer_len = message_editor.update_in(&mut cx, |message_editor, window, cx| {
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

        snapshot.len()
    });

    (
        SelectionMentionFixture {
            message_editor,
            first_uri,
            first_range,
            second_uri,
            second_range,
            buffer_len,
        },
        cx,
    )
}

#[gpui::test]
async fn test_serialized_copy_text_selection_covers_only_mention(cx: &mut TestAppContext) {
    init_test(cx);

    let (fixture, mut cx) = setup_selection_mention_fixture(cx).await;

    fixture
        .message_editor
        .update_in(&mut cx, |message_editor, window, cx| {
            let range = fixture.first_range.clone();
            message_editor.editor.update(cx, |editor, cx| {
                editor.change_selections(Default::default(), window, cx, |selections| {
                    selections.select_ranges([
                        MultiBufferOffset(range.start)..MultiBufferOffset(range.end)
                    ]);
                });
            });
        });

    let copied = fixture
        .message_editor
        .update(&mut cx, |message_editor, cx| {
            message_editor
                .serialize_selection_with_mentions(false, cx)
                .map(|(text, _)| text)
        });

    assert_eq!(copied, Some(fixture.first_uri.as_link().to_string()));
}

#[gpui::test]
async fn test_serialized_copy_text_returns_none_when_mentions_outside_selection(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let (fixture, mut cx) = setup_selection_mention_fixture(cx).await;

    let between_start = fixture.first_range.end;
    let between_end = fixture.second_range.start - 1;

    fixture
        .message_editor
        .update_in(&mut cx, |message_editor, window, cx| {
            message_editor.editor.update(cx, |editor, cx| {
                editor.change_selections(Default::default(), window, cx, |selections| {
                    selections.select_ranges([
                        MultiBufferOffset(between_start)..MultiBufferOffset(between_end)
                    ]);
                });
            });
        });

    let copied = fixture
        .message_editor
        .update(&mut cx, |message_editor, cx| {
            message_editor
                .serialize_selection_with_mentions(false, cx)
                .map(|(text, _)| text)
        });

    assert_eq!(copied, None);
}

#[gpui::test]
async fn test_draft_content_blocks_snapshot_preserves_selection_mentions(cx: &mut TestAppContext) {
    init_test(cx);

    let (fixture, mut cx) = setup_selection_mention_fixture(cx).await;

    let blocks = fixture.message_editor.update(&mut cx, |editor, cx| {
        editor
            .session_capabilities
            .write()
            .set_prompt_capabilities(acp::PromptCapabilities::new().embedded_context(true));
        editor.draft_content_blocks_snapshot(cx)
    });

    // Each selection mention must round-trip as a `Resource` block carrying
    // its URI and content, not as a `Text` block containing the fold
    // placeholder string.
    let resource_uris: Vec<&str> = blocks
        .iter()
        .filter_map(|block| match block {
            acp::ContentBlock::Resource(acp::EmbeddedResource {
                resource:
                    acp::EmbeddedResourceResource::TextResourceContents(acp::TextResourceContents {
                        uri,
                        ..
                    }),
                ..
            }) => Some(uri.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        resource_uris.len(),
        2,
        "snapshot should emit one Resource block per selection mention; got {blocks:#?}"
    );
    assert!(resource_uris.contains(&fixture.first_uri.to_uri().to_string().as_str()));
    for block in &blocks {
        if let acp::ContentBlock::Text(text) = block {
            assert!(
                !text.text.split_whitespace().any(|word| word == "selection"),
                "text block must not contain bare fold placeholder: {:?}",
                text.text
            );
        }
    }
}

#[gpui::test]
async fn test_cut_with_selection_mentions_serializes_and_removes(cx: &mut TestAppContext) {
    init_test(cx);

    let (fixture, mut cx) = setup_selection_mention_fixture(cx).await;

    let buffer_len = fixture.buffer_len;
    fixture
        .message_editor
        .update_in(&mut cx, |message_editor, window, cx| {
            message_editor.editor.update(cx, |editor, cx| {
                editor.change_selections(Default::default(), window, cx, |selections| {
                    selections.select_ranges([MultiBufferOffset(0)..buffer_len]);
                });
            });
            message_editor.cut(&Cut, window, cx);
        });

    let expected_text = format!(
        "{} needs work\n{} looks fine",
        fixture.first_uri.as_link(),
        fixture.second_uri.as_link()
    );

    let clipboard_text = cx
        .read_from_clipboard()
        .and_then(|item| match item.entries().first().cloned() {
            Some(ClipboardEntry::String(entry)) => Some(entry.text().to_string()),
            _ => None,
        })
        .expect("cut should write serialized text to clipboard");
    assert_eq!(clipboard_text, expected_text);

    let remaining_text = fixture.message_editor.read_with(&cx, |message_editor, cx| {
        message_editor.editor.read(cx).text(cx)
    });
    assert_eq!(remaining_text, "");
}

#[gpui::test]
async fn test_cut_with_empty_cursor_on_mention_line_removes_whole_line(cx: &mut TestAppContext) {
    init_test(cx);

    let (fixture, mut cx) = setup_selection_mention_fixture(cx).await;

    let cursor_offset = MultiBufferOffset(fixture.first_range.end + 4);
    fixture
        .message_editor
        .update_in(&mut cx, |message_editor, window, cx| {
            message_editor.editor.update(cx, |editor, cx| {
                editor.change_selections(Default::default(), window, cx, |selections| {
                    selections.select_ranges([cursor_offset..cursor_offset]);
                });
            });
            message_editor.cut(&Cut, window, cx);
        });

    let clipboard_text = cx
        .read_from_clipboard()
        .and_then(|item| match item.entries().first().cloned() {
            Some(ClipboardEntry::String(entry)) => Some(entry.text().to_string()),
            _ => None,
        })
        .expect("cut should write serialized text to clipboard");
    assert_eq!(
        clipboard_text,
        format!("{} needs work\n", fixture.first_uri.as_link())
    );

    let remaining_text = fixture.message_editor.read_with(&cx, |message_editor, cx| {
        message_editor.editor.read(cx).text(cx)
    });
    assert_eq!(remaining_text, "selection looks fine");
}

#[gpui::test]
async fn test_serialized_cut_text_returns_none_when_mentions_outside_selection(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let (fixture, mut cx) = setup_selection_mention_fixture(cx).await;

    let between_start = fixture.first_range.end;
    let between_end = fixture.second_range.start - 1;
    fixture
        .message_editor
        .update_in(&mut cx, |message_editor, window, cx| {
            message_editor.editor.update(cx, |editor, cx| {
                editor.change_selections(Default::default(), window, cx, |selections| {
                    selections.select_ranges([
                        MultiBufferOffset(between_start)..MultiBufferOffset(between_end)
                    ]);
                });
            });
        });

    let result = fixture
        .message_editor
        .update(&mut cx, |message_editor, cx| {
            message_editor.serialize_selection_with_mentions(true, cx)
        });

    assert!(
        result.is_none(),
        "serialize_selection_with_mentions should return None so the default editor cut runs"
    );
}

#[gpui::test]
async fn test_paste_mention_link_with_completion_trigger_does_not_panic(cx: &mut TestAppContext) {
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

    let (_message_editor, editor) = workspace.update_in(&mut cx, |workspace, window, cx| {
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

    cx.simulate_input("@");

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(editor.text(cx), "@");
        assert!(editor.has_visible_completions_menu());
    });

    cx.write_to_clipboard(ClipboardItem::new_string("[@f](file:///test.txt) @".into()));
    cx.dispatch_action(Paste);

    editor.update(&mut cx, |editor, cx| {
        assert!(editor.text(cx).contains("[@f](file:///test.txt)"));
    });
}
