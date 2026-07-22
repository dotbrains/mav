use super::*;

#[gpui::test]
async fn test_deserialize(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.rs"), Default::default()).await;

    // Test case 1: Deserialize with path and contents
    {
        let project = Project::test(fs.clone(), [path!("/file.rs").as_ref()], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
        let db = cx.update(|_, cx| workspace::WorkspaceDb::global(cx));
        let workspace_id = db.next_id().await.unwrap();
        let editor_db = cx.update(|_, cx| EditorDb::global(cx));
        let item_id = 1234 as ItemId;
        let mtime = fs
            .metadata(Path::new(path!("/file.rs")))
            .await
            .unwrap()
            .unwrap()
            .mtime;

        let serialized_editor = SerializedEditor {
            abs_path: Some(PathBuf::from(path!("/file.rs"))),
            contents: Some("fn main() {}".to_string()),
            language: Some("Rust".to_string()),
            mtime: Some(mtime),
        };

        editor_db
            .save_serialized_editor(item_id, workspace_id, serialized_editor.clone())
            .await
            .unwrap();

        let deserialized = deserialize_editor(item_id, workspace_id, workspace, project, cx).await;

        deserialized.update(cx, |editor, cx| {
            assert_eq!(editor.text(cx), "fn main() {}");
            assert!(editor.is_dirty(cx));
            assert!(!editor.has_conflict(cx));
            let buffer = editor.buffer().read(cx).as_singleton().unwrap().read(cx);
            assert!(buffer.file().is_some());
        });
    }

    // Test case 2: Deserialize with only path
    {
        let project = Project::test(fs.clone(), [path!("/file.rs").as_ref()], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
        let db = cx.update(|_, cx| workspace::WorkspaceDb::global(cx));
        let editor_db = cx.update(|_, cx| EditorDb::global(cx));

        let workspace_id = db.next_id().await.unwrap();

        let item_id = 5678 as ItemId;
        let serialized_editor = SerializedEditor {
            abs_path: Some(PathBuf::from(path!("/file.rs"))),
            contents: None,
            language: None,
            mtime: None,
        };

        editor_db
            .save_serialized_editor(item_id, workspace_id, serialized_editor)
            .await
            .unwrap();

        let deserialized = deserialize_editor(item_id, workspace_id, workspace, project, cx).await;

        deserialized.update(cx, |editor, cx| {
            assert_eq!(editor.text(cx), ""); // The file should be empty as per our initial setup
            assert!(!editor.is_dirty(cx));
            assert!(!editor.has_conflict(cx));

            let buffer = editor.buffer().read(cx).as_singleton().unwrap().read(cx);
            assert!(buffer.file().is_some());
        });
    }

    // Test case 3: Deserialize with no path (untitled buffer, with content and language)
    {
        let project = Project::test(fs.clone(), [path!("/file.rs").as_ref()], cx).await;
        // Add Rust to the language, so that we can restore the language of the buffer
        project.read_with(cx, |project, _| {
            project.languages().add(languages::rust_lang())
        });

        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
        let db = cx.update(|_, cx| workspace::WorkspaceDb::global(cx));
        let editor_db = cx.update(|_, cx| EditorDb::global(cx));

        let workspace_id = db.next_id().await.unwrap();

        let item_id = 9012 as ItemId;
        let serialized_editor = SerializedEditor {
            abs_path: None,
            contents: Some("hello".to_string()),
            language: Some("Rust".to_string()),
            mtime: None,
        };

        editor_db
            .save_serialized_editor(item_id, workspace_id, serialized_editor)
            .await
            .unwrap();

        let deserialized = deserialize_editor(item_id, workspace_id, workspace, project, cx).await;

        deserialized.update(cx, |editor, cx| {
            assert_eq!(editor.text(cx), "hello");
            assert!(editor.is_dirty(cx)); // The editor should be dirty for an untitled buffer

            let buffer = editor.buffer().read(cx).as_singleton().unwrap().read(cx);
            assert_eq!(
                buffer.language().map(|lang| lang.name()),
                Some("Rust".into())
            ); // Language should be set to Rust
            assert!(buffer.file().is_none()); // The buffer should not have an associated file
        });
    }

    // Test case 4: Deserialize with path, content, and old mtime
    {
        let project = Project::test(fs.clone(), [path!("/file.rs").as_ref()], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
        let db = cx.update(|_, cx| workspace::WorkspaceDb::global(cx));
        let editor_db = cx.update(|_, cx| EditorDb::global(cx));

        let workspace_id = db.next_id().await.unwrap();

        let item_id = 9345 as ItemId;
        let old_mtime = MTime::from_seconds_and_nanos(0, 50);
        let serialized_editor = SerializedEditor {
            abs_path: Some(PathBuf::from(path!("/file.rs"))),
            contents: Some("fn main() {}".to_string()),
            language: Some("Rust".to_string()),
            mtime: Some(old_mtime),
        };

        editor_db
            .save_serialized_editor(item_id, workspace_id, serialized_editor)
            .await
            .unwrap();

        let deserialized = deserialize_editor(item_id, workspace_id, workspace, project, cx).await;

        deserialized.update(cx, |editor, cx| {
            assert_eq!(editor.text(cx), "fn main() {}");
            assert!(editor.has_conflict(cx)); // The editor should have a conflict
        });
    }

    // Test case 5: Deserialize with no path, no content, no language, and no old mtime (new, empty, unsaved buffer)
    {
        let project = Project::test(fs.clone(), [path!("/file.rs").as_ref()], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
        let db = cx.update(|_, cx| workspace::WorkspaceDb::global(cx));
        let editor_db = cx.update(|_, cx| EditorDb::global(cx));

        let workspace_id = db.next_id().await.unwrap();

        let item_id = 10000 as ItemId;
        let serialized_editor = SerializedEditor {
            abs_path: None,
            contents: None,
            language: None,
            mtime: None,
        };

        editor_db
            .save_serialized_editor(item_id, workspace_id, serialized_editor)
            .await
            .unwrap();

        let deserialized = deserialize_editor(item_id, workspace_id, workspace, project, cx).await;

        deserialized.update(cx, |editor, cx| {
            assert_eq!(editor.text(cx), "");
            assert!(!editor.is_dirty(cx));
            assert!(!editor.has_conflict(cx));

            let buffer = editor.buffer().read(cx).as_singleton().unwrap().read(cx);
            assert!(buffer.file().is_none());
        });
    }

    // Test case 6: Deserialize with path and contents in an empty workspace (no worktree)
    // This tests the hot-exit scenario where a file is opened in an empty workspace
    // and has unsaved changes that should be restored.
    {
        let fs = FakeFs::new(cx.executor());
        fs.insert_file(path!("/standalone.rs"), "original content".into())
            .await;

        // Create an empty project with no worktrees
        let project = Project::test(fs.clone(), [], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
        let db = cx.update(|_, cx| workspace::WorkspaceDb::global(cx));
        let editor_db = cx.update(|_, cx| EditorDb::global(cx));

        let workspace_id = db.next_id().await.unwrap();
        let item_id = 11000 as ItemId;

        let mtime = fs
            .metadata(Path::new(path!("/standalone.rs")))
            .await
            .unwrap()
            .unwrap()
            .mtime;

        // Simulate serialized state: file with unsaved changes
        let serialized_editor = SerializedEditor {
            abs_path: Some(PathBuf::from(path!("/standalone.rs"))),
            contents: Some("modified content".to_string()),
            language: Some("Rust".to_string()),
            mtime: Some(mtime),
        };

        editor_db
            .save_serialized_editor(item_id, workspace_id, serialized_editor)
            .await
            .unwrap();

        let deserialized = deserialize_editor(item_id, workspace_id, workspace, project, cx).await;

        deserialized.update(cx, |editor, cx| {
            // The editor should have the serialized contents, not the disk contents
            assert_eq!(editor.text(cx), "modified content");
            assert!(editor.is_dirty(cx));
            assert!(!editor.has_conflict(cx));

            let buffer = editor.buffer().read(cx).as_singleton().unwrap().read(cx);
            assert!(buffer.file().is_some());
        });
    }
}

// Verify that renaming an open file emits EditorEvent::FileHandleChanged so that
// the workspace re-serializes the editor with the updated path.
#[gpui::test]
async fn test_file_handle_changed_on_rename(cx: &mut gpui::TestAppContext) {
    use serde_json::json;
    use std::cell::RefCell;
    use std::rc::Rc;
    use util::rel_path::rel_path;

    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({ "file.rs": "fn main() {}" }))
        .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/root/file.rs"), cx)
        })
        .await
        .unwrap();

    let received_file_handle_changed = Rc::new(RefCell::new(false));
    let (editor, cx) = cx.add_window_view({
        let project = project.clone();
        let received_file_handle_changed = received_file_handle_changed.clone();
        move |window, cx| {
            let mut editor = Editor::for_buffer(buffer, Some(project), window, cx);
            editor.set_should_serialize(true, cx);
            let entity = cx.entity();
            cx.subscribe_in(&entity, window, move |_, _, event: &EditorEvent, _, _| {
                if matches!(event, EditorEvent::FileHandleChanged) {
                    *received_file_handle_changed.borrow_mut() = true;
                }
            })
            .detach();
            editor
        }
    });

    cx.run_until_parked();

    let (entry_id, worktree_id) = project.update(cx, |project, cx| {
        let worktree = project.worktrees(cx).next().unwrap();
        let worktree = worktree.read(cx);
        let entry = worktree.entry_for_path(rel_path("file.rs")).unwrap();
        (entry.id, worktree.id())
    });

    project
        .update(cx, |project, cx| {
            project.rename_entry(entry_id, (worktree_id, rel_path("renamed.rs")).into(), cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    assert!(
        *received_file_handle_changed.borrow(),
        "EditorEvent::FileHandleChanged must be emitted when the open file is renamed"
    );

    editor.update(cx, |editor, cx| {
        let buffer = editor.buffer().read(cx).as_singleton().unwrap();
        let path = buffer.read(cx).file().unwrap().path();
        assert!(
            path.as_std_path().ends_with("renamed.rs"),
            "buffer path must reflect the renamed file, got {path:?}"
        );
    });
}

// Regression test for https://github.com/mav-industries/mav/issues/35947
// Verifies that deserializing a non-worktree editor does not add the item
// to any pane as a side effect.
#[gpui::test]
async fn test_deserialize_non_worktree_file_does_not_add_to_pane(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/outside"), json!({ "settings.json": "{}" }))
        .await;

    // Project with a different root — settings.json is NOT in any worktree
    let project = Project::test(fs.clone(), [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let db = cx.update(|_, cx| workspace::WorkspaceDb::global(cx));
    let editor_db = cx.update(|_, cx| EditorDb::global(cx));

    let workspace_id = db.next_id().await.unwrap();
    let item_id = 99999 as ItemId;

    let serialized_editor = SerializedEditor {
        abs_path: Some(PathBuf::from(path!("/outside/settings.json"))),
        contents: None,
        language: None,
        mtime: None,
    };

    editor_db
        .save_serialized_editor(item_id, workspace_id, serialized_editor)
        .await
        .unwrap();

    // Count items in all panes before deserialization
    let pane_items_before = workspace.read_with(cx, |workspace, cx| {
        workspace
            .panes()
            .iter()
            .map(|pane| pane.read(cx).items_len())
            .sum::<usize>()
    });

    let deserialized =
        deserialize_editor(item_id, workspace_id, workspace.clone(), project, cx).await;

    cx.run_until_parked();

    // The editor should exist and have the file
    deserialized.update(cx, |editor, cx| {
        let buffer = editor.buffer().read(cx).as_singleton().unwrap().read(cx);
        assert!(buffer.file().is_some());
    });

    // No items should have been added to any pane as a side effect
    let pane_items_after = workspace.read_with(cx, |workspace, cx| {
        workspace
            .panes()
            .iter()
            .map(|pane| pane.read(cx).items_len())
            .sum::<usize>()
    });

    assert_eq!(
        pane_items_before, pane_items_after,
        "Editor::deserialize should not add items to panes as a side effect"
    );
}
