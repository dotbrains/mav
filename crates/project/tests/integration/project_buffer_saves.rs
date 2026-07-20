use super::*;
use pretty_assertions::assert_eq;

#[gpui::test(iterations = 10)]
async fn test_save_file(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file1": "the old contents",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let buffer = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/file1"), cx))
        .await
        .unwrap();
    buffer.update(cx, |buffer, cx| {
        assert_eq!(buffer.text(), "the old contents");
        buffer.edit([(0..0, "a line of text.\n".repeat(10 * 1024))], None, cx);
    });

    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();

    let new_text = fs
        .load(Path::new(path!("/dir/file1")))
        .await
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(new_text, buffer.update(cx, |buffer, _| buffer.text()));
}

#[gpui::test(iterations = 10)]
async fn test_save_file_spawns_language_server(cx: &mut gpui::TestAppContext) {
    // Issue: #24349
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({})).await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    language_registry.add(rust_lang());
    let mut fake_rust_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "the-rust-language-server",
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), "::".to_string()]),
                    ..Default::default()
                }),
                text_document_sync: Some(lsp::TextDocumentSyncCapability::Options(
                    lsp::TextDocumentSyncOptions {
                        save: Some(lsp::TextDocumentSyncSaveOptions::Supported(true)),
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let buffer = project
        .update(cx, |this, cx| this.create_buffer(None, false, cx))
        .unwrap()
        .await;
    project.update(cx, |this, cx| {
        this.register_buffer_with_language_servers(&buffer, cx);
        buffer.update(cx, |buffer, cx| {
            assert!(!this.has_language_servers_for(buffer, cx));
        })
    });

    project
        .update(cx, |this, cx| {
            let worktree_id = this.worktrees(cx).next().unwrap().read(cx).id();
            this.save_buffer_as(
                buffer.clone(),
                ProjectPath {
                    worktree_id,
                    path: rel_path("file.rs").into(),
                },
                cx,
            )
        })
        .await
        .unwrap();
    // A server is started up, and it is notified about Rust files.
    let mut fake_rust_server = fake_rust_servers.next().await.unwrap();
    assert_eq!(
        fake_rust_server
            .receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await
            .text_document,
        lsp::TextDocumentItem {
            uri: lsp::Uri::from_file_path(path!("/dir/file.rs")).unwrap(),
            version: 0,
            text: "".to_string(),
            language_id: "rust".to_string(),
        }
    );

    project.update(cx, |this, cx| {
        buffer.update(cx, |buffer, cx| {
            assert!(this.has_language_servers_for(buffer, cx));
        })
    });
}

#[gpui::test(iterations = 30)]
async fn test_file_changes_multiple_times_on_disk(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file1": "the original contents",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let worktree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    let buffer = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/file1"), cx))
        .await
        .unwrap();

    // Change the buffer's file on disk, and then wait for the file change
    // to be detected by the worktree, so that the buffer starts reloading.
    fs.save(
        path!("/dir/file1").as_ref(),
        &"the first contents".into(),
        Default::default(),
    )
    .await
    .unwrap();
    worktree.next_event(cx).await;

    // Change the buffer's file again. Depending on the random seed, the
    // previous file change may still be in progress.
    fs.save(
        path!("/dir/file1").as_ref(),
        &"the second contents".into(),
        Default::default(),
    )
    .await
    .unwrap();
    worktree.next_event(cx).await;

    cx.executor().run_until_parked();
    let on_disk_text = fs.load(Path::new(path!("/dir/file1"))).await.unwrap();
    buffer.read_with(cx, |buffer, _| {
        assert_eq!(buffer.text(), on_disk_text);
        assert!(!buffer.is_dirty(), "buffer should not be dirty");
        assert!(!buffer.has_conflict(), "buffer should not be dirty");
    });
}

