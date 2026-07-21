use super::*;

#[gpui::test]
async fn test_paste_external_file_path_inserts_file_mention(cx: &mut TestAppContext) {
    init_test(cx);
    let (message_editor, editor, mut cx) =
        setup_paste_test_message_editor(json!({"file.txt": "content"}), cx).await;
    paste_external_paths(
        &message_editor,
        vec![PathBuf::from(path!("/project/file.txt"))],
        &mut cx,
    );

    let expected_uri = MentionUri::File {
        abs_path: path!("/project/file.txt").into(),
    }
    .to_uri()
    .to_string();

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(editor.text(cx), format!("[@file.txt]({expected_uri}) "));
    });

    let contents = mention_contents(&message_editor, &mut cx).await;

    let [(uri, Mention::Text { content, .. })] = contents.as_slice() else {
        panic!("Unexpected mentions");
    };
    assert_eq!(content, "content");
    assert_eq!(
        uri,
        &MentionUri::File {
            abs_path: path!("/project/file.txt").into(),
        }
    );
}

#[gpui::test]
async fn test_paste_external_directory_path_inserts_directory_mention(cx: &mut TestAppContext) {
    init_test(cx);
    let (message_editor, editor, mut cx) = setup_paste_test_message_editor(
        json!({
            "src": {
                "main.rs": "fn main() {}\n",
            }
        }),
        cx,
    )
    .await;
    paste_external_paths(
        &message_editor,
        vec![PathBuf::from(path!("/project/src"))],
        &mut cx,
    );

    let expected_uri = MentionUri::Directory {
        abs_path: path!("/project/src").into(),
    }
    .to_uri()
    .to_string();

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(editor.text(cx), format!("[@src]({expected_uri}) "));
    });

    let contents = mention_contents(&message_editor, &mut cx).await;

    let [(uri, Mention::Link)] = contents.as_slice() else {
        panic!("Unexpected mentions");
    };
    assert_eq!(
        uri,
        &MentionUri::Directory {
            abs_path: path!("/project/src").into(),
        }
    );
}

#[gpui::test]
async fn test_paste_external_file_path_inserts_at_cursor(cx: &mut TestAppContext) {
    init_test(cx);
    let (message_editor, editor, mut cx) =
        setup_paste_test_message_editor(json!({"file.txt": "content"}), cx).await;

    editor.update_in(&mut cx, |editor, window, cx| {
        editor.set_text("Hello world", window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_ranges([MultiBufferOffset(6)..MultiBufferOffset(6)]);
        });
    });

    paste_external_paths(
        &message_editor,
        vec![PathBuf::from(path!("/project/file.txt"))],
        &mut cx,
    );

    let expected_uri = MentionUri::File {
        abs_path: path!("/project/file.txt").into(),
    }
    .to_uri()
    .to_string();

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(
            editor.text(cx),
            format!("Hello [@file.txt]({expected_uri}) world")
        );
    });
}

#[gpui::test]
async fn test_dragged_file_path_inserts_at_cursor(cx: &mut TestAppContext) {
    init_test(cx);
    let (message_editor, editor, mut cx) =
        setup_paste_test_message_editor(json!({"file.txt": "content"}), cx).await;

    editor.update_in(&mut cx, |editor, window, cx| {
        editor.set_text("Hello world", window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_ranges([MultiBufferOffset(6)..MultiBufferOffset(6)]);
        });
    });

    insert_dragged_project_paths(&message_editor, vec!["file.txt"], &mut cx);

    let expected_uri = MentionUri::File {
        abs_path: path!("/project/file.txt").into(),
    }
    .to_uri()
    .to_string();

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(
            editor.text(cx),
            format!("Hello [@file.txt]({expected_uri}) world")
        );
    });

    let contents = mention_contents(&message_editor, &mut cx).await;

    let [(uri, Mention::Text { content, .. })] = contents.as_slice() else {
        panic!("Unexpected mentions");
    };
    assert_eq!(content, "content");
    assert_eq!(
        uri,
        &MentionUri::File {
            abs_path: path!("/project/file.txt").into(),
        }
    );
}

#[gpui::test]
async fn test_dragged_file_paths_insert_in_order_at_cursor(cx: &mut TestAppContext) {
    init_test(cx);
    let (message_editor, editor, mut cx) = setup_paste_test_message_editor(
        json!({
            "one.txt": "one",
            "two.txt": "two",
        }),
        cx,
    )
    .await;

    editor.update_in(&mut cx, |editor, window, cx| {
        editor.set_text("Hello world", window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_ranges([MultiBufferOffset(6)..MultiBufferOffset(6)]);
        });
    });

    insert_dragged_project_paths(&message_editor, vec!["one.txt", "two.txt"], &mut cx);

    let first_uri = MentionUri::File {
        abs_path: path!("/project/one.txt").into(),
    }
    .to_uri()
    .to_string();
    let second_uri = MentionUri::File {
        abs_path: path!("/project/two.txt").into(),
    }
    .to_uri()
    .to_string();

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(
            editor.text(cx),
            format!("Hello [@one.txt]({first_uri}) [@two.txt]({second_uri}) world")
        );
    });
}

