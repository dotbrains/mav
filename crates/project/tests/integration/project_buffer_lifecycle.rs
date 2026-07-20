use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_buffer_deduping(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/dir",
        json!({
            "a.txt": "a-contents",
            "b.txt": "b-contents",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;

    // Spawn multiple tasks to open paths, repeating some paths.
    let (buffer_a_1, buffer_b, buffer_a_2) = project.update(cx, |p, cx| {
        (
            p.open_local_buffer("/dir/a.txt", cx),
            p.open_local_buffer("/dir/b.txt", cx),
            p.open_local_buffer("/dir/a.txt", cx),
        )
    });

    let buffer_a_1 = buffer_a_1.await.unwrap();
    let buffer_a_2 = buffer_a_2.await.unwrap();
    let buffer_b = buffer_b.await.unwrap();
    assert_eq!(buffer_a_1.update(cx, |b, _| b.text()), "a-contents");
    assert_eq!(buffer_b.update(cx, |b, _| b.text()), "b-contents");

    // There is only one buffer per path.
    let buffer_a_id = buffer_a_1.entity_id();
    assert_eq!(buffer_a_2.entity_id(), buffer_a_id);

    // Open the same path again while it is still open.
    drop(buffer_a_1);
    let buffer_a_3 = project
        .update(cx, |p, cx| p.open_local_buffer("/dir/a.txt", cx))
        .await
        .unwrap();

    // There's still only one buffer per path.
    assert_eq!(buffer_a_3.entity_id(), buffer_a_id);
}

#[gpui::test]
async fn test_buffer_is_dirty(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file1": "abc",
            "file2": "def",
            "file3": "ghi",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    let buffer1 = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/file1"), cx))
        .await
        .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));

    // initially, the buffer isn't dirty.
    buffer1.update(cx, |buffer, cx| {
        cx.subscribe(&buffer1, {
            let events = events.clone();
            move |_, _, event, _| match event {
                BufferEvent::Operation { .. } => {}
                _ => events.lock().push(event.clone()),
            }
        })
        .detach();

        assert!(!buffer.is_dirty());
        assert!(events.lock().is_empty());

        buffer.edit([(1..2, "")], None, cx);
    });

    // after the first edit, the buffer is dirty, and emits a dirtied event.
    buffer1.update(cx, |buffer, cx| {
        assert!(buffer.text() == "ac");
        assert!(buffer.is_dirty());
        assert_eq!(
            *events.lock(),
            &[
                language::BufferEvent::Edited {
                    source: language::BufferEditSource::User
                },
                language::BufferEvent::DirtyChanged
            ]
        );
        events.lock().clear();
        buffer.did_save(
            buffer.version(),
            buffer.file().unwrap().disk_state().mtime(),
            cx,
        );
    });

    // after saving, the buffer is not dirty, and emits a saved event.
    buffer1.update(cx, |buffer, cx| {
        assert!(!buffer.is_dirty());
        assert_eq!(*events.lock(), &[language::BufferEvent::Saved]);
        events.lock().clear();

        buffer.edit([(1..1, "B")], None, cx);
        buffer.edit([(2..2, "D")], None, cx);
    });

    // after editing again, the buffer is dirty, and emits another dirty event.
    buffer1.update(cx, |buffer, cx| {
        assert!(buffer.text() == "aBDc");
        assert!(buffer.is_dirty());
        assert_eq!(
            *events.lock(),
            &[
                language::BufferEvent::Edited {
                    source: language::BufferEditSource::User
                },
                language::BufferEvent::DirtyChanged,
                language::BufferEvent::Edited {
                    source: language::BufferEditSource::User
                },
            ],
        );
        events.lock().clear();

        // After restoring the buffer to its previously-saved state,
        // the buffer is not considered dirty anymore.
        buffer.edit([(1..3, "")], None, cx);
        assert!(buffer.text() == "ac");
        assert!(!buffer.is_dirty());
    });

    assert_eq!(
        *events.lock(),
        &[
            language::BufferEvent::Edited {
                source: language::BufferEditSource::User
            },
            language::BufferEvent::DirtyChanged
        ]
    );

    // When a file is deleted, it is not considered dirty.
    let events = Arc::new(Mutex::new(Vec::new()));
    let buffer2 = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/file2"), cx))
        .await
        .unwrap();
    buffer2.update(cx, |_, cx| {
        cx.subscribe(&buffer2, {
            let events = events.clone();
            move |_, _, event, _| match event {
                BufferEvent::Operation { .. } => {}
                _ => events.lock().push(event.clone()),
            }
        })
        .detach();
    });

    fs.remove_file(path!("/dir/file2").as_ref(), Default::default())
        .await
        .unwrap();
    cx.executor().run_until_parked();
    buffer2.update(cx, |buffer, _| assert!(!buffer.is_dirty()));
    assert_eq!(
        mem::take(&mut *events.lock()),
        &[language::BufferEvent::FileHandleChanged]
    );

    // Buffer becomes dirty when edited.
    buffer2.update(cx, |buffer, cx| {
        buffer.edit([(2..3, "")], None, cx);
        assert_eq!(buffer.is_dirty(), true);
    });
    assert_eq!(
        mem::take(&mut *events.lock()),
        &[
            language::BufferEvent::Edited {
                source: language::BufferEditSource::User
            },
            language::BufferEvent::DirtyChanged
        ]
    );

    // Buffer becomes clean again when all of its content is removed, because
    // the file was deleted.
    buffer2.update(cx, |buffer, cx| {
        buffer.edit([(0..2, "")], None, cx);
        assert_eq!(buffer.is_empty(), true);
        assert_eq!(buffer.is_dirty(), false);
    });
    assert_eq!(
        *events.lock(),
        &[
            language::BufferEvent::Edited {
                source: language::BufferEditSource::User
            },
            language::BufferEvent::DirtyChanged
        ]
    );

    // When a file is already dirty when deleted, we don't emit a Dirtied event.
    let events = Arc::new(Mutex::new(Vec::new()));
    let buffer3 = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/file3"), cx))
        .await
        .unwrap();
    buffer3.update(cx, |_, cx| {
        cx.subscribe(&buffer3, {
            let events = events.clone();
            move |_, _, event, _| match event {
                BufferEvent::Operation { .. } => {}
                _ => events.lock().push(event.clone()),
            }
        })
        .detach();
    });

    buffer3.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "x")], None, cx);
    });
    events.lock().clear();
    fs.remove_file(path!("/dir/file3").as_ref(), Default::default())
        .await
        .unwrap();
    cx.executor().run_until_parked();
    assert_eq!(*events.lock(), &[language::BufferEvent::FileHandleChanged]);
    cx.update(|cx| assert!(buffer3.read(cx).is_dirty()));
}

