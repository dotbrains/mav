use crate::bookmark_store::*;

#[gpui::test]
async fn test_all_serialized_bookmarks_empty(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(path!("/project"), json!({"file1.rs": "line1\nline2\n"}))
        .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    assert!(get_all_bookmarks(&project, cx).is_empty());
}

#[gpui::test]
async fn test_all_serialized_bookmarks_single_file(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({"file1.rs": "line1\nline2\nline3\nline4\nline5\n"}),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let buffer = open_buffer(&project, path!("/project/file1.rs"), cx).await;

    add_bookmarks(&project, &buffer, &[0, 2], cx);

    let bookmarks = get_all_bookmarks(&project, cx);
    assert_eq!(bookmarks.len(), 1);
    assert_bookmark_rows(&bookmarks, path!("/project/file1.rs"), &[0, 2]);
}

#[gpui::test]
async fn test_all_serialized_bookmarks_includes_labels(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({"file1.rs": "line1\nline2\nline3\n"}),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let buffer = open_buffer(&project, path!("/project/file1.rs"), cx).await;

    add_labeled_bookmark(&project, &buffer, 0, "first", cx);
    add_labeled_bookmark(&project, &buffer, 2, "  keeps inner spaces  ", cx);

    let bookmarks = get_all_bookmarks(&project, cx);
    assert_bookmark_labels(
        &bookmarks,
        path!("/project/file1.rs"),
        &[(0, "first"), (2, "  keeps inner spaces  ")],
    );
}

#[gpui::test]
async fn test_all_serialized_bookmarks_multiple_files(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            "file1.rs": "line1\nline2\nline3\n",
            "file2.rs": "lineA\nlineB\nlineC\nlineD\n",
            "file3.rs": "single line"
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let buffer1 = open_buffer(&project, path!("/project/file1.rs"), cx).await;
    let buffer2 = open_buffer(&project, path!("/project/file2.rs"), cx).await;
    let _buffer3 = open_buffer(&project, path!("/project/file3.rs"), cx).await;

    add_bookmarks(&project, &buffer1, &[1], cx);
    add_bookmarks(&project, &buffer2, &[0, 3], cx);

    let bookmarks = get_all_bookmarks(&project, cx);
    assert_eq!(bookmarks.len(), 2);
    assert_bookmark_rows(&bookmarks, path!("/project/file1.rs"), &[1]);
    assert_bookmark_rows(&bookmarks, path!("/project/file2.rs"), &[0, 3]);
    assert!(
        !bookmarks.contains_key(&project_path(path!("/project/file3.rs"))),
        "file3.rs should have no bookmarks"
    );
}

#[gpui::test]
async fn test_all_serialized_bookmarks_after_toggle_off(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({"file1.rs": "line1\nline2\nline3\n"}),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let buffer = open_buffer(&project, path!("/project/file1.rs"), cx).await;

    add_bookmarks(&project, &buffer, &[1], cx);
    assert_eq!(get_all_bookmarks(&project, cx).len(), 1);

    // Toggle same row again to remove it
    add_bookmarks(&project, &buffer, &[1], cx);
    assert!(get_all_bookmarks(&project, cx).is_empty());
}

#[gpui::test]
async fn test_all_serialized_bookmarks_with_clear(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            "file1.rs": "line1\nline2\nline3\n",
            "file2.rs": "lineA\nlineB\n"
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let buffer1 = open_buffer(&project, path!("/project/file1.rs"), cx).await;
    let buffer2 = open_buffer(&project, path!("/project/file2.rs"), cx).await;

    add_bookmarks(&project, &buffer1, &[0], cx);
    add_bookmarks(&project, &buffer2, &[1], cx);
    assert_eq!(get_all_bookmarks(&project, cx).len(), 2);

    clear_bookmarks(&project, cx);
    assert!(get_all_bookmarks(&project, cx).is_empty());
}

#[gpui::test]
async fn test_all_serialized_bookmarks_returns_sorted_by_path(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({"b.rs": "line1\n", "a.rs": "line1\n", "c.rs": "line1\n"}),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let buffer_b = open_buffer(&project, path!("/project/b.rs"), cx).await;
    let buffer_a = open_buffer(&project, path!("/project/a.rs"), cx).await;
    let buffer_c = open_buffer(&project, path!("/project/c.rs"), cx).await;

    add_bookmarks(&project, &buffer_b, &[0], cx);
    add_bookmarks(&project, &buffer_a, &[0], cx);
    add_bookmarks(&project, &buffer_c, &[0], cx);

    let paths: Vec<_> = get_all_bookmarks(&project, cx).keys().cloned().collect();
    assert_eq!(
        paths,
        [
            project_path(path!("/project/a.rs")),
            project_path(path!("/project/b.rs")),
            project_path(path!("/project/c.rs")),
        ]
    );
}

#[gpui::test]
async fn test_all_serialized_bookmarks_deduplicates_same_row(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({"file1.rs": "line1\nline2\nline3\nline4\n"}),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let buffer = open_buffer(&project, path!("/project/file1.rs"), cx).await;

    add_bookmarks(&project, &buffer, &[1, 2], cx);

    let bookmarks = get_all_bookmarks(&project, cx);
    assert_bookmark_rows(&bookmarks, path!("/project/file1.rs"), &[1, 2]);

    // Verify no duplicates
    let rows: Vec<u32> = bookmarks
        .get(&project_path(path!("/project/file1.rs")))
        .unwrap()
        .iter()
        .map(|b| b.row)
        .collect();
    let mut deduped = rows.clone();
    deduped.dedup();
    assert_eq!(rows, deduped);
}