#[gpui::test]
async fn test_paste_mixed_external_image_without_extension_and_file_path(cx: &mut TestAppContext) {
    init_test(cx);
    let (message_editor, editor, mut cx) =
        setup_paste_test_message_editor(json!({"file.txt": "content"}), cx).await;

    message_editor.update(&mut cx, |message_editor, _cx| {
        message_editor
            .session_capabilities
            .write()
            .set_prompt_capabilities(acp::PromptCapabilities::new().image(true));
    });

    let temporary_image_path = write_test_png_file(None);
    paste_external_paths(
        &message_editor,
        vec![
            temporary_image_path.clone(),
            PathBuf::from(path!("/project/file.txt")),
        ],
        &mut cx,
    );

    let image_name = temporary_image_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Image")
        .to_string();
    std::fs::remove_file(&temporary_image_path).expect("remove temp png");

    let expected_file_uri = MentionUri::File {
        abs_path: path!("/project/file.txt").into(),
    }
    .to_uri()
    .to_string();
    let expected_image_uri = MentionUri::PastedImage {
        name: image_name.clone(),
    }
    .to_uri()
    .to_string();

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(
            editor.text(cx),
            format!("[@{image_name}]({expected_image_uri}) [@file.txt]({expected_file_uri}) ")
        );
    });

    let contents = mention_contents(&message_editor, &mut cx).await;

    assert_eq!(contents.len(), 2);
    assert!(contents.iter().any(|(uri, mention)| {
        matches!(uri, MentionUri::PastedImage { .. }) && matches!(mention, Mention::Image(_))
    }));
    assert!(contents.iter().any(|(uri, mention)| {
        *uri == MentionUri::File {
            abs_path: path!("/project/file.txt").into(),
        } && matches!(
            mention,
            Mention::Text {
                content,
                tracked_buffers: _,
            } if content == "content"
        )
    }));
}

async fn setup_paste_test_message_editor(
    project_tree: Value,
    cx: &mut TestAppContext,
) -> (Entity<MessageEditor>, Entity<Editor>, VisualTestContext) {
    let app_state = cx.update(AppState::test);

    cx.update(|cx| {
        editor::init(cx);
        workspace::init(app_state.clone(), cx);
    });

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/project"), project_tree)
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

    (message_editor, editor, cx)
}

fn paste_external_paths(
    message_editor: &Entity<MessageEditor>,
    paths: Vec<PathBuf>,
    cx: &mut VisualTestContext,
) {
    cx.write_to_clipboard(ClipboardItem {
        entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(paths.into()))],
    });

    message_editor.update_in(cx, |message_editor, window, cx| {
        message_editor.paste(&Paste, window, cx);
    });
    cx.run_until_parked();
}

fn insert_dragged_project_paths(
    message_editor: &Entity<MessageEditor>,
    paths: Vec<&str>,
    cx: &mut VisualTestContext,
) {
    message_editor.update_in(cx, |message_editor, window, cx| {
        let workspace = message_editor
            .workspace
            .upgrade()
            .expect("message editor should keep workspace alive");
        let project = workspace.read(cx).project().clone();
        let worktree_id = project.update(cx, |project, cx| {
            let mut worktrees = project.worktrees(cx).collect::<Vec<_>>();
            assert_eq!(worktrees.len(), 1, "expected a single worktree");
            worktrees.pop().unwrap().read(cx).id()
        });

        let paths = paths
            .into_iter()
            .map(|path| ProjectPath {
                worktree_id,
                path: rel_path(path).into(),
            })
            .collect();

        message_editor.insert_dragged_files(paths, vec![], window, cx);
    });
    cx.run_until_parked();
}

async fn mention_contents(
    message_editor: &Entity<MessageEditor>,
    cx: &mut VisualTestContext,
) -> Vec<(MentionUri, Mention)> {
    message_editor
        .update(cx, |message_editor, cx| {
            message_editor
                .mention_set()
                .update(cx, |mention_set, cx| mention_set.contents(false, cx))
        })
        .await
        .unwrap()
        .into_values()
        .collect::<Vec<_>>()
}

fn write_test_png_file(extension: Option<&str>) -> PathBuf {
    let bytes = base64::prelude::BASE64_STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==")
        .expect("decode png");
    let file_name = match extension {
        Some(extension) => format!("mav-agent-ui-test-{}.{}", uuid::Uuid::new_v4(), extension),
        None => format!("mav-agent-ui-test-{}", uuid::Uuid::new_v4()),
    };
    let path = std::env::temp_dir().join(file_name);
    std::fs::write(&path, bytes).expect("write temp png");
    path
}

// Helper that creates a minimal MessageEditor inside a window, returning both
// the entity and the underlying VisualTestContext so callers can drive updates.
async fn setup_message_editor(
    cx: &mut TestAppContext,
) -> (Entity<MessageEditor>, &mut VisualTestContext) {
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({"file.txt": ""})).await;
    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let message_editor = cx.update(|window, cx| {
        cx.new(|cx| {
            MessageEditor::new(
                workspace.downgrade(),
                project.downgrade(),
                None,
                Default::default(),
                "Test Agent".into(),
                "Test",
                EditorMode::AutoHeight {
                    min_lines: 1,
                    max_lines: None,
                },
                window,
                cx,
            )
        })
    });

    cx.run_until_parked();
    (message_editor, cx)
}
