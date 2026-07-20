use crate::bookmark_store::*;

#[gpui::test]
async fn test_with_serialized_bookmarks_restores_bookmarks(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            "file1.rs": "line1\nline2\nline3\nline4\nline5\n",
            "file2.rs": "aaa\nbbb\nccc\n"
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;

    let serialized = build_serialized(&[
        (path!("/project/file1.rs"), &[0, 3]),
        (path!("/project/file2.rs"), &[1]),
    ]);

    restore_bookmarks(&project, serialized, cx).await;

    let restored = get_all_bookmarks(&project, cx);
    assert_eq!(restored.len(), 2);
    assert_bookmark_rows(&restored, path!("/project/file1.rs"), &[0, 3]);
    assert_bookmark_rows(&restored, path!("/project/file2.rs"), &[1]);
}

#[gpui::test]
async fn test_with_serialized_bookmarks_skips_out_of_range_rows(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    // 3 lines: rows 0, 1, 2
    fs.insert_tree(
        path!("/project"),
        json!({"file1.rs": "line1\nline2\nline3"}),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;

    let serialized = build_serialized(&[(path!("/project/file1.rs"), &[1, 100, 2])]);
    restore_bookmarks(&project, serialized, cx).await;

    // Before resolution, unloaded bookmarks are stored as-is
    let unresolved = get_all_bookmarks(&project, cx);
    assert_bookmark_rows(&unresolved, path!("/project/file1.rs"), &[1, 2, 100]);

    // Open the buffer to trigger lazy resolution
    let buffer = open_buffer(&project, path!("/project/file1.rs"), cx).await;
    project.update(cx, |project, cx| {
        let buffer_snapshot = buffer.read(cx).snapshot();
        project.bookmark_store().update(cx, |store, cx| {
            store.bookmarks_for_buffer(
                buffer.clone(),
                buffer_snapshot.anchor_before(0)
                    ..buffer_snapshot.anchor_after(buffer_snapshot.len()),
                &buffer_snapshot,
                cx,
            );
        });
    });

    // After resolution, out-of-range rows are filtered
    let restored = get_all_bookmarks(&project, cx);
    assert_bookmark_rows(&restored, path!("/project/file1.rs"), &[1, 2]);
}

#[gpui::test]
async fn test_with_serialized_bookmarks_skips_empty_entries(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({"file1.rs": "line1\nline2\n", "file2.rs": "aaa\nbbb\n"}),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;

    let mut serialized = build_serialized(&[(path!("/project/file1.rs"), &[0])]);
    serialized.insert(project_path(path!("/project/file2.rs")), vec![]);

    restore_bookmarks(&project, serialized, cx).await;

    let restored = get_all_bookmarks(&project, cx);
    assert_eq!(restored.len(), 1);
    assert!(restored.contains_key(&project_path(path!("/project/file1.rs"))));
    assert!(!restored.contains_key(&project_path(path!("/project/file2.rs"))));
}

#[gpui::test]
async fn test_with_serialized_bookmarks_all_out_of_range_produces_no_entry(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(path!("/project"), json!({"tiny.rs": "x"}))
        .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;

    let serialized = build_serialized(&[(path!("/project/tiny.rs"), &[5, 10])]);
    restore_bookmarks(&project, serialized, cx).await;

    // Before resolution, unloaded bookmarks are stored as-is
    let unresolved = get_all_bookmarks(&project, cx);
    assert_eq!(unresolved.len(), 1);

    // Open the buffer to trigger lazy resolution
    let buffer = open_buffer(&project, path!("/project/tiny.rs"), cx).await;
    project.update(cx, |project, cx| {
        let buffer_snapshot = buffer.read(cx).snapshot();
        project.bookmark_store().update(cx, |store, cx| {
            store.bookmarks_for_buffer(
                buffer.clone(),
                buffer_snapshot.anchor_before(0)
                    ..buffer_snapshot.anchor_after(buffer_snapshot.len()),
                &buffer_snapshot,
                cx,
            );
        });
    });

    // After resolution, all out-of-range rows are filtered away
    assert!(get_all_bookmarks(&project, cx).is_empty());
}

#[gpui::test]
async fn test_with_serialized_bookmarks_replaces_existing(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({"file1.rs": "aaa\nbbb\nccc\nddd\n"}),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let buffer = open_buffer(&project, path!("/project/file1.rs"), cx).await;

    add_bookmarks(&project, &buffer, &[0], cx);
    assert_bookmark_rows(
        &get_all_bookmarks(&project, cx),
        path!("/project/file1.rs"),
        &[0],
    );

    // Restoring different bookmarks should replace, not merge
    let serialized = build_serialized(&[(path!("/project/file1.rs"), &[2, 3])]);
    restore_bookmarks(&project, serialized, cx).await;

    let after = get_all_bookmarks(&project, cx);
    assert_eq!(after.len(), 1);
    assert_bookmark_rows(&after, path!("/project/file1.rs"), &[2, 3]);
}

#[gpui::test]
async fn test_serialize_deserialize_round_trip(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            "alpha.rs": "fn main() {\n    println!(\"hello\");\n    return;\n}\n",
            "beta.rs": "use std::io;\nfn read() {}\nfn write() {}\n"
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let buffer_alpha = open_buffer(&project, path!("/project/alpha.rs"), cx).await;
    let buffer_beta = open_buffer(&project, path!("/project/beta.rs"), cx).await;

    add_bookmarks(&project, &buffer_alpha, &[0, 2, 3], cx);
    add_bookmarks(&project, &buffer_beta, &[1], cx);

    // Serialize
    let serialized = get_all_bookmarks(&project, cx);
    assert_eq!(serialized.len(), 2);
    assert_bookmark_rows(&serialized, path!("/project/alpha.rs"), &[0, 2, 3]);
    assert_bookmark_rows(&serialized, path!("/project/beta.rs"), &[1]);

    // Clear and restore
    clear_bookmarks(&project, cx);
    assert!(get_all_bookmarks(&project, cx).is_empty());

    restore_bookmarks(&project, serialized, cx).await;

    let restored = get_all_bookmarks(&project, cx);
    assert_eq!(restored.len(), 2);
    assert_bookmark_rows(&restored, path!("/project/alpha.rs"), &[0, 2, 3]);
    assert_bookmark_rows(&restored, path!("/project/beta.rs"), &[1]);
}

#[gpui::test]
async fn test_round_trip_preserves_bookmarks_after_file_edit(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({"file.rs": "aaa\nbbb\nccc\nddd\neee\n"}),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let buffer = open_buffer(&project, path!("/project/file.rs"), cx).await;

    add_bookmarks(&project, &buffer, &[1, 3], cx);

    // Insert a line at the beginning, shifting bookmarks down by 1
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "new_first_line\n")], None, cx);
    });

    let serialized = get_all_bookmarks(&project, cx);
    assert_bookmark_rows(&serialized, path!("/project/file.rs"), &[2, 4]);

    // Clear and restore
    clear_bookmarks(&project, cx);
    restore_bookmarks(&project, serialized, cx).await;

    let restored = get_all_bookmarks(&project, cx);
    assert_bookmark_rows(&restored, path!("/project/file.rs"), &[2, 4]);
}
