use crate::bookmark_store::*;

#[gpui::test]
async fn test_file_deletion_removes_bookmarks(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            "file1.rs": "aaa\nbbb\nccc\n",
            "file2.rs": "ddd\neee\nfff\n"
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let buffer1 = open_buffer(&project, path!("/project/file1.rs"), cx).await;
    let buffer2 = open_buffer(&project, path!("/project/file2.rs"), cx).await;

    add_bookmarks(&project, &buffer1, &[0, 2], cx);
    add_bookmarks(&project, &buffer2, &[1], cx);
    assert_eq!(get_all_bookmarks(&project, cx).len(), 2);

    // Delete file1.rs
    fs.remove_file(path!("/project/file1.rs").as_ref(), Default::default())
        .await
        .unwrap();
    cx.executor().run_until_parked();

    // file1.rs bookmarks should be gone, file2.rs bookmarks preserved
    let bookmarks = get_all_bookmarks(&project, cx);
    assert_eq!(bookmarks.len(), 1);
    assert!(!bookmarks.contains_key(&project_path(path!("/project/file1.rs"))));
    assert_bookmark_rows(&bookmarks, path!("/project/file2.rs"), &[1]);
}

#[gpui::test]
async fn test_deleting_all_bookmarked_files_clears_store(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            "file1.rs": "aaa\nbbb\n",
            "file2.rs": "ccc\nddd\n"
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let buffer1 = open_buffer(&project, path!("/project/file1.rs"), cx).await;
    let buffer2 = open_buffer(&project, path!("/project/file2.rs"), cx).await;

    add_bookmarks(&project, &buffer1, &[0], cx);
    add_bookmarks(&project, &buffer2, &[1], cx);
    assert_eq!(get_all_bookmarks(&project, cx).len(), 2);

    // Delete both files
    fs.remove_file(path!("/project/file1.rs").as_ref(), Default::default())
        .await
        .unwrap();
    fs.remove_file(path!("/project/file2.rs").as_ref(), Default::default())
        .await
        .unwrap();
    cx.executor().run_until_parked();

    assert!(get_all_bookmarks(&project, cx).is_empty());
}

#[gpui::test]
async fn test_file_rename_re_keys_bookmarks(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(path!("/project"), json!({"old_name.rs": "aaa\nbbb\nccc\n"}))
        .await;

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let buffer = open_buffer(&project, path!("/project/old_name.rs"), cx).await;

    add_bookmarks(&project, &buffer, &[0, 2], cx);
    assert_bookmark_rows(
        &get_all_bookmarks(&project, cx),
        path!("/project/old_name.rs"),
        &[0, 2],
    );

    // Rename the file
    fs.rename(
        path!("/project/old_name.rs").as_ref(),
        path!("/project/new_name.rs").as_ref(),
        Default::default(),
    )
    .await
    .unwrap();
    cx.executor().run_until_parked();

    let bookmarks = get_all_bookmarks(&project, cx);
    assert_eq!(bookmarks.len(), 1);
    assert!(!bookmarks.contains_key(&project_path(path!("/project/old_name.rs"))));
    assert_bookmark_rows(&bookmarks, path!("/project/new_name.rs"), &[0, 2]);
}

#[gpui::test]
async fn test_file_rename_preserves_other_bookmarks(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            "rename_me.rs": "aaa\nbbb\n",
            "untouched.rs": "ccc\nddd\neee\n"
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let buffer_rename = open_buffer(&project, path!("/project/rename_me.rs"), cx).await;
    let buffer_other = open_buffer(&project, path!("/project/untouched.rs"), cx).await;

    add_bookmarks(&project, &buffer_rename, &[1], cx);
    add_bookmarks(&project, &buffer_other, &[0, 2], cx);

    fs.rename(
        path!("/project/rename_me.rs").as_ref(),
        path!("/project/renamed.rs").as_ref(),
        Default::default(),
    )
    .await
    .unwrap();
    cx.executor().run_until_parked();

    let bookmarks = get_all_bookmarks(&project, cx);
    assert_eq!(bookmarks.len(), 2);
    assert_bookmark_rows(&bookmarks, path!("/project/renamed.rs"), &[1]);
    assert_bookmark_rows(&bookmarks, path!("/project/untouched.rs"), &[0, 2]);
}
