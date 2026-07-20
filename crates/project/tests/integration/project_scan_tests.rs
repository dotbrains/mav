use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_undo_encoding_change(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());

    // Create a file with ASCII content "Hi" - this will be detected as UTF-8
    // When reinterpreted as UTF-16LE, the bytes 0x48 0x69 become a single character
    let ascii_bytes: Vec<u8> = vec![0x48, 0x69];
    fs.insert_tree(path!("/dir"), json!({})).await;
    fs.insert_file(path!("/dir/test.txt"), ascii_bytes).await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    let buffer = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/test.txt"), cx))
        .await
        .unwrap();

    let (initial_encoding, initial_text, initial_dirty) = buffer.read_with(cx, |buffer, _| {
        (buffer.encoding(), buffer.text(), buffer.is_dirty())
    });
    assert_eq!(initial_encoding, encoding_rs::UTF_8);
    assert_eq!(initial_text, "Hi");
    assert!(!initial_dirty);

    let reload_receiver = buffer.update(cx, |buffer, cx| {
        buffer.reload_with_encoding(encoding_rs::UTF_16LE, cx)
    });
    cx.executor().run_until_parked();

    // Wait for reload to complete
    let _ = reload_receiver.await;

    // Verify the encoding changed, text is different, and still not dirty (we reloaded from disk)
    let (reloaded_encoding, reloaded_text, reloaded_dirty) = buffer.read_with(cx, |buffer, _| {
        (buffer.encoding(), buffer.text(), buffer.is_dirty())
    });
    assert_eq!(reloaded_encoding, encoding_rs::UTF_16LE);
    assert_eq!(reloaded_text, "楈");
    assert!(!reloaded_dirty);

    // Undo the reload
    buffer.update(cx, |buffer, cx| {
        buffer.undo(cx);
    });

    buffer.read_with(cx, |buffer, _| {
        assert_eq!(buffer.encoding(), encoding_rs::UTF_8);
        assert_eq!(buffer.text(), "Hi");
        assert!(!buffer.is_dirty());
    });

    buffer.update(cx, |buffer, cx| {
        buffer.redo(cx);
    });

    buffer.read_with(cx, |buffer, _| {
        assert_eq!(buffer.encoding(), encoding_rs::UTF_16LE);
        assert_ne!(buffer.text(), "Hi");
        assert!(!buffer.is_dirty());
    });
}

#[gpui::test]
async fn test_initial_scan_complete(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "a": {
                ".git": {},
                ".mav": {
                    "tasks.json": r#"[{"label": "task-a", "command": "echo a"}]"#
                },
                "src": { "main.rs": "" }
            },
            "b": {
                ".git": {},
                ".mav": {
                    "tasks.json": r#"[{"label": "task-b", "command": "echo b"}]"#
                },
                "src": { "lib.rs": "" }
            },
        }),
    )
    .await;

    let repos_created = Rc::new(RefCell::new(Vec::new()));
    let _observe = {
        let repos_created = repos_created.clone();
        cx.update(|cx| {
            cx.observe_new::<Repository>(move |repo, _, cx| {
                repos_created.borrow_mut().push(cx.entity().downgrade());
                let _ = repo;
            })
        })
    };

    let project = Project::test(
        fs.clone(),
        [path!("/root/a").as_ref(), path!("/root/b").as_ref()],
        cx,
    )
    .await;

    let scan_complete = project.read_with(cx, |project, cx| project.wait_for_initial_scan(cx));
    scan_complete.await;

    project.read_with(cx, |project, cx| {
        assert!(
            project.worktree_store().read(cx).initial_scan_completed(),
            "Expected initial scan to be completed after awaiting wait_for_initial_scan"
        );
    });

    let created_repos_len = repos_created.borrow().len();
    assert_eq!(
        created_repos_len, 2,
        "Expected 2 repositories to be created during scan, got {}",
        created_repos_len
    );

    project.read_with(cx, |project, cx| {
        let git_store = project.git_store().read(cx);
        assert_eq!(
            git_store.repositories().len(),
            2,
            "Expected 2 repositories in GitStore"
        );
    });
}
