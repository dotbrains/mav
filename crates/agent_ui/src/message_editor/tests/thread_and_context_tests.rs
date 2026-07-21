use super::*;

#[gpui::test]
async fn test_large_file_mention_fallback(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());

    // Create a large file that exceeds AUTO_OUTLINE_SIZE
    // Using plain text without a configured language, so no outline is available
    const LINE: &str = "This is a line of text in the file\n";
    let large_content = LINE.repeat(2 * (outline::AUTO_OUTLINE_SIZE / LINE.len()));
    assert!(large_content.len() > outline::AUTO_OUTLINE_SIZE);

    // Create a small file that doesn't exceed AUTO_OUTLINE_SIZE
    let small_content = "fn small_function() { /* small */ }\n";
    assert!(small_content.len() < outline::AUTO_OUTLINE_SIZE);

    fs.insert_tree(
        "/project",
        json!({
            "large_file.txt": large_content.clone(),
            "small_file.txt": small_content,
        }),
    )
    .await;

    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = Some(cx.new(|cx| ThreadStore::new(cx)));

    let message_editor = cx.update(|window, cx| {
        cx.new(|cx| {
            let editor = MessageEditor::new(
                workspace.downgrade(),
                project.downgrade(),
                thread_store.clone(),
                Default::default(),
                "Test Agent".into(),
                "Test",
                EditorMode::AutoHeight {
                    min_lines: 1,
                    max_lines: None,
                },
                window,
                cx,
            );
            // Enable embedded context so files are actually included
            editor
                .session_capabilities
                .write()
                .set_prompt_capabilities(acp::PromptCapabilities::new().embedded_context(true));
            editor
        })
    });

    // Test large file mention
    // Get the absolute path using the project's worktree
    let large_file_abs_path = project.read_with(cx, |project, cx| {
        let worktree = project.worktrees(cx).next().unwrap();
        let worktree_root = worktree.read(cx).abs_path();
        worktree_root.join("large_file.txt")
    });
    let large_file_task = message_editor.update(cx, |editor, cx| {
        editor.mention_set().update(cx, |set, cx| {
            set.confirm_mention_for_file(large_file_abs_path, true, cx)
        })
    });

    let large_file_mention = large_file_task.await.unwrap();
    match large_file_mention {
        Mention::Text { content, .. } => {
            // Should contain some of the content but not all of it
            assert!(
                content.contains(LINE),
                "Should contain some of the file content"
            );
            assert!(
                !content.contains(&LINE.repeat(100)),
                "Should not contain the full file"
            );
            // Should be much smaller than original
            assert!(
                content.len() < large_content.len() / 10,
                "Should be significantly truncated"
            );
        }
        _ => panic!("Expected Text mention for large file"),
    }

    // Test small file mention
    // Get the absolute path using the project's worktree
    let small_file_abs_path = project.read_with(cx, |project, cx| {
        let worktree = project.worktrees(cx).next().unwrap();
        let worktree_root = worktree.read(cx).abs_path();
        worktree_root.join("small_file.txt")
    });
    let small_file_task = message_editor.update(cx, |editor, cx| {
        editor.mention_set().update(cx, |set, cx| {
            set.confirm_mention_for_file(small_file_abs_path, true, cx)
        })
    });

    let small_file_mention = small_file_task.await.unwrap();
    match small_file_mention {
        Mention::Text { content, .. } => {
            // Should contain the full actual content
            assert_eq!(content, small_content);
        }
        _ => panic!("Expected Text mention for small file"),
    }
}

#[gpui::test]
async fn test_insert_thread_summary(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(LanguageModelRegistry::test);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({"file": ""})).await;
    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = Some(cx.new(|cx| ThreadStore::new(cx)));

    let session_id = acp::SessionId::new("thread-123");
    let title = Some("Previous Conversation".into());

    let message_editor = cx.update(|window, cx| {
        cx.new(|cx| {
            let mut editor = MessageEditor::new(
                workspace.downgrade(),
                project.downgrade(),
                thread_store.clone(),
                Default::default(),
                "Test Agent".into(),
                "Test",
                EditorMode::AutoHeight {
                    min_lines: 1,
                    max_lines: None,
                },
                window,
                cx,
            );
            editor.insert_thread_summary(session_id.clone(), title.clone(), window, cx);
            editor
        })
    });

    // Construct expected values for verification
    let expected_uri = MentionUri::Thread {
        id: session_id.clone(),
        name: title.as_ref().unwrap().to_string(),
    };
    let expected_title = title.as_ref().unwrap();
    let expected_link = format!("[@{}]({})", expected_title, expected_uri.to_uri());

    message_editor.read_with(cx, |editor, cx| {
        let text = editor.text(cx);

        assert!(
            text.contains(&expected_link),
            "Expected editor text to contain thread mention link.\nExpected substring: {}\nActual text: {}",
            expected_link,
            text
        );

        let mentions = editor.mention_set().read(cx).mentions();
        assert_eq!(
            mentions.len(),
            1,
            "Expected exactly one mention after inserting thread summary"
        );

        assert!(
            mentions.contains(&expected_uri),
            "Expected mentions to contain the thread URI"
        );
    });
}

#[gpui::test]
async fn test_insert_thread_summary_skipped_for_external_agents(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(LanguageModelRegistry::test);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({"file": ""})).await;
    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = None;

    let message_editor = cx.update(|window, cx| {
        cx.new(|cx| {
            let mut editor = MessageEditor::new(
                workspace.downgrade(),
                project.downgrade(),
                thread_store.clone(),
                Default::default(),
                "Test Agent".into(),
                "Test",
                EditorMode::AutoHeight {
                    min_lines: 1,
                    max_lines: None,
                },
                window,
                cx,
            );
            editor.insert_thread_summary(
                acp::SessionId::new("thread-123"),
                Some("Previous Conversation".into()),
                window,
                cx,
            );
            editor
        })
    });

    message_editor.read_with(cx, |editor, cx| {
        assert!(
            editor.text(cx).is_empty(),
            "Expected thread summary to be skipped for external agents"
        );
        assert!(
            editor.mention_set().read(cx).mentions().is_empty(),
            "Expected no mentions when thread summary is skipped"
        );
    });
}

mod thread_mode_tests;