#[gpui::test(iterations = 30)]
async fn test_edit_buffer_while_it_reloads(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file1": "the original contents",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let worktree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    let buffer = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/file1"), cx))
        .await
        .unwrap();

    // Change the buffer's file on disk, and then wait for the file change
    // to be detected by the worktree, so that the buffer starts reloading.
    fs.save(
        path!("/dir/file1").as_ref(),
        &"the first contents".into(),
        Default::default(),
    )
    .await
    .unwrap();
    worktree.next_event(cx).await;

    cx.executor()
        .spawn(cx.executor().simulate_random_delay())
        .await;

    // Perform a noop edit, causing the buffer's version to increase.
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..0, " ")], None, cx);
        buffer.undo(cx);
    });

    cx.executor().run_until_parked();
    let on_disk_text = fs.load(Path::new(path!("/dir/file1"))).await.unwrap();
    buffer.read_with(cx, |buffer, _| {
        let buffer_text = buffer.text();
        if buffer_text == on_disk_text {
            assert!(
                !buffer.is_dirty() && !buffer.has_conflict(),
                "buffer shouldn't be dirty. text: {buffer_text:?}, disk text: {on_disk_text:?}",
            );
        }
        // If the file change occurred while the buffer was processing the first
        // change, the buffer will be in a conflicting state.
        else {
            assert!(buffer.is_dirty(), "buffer should report that it is dirty. text: {buffer_text:?}, disk text: {on_disk_text:?}");
            assert!(buffer.has_conflict(), "buffer should report that it is dirty. text: {buffer_text:?}, disk text: {on_disk_text:?}");
        }
    });
}

#[gpui::test]
async fn test_save_in_single_file_worktree(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file1": "the old contents",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir/file1").as_ref()], cx).await;
    let buffer = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/file1"), cx))
        .await
        .unwrap();
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "a line of text.\n".repeat(10 * 1024))], None, cx);
    });

    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();

    let new_text = fs
        .load(Path::new(path!("/dir/file1")))
        .await
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(new_text, buffer.update(cx, |buffer, _| buffer.text()));
}

#[gpui::test]
async fn test_save_as(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/dir", json!({})).await;

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;

    let languages = project.update(cx, |project, _| project.languages().clone());
    languages.add(rust_lang());

    let buffer = project.update(cx, |project, cx| {
        project.create_local_buffer("", None, false, cx)
    });
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "abc")], None, cx);
        assert!(buffer.is_dirty());
        assert!(!buffer.has_conflict());
        assert_eq!(buffer.language().unwrap().name(), "Plain Text");
    });
    project
        .update(cx, |project, cx| {
            let worktree_id = project.worktrees(cx).next().unwrap().read(cx).id();
            let path = ProjectPath {
                worktree_id,
                path: rel_path("file1.rs").into(),
            };
            project.save_buffer_as(buffer.clone(), path, cx)
        })
        .await
        .unwrap();
    assert_eq!(fs.load(Path::new("/dir/file1.rs")).await.unwrap(), "abc");

    cx.executor().run_until_parked();
    buffer.update(cx, |buffer, cx| {
        assert_eq!(
            buffer.file().unwrap().full_path(cx),
            Path::new("dir/file1.rs")
        );
        assert!(!buffer.is_dirty());
        assert!(!buffer.has_conflict());
        assert_eq!(buffer.language().unwrap().name(), "Rust");
    });

    let opened_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/file1.rs", cx)
        })
        .await
        .unwrap();
    assert_eq!(opened_buffer, buffer);
}

#[gpui::test]
async fn test_save_as_existing_file(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    fs.insert_tree(
        path!("/dir"),
        json!({
                "data_a.txt": "data about a"
        }),
    )
    .await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/data_a.txt"), cx)
        })
        .await
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(11..12, "b")], None, cx);
    });

    // Save buffer's contents as a new file and confirm that the buffer's now
    // associated with `data_b.txt` instead of `data_a.txt`, confirming that the
    // file associated with the buffer has now been updated to `data_b.txt`
    project
        .update(cx, |project, cx| {
            let worktree_id = project.worktrees(cx).next().unwrap().read(cx).id();
            let new_path = ProjectPath {
                worktree_id,
                path: rel_path("data_b.txt").into(),
            };

            project.save_buffer_as(buffer.clone(), new_path, cx)
        })
        .await
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        assert_eq!(
            buffer.file().unwrap().full_path(cx),
            Path::new("dir/data_b.txt")
        )
    });

    // Open the original `data_a.txt` file, confirming that its contents are
    // unchanged and the resulting buffer's associated file is `data_a.txt`.
    let original_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/data_a.txt"), cx)
        })
        .await
        .unwrap();

    original_buffer.update(cx, |buffer, cx| {
        assert_eq!(buffer.text(), "data about a");
        assert_eq!(
            buffer.file().unwrap().full_path(cx),
            Path::new("dir/data_a.txt")
        )
    });
}