#[gpui::test]
async fn test_dirty_buffer_reloads_after_undo(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file.txt": "version 1",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let buffer = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/file.txt"), cx))
        .await
        .unwrap();

    buffer.read_with(cx, |buffer, _| {
        assert_eq!(buffer.text(), "version 1");
        assert!(!buffer.is_dirty());
    });

    // User makes an edit, making the buffer dirty.
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "user edit: ")], None, cx);
    });

    buffer.read_with(cx, |buffer, _| {
        assert!(buffer.is_dirty());
        assert_eq!(buffer.text(), "user edit: version 1");
    });

    // External tool writes new content while buffer is dirty.
    // file_updated() updates the File but suppresses ReloadNeeded.
    fs.save(
        path!("/dir/file.txt").as_ref(),
        &"version 2 from external tool".into(),
        Default::default(),
    )
    .await
    .unwrap();
    cx.executor().run_until_parked();

    buffer.read_with(cx, |buffer, _| {
        assert!(buffer.has_conflict());
        assert_eq!(buffer.text(), "user edit: version 1");
    });

    // User undoes their edit. Buffer becomes clean, but disk has different
    // content. did_edit() detects the dirty->clean transition and checks if
    // disk changed while dirty. Since mtime differs from saved_mtime, it
    // emits ReloadNeeded.
    buffer.update(cx, |buffer, cx| {
        buffer.undo(cx);
    });
    cx.executor().run_until_parked();

    buffer.read_with(cx, |buffer, _| {
        assert_eq!(
            buffer.text(),
            "version 2 from external tool",
            "buffer should reload from disk after undo makes it clean"
        );
        assert!(!buffer.is_dirty());
    });
}

#[gpui::test]
async fn test_buffer_file_change_to_binary_fails(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file.txt": "",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let buffer = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/file.txt"), cx))
        .await
        .unwrap();

    fs.write(
        path!("/dir/file.txt").as_ref(),
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01",
    )
    .await
    .unwrap();
    cx.executor().run_until_parked();

    // Test that existing buffer is left untouched
    buffer.read_with(cx, |buffer, _| {
        assert_eq!(buffer.text(), "");
    });
}

#[gpui::test]
async fn test_buffer_file_changes_on_disk(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let (initial_contents, initial_offsets) =
        marked_text_offsets("one twoˇ\nthree ˇfourˇ five\nsixˇ seven\n");
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "the-file": initial_contents,
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let buffer = project
        .update(cx, |p, cx| p.open_local_buffer(path!("/dir/the-file"), cx))
        .await
        .unwrap();

    let anchors = initial_offsets
        .iter()
        .map(|offset| buffer.update(cx, |b, _| b.anchor_before(offset)))
        .collect::<Vec<_>>();

    // Change the file on disk, adding two new lines of text, and removing
    // one line.
    buffer.update(cx, |buffer, _| {
        assert!(!buffer.is_dirty());
        assert!(!buffer.has_conflict());
    });

    let (new_contents, new_offsets) =
        marked_text_offsets("oneˇ\nthree ˇFOURˇ five\nsixtyˇ seven\n");
    fs.save(
        path!("/dir/the-file").as_ref(),
        &new_contents.as_str().into(),
        LineEnding::Unix,
    )
    .await
    .unwrap();

    // Because the buffer was not modified, it is reloaded from disk. Its
    // contents are edited according to the diff between the old and new
    // file contents.
    cx.executor().run_until_parked();
    buffer.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), new_contents);
        assert!(!buffer.is_dirty());
        assert!(!buffer.has_conflict());

        let anchor_offsets = anchors
            .iter()
            .map(|anchor| anchor.to_offset(&*buffer))
            .collect::<Vec<_>>();
        assert_eq!(anchor_offsets, new_offsets);
    });

    // Modify the buffer
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..0, " ")], None, cx);
        assert!(buffer.is_dirty());
        assert!(!buffer.has_conflict());
    });

    // Change the file on disk again, adding blank lines to the beginning.
    fs.save(
        path!("/dir/the-file").as_ref(),
        &"\n\n\nAAAA\naaa\nBB\nbbbbb\n".into(),
        LineEnding::Unix,
    )
    .await
    .unwrap();

    // Because the buffer is modified, it doesn't reload from disk, but is
    // marked as having a conflict.
    cx.executor().run_until_parked();
    buffer.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), " ".to_string() + &new_contents);
        assert!(buffer.has_conflict());
    });
}
