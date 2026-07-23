use super::*;

#[gpui::test]
async fn test_fake_fs_trash(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "file_c.txt": "File C",
                "file_d.txt": "File D"
            },
            "file_a.txt": "File A",
            "file_b.txt": "File B",
        }),
    )
    .await;

    // Trashing a file.
    let root_path = PathBuf::from(path!("/root"));
    let path = path!("/root/file_a.txt").as_ref();
    let trashed_entry = fs
        .trash(path, Default::default())
        .await
        .expect("should be able to trash {path:?}");

    assert_eq!(trashed_entry.name, "file_a.txt");
    assert_eq!(trashed_entry.original_parent, root_path);
    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/file_b.txt")),
            PathBuf::from(path!("/root/src/file_c.txt")),
            PathBuf::from(path!("/root/src/file_d.txt"))
        ]
    );

    let trash_entries = fs.trash_entries();
    assert_eq!(trash_entries.len(), 1);
    assert_eq!(trash_entries[0].name, "file_a.txt");
    assert_eq!(trash_entries[0].original_parent, root_path);

    // Trashing a directory.
    let path = path!("/root/src").as_ref();
    let trashed_entry = fs
        .trash(
            path,
            RemoveOptions {
                recursive: true,
                ..Default::default()
            },
        )
        .await
        .expect("should be able to trash {path:?}");

    assert_eq!(trashed_entry.name, "src");
    assert_eq!(trashed_entry.original_parent, root_path);
    assert_eq!(fs.files(), vec![PathBuf::from(path!("/root/file_b.txt"))]);

    let trash_entries = fs.trash_entries();
    assert_eq!(trash_entries.len(), 2);
    assert_eq!(trash_entries[1].name, "src");
    assert_eq!(trash_entries[1].original_parent, root_path);
}

#[gpui::test]
async fn test_fake_fs_restore(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "file_a.txt": "File A",
                "file_b.txt": "File B",
            },
            "file_c.txt": "File C",
        }),
    )
    .await;

    // Providing a non-existent `TrashedEntry` should result in an error.
    let id = OsString::from("/trash/file_c.txt");
    let name = OsString::from("file_c.txt");
    let original_parent = PathBuf::from(path!("/root"));
    let trashed_entry = TrashedEntry {
        id,
        name,
        original_parent,
    };
    let result = fs.restore(trashed_entry).await;
    assert!(matches!(result, Err(TrashRestoreError::NotFound { .. })));

    // Attempt deleting a file, asserting that the filesystem no longer reports
    // it as part of its list of files, restore it and verify that the list of
    // files and trash has been updated accordingly.
    let path = path!("/root/src/file_a.txt").as_ref();
    let trashed_entry = fs.trash(path, Default::default()).await.unwrap();

    assert_eq!(fs.trash_entries().len(), 1);
    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/file_c.txt")),
            PathBuf::from(path!("/root/src/file_b.txt"))
        ]
    );

    fs.restore(trashed_entry).await.unwrap();

    assert_eq!(fs.trash_entries().len(), 0);
    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/file_c.txt")),
            PathBuf::from(path!("/root/src/file_a.txt")),
            PathBuf::from(path!("/root/src/file_b.txt"))
        ]
    );

    // Deleting and restoring a directory should also remove all of its files
    // but create a single trashed entry, which should be removed after
    // restoration.
    let options = RemoveOptions {
        recursive: true,
        ..Default::default()
    };
    let path = path!("/root/src/").as_ref();
    let trashed_entry = fs.trash(path, options).await.unwrap();

    assert_eq!(fs.trash_entries().len(), 1);
    assert_eq!(fs.files(), vec![PathBuf::from(path!("/root/file_c.txt"))]);

    fs.restore(trashed_entry).await.unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/file_c.txt")),
            PathBuf::from(path!("/root/src/file_a.txt")),
            PathBuf::from(path!("/root/src/file_b.txt"))
        ]
    );
    assert_eq!(fs.trash_entries().len(), 0);

    // A collision error should be returned in case a file is being restored to
    // a path where a file already exists.
    let path = path!("/root/src/file_a.txt").as_ref();
    let trashed_entry = fs.trash(path, Default::default()).await.unwrap();

    assert_eq!(fs.trash_entries().len(), 1);
    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/file_c.txt")),
            PathBuf::from(path!("/root/src/file_b.txt"))
        ]
    );

    fs.write(path, "New File A".as_bytes()).await.unwrap();

    assert_eq!(fs.trash_entries().len(), 1);
    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/file_c.txt")),
            PathBuf::from(path!("/root/src/file_a.txt")),
            PathBuf::from(path!("/root/src/file_b.txt"))
        ]
    );

    let file_contents = fs.files_with_contents(path);
    assert!(fs.restore(trashed_entry).await.is_err());
    assert_eq!(
        file_contents,
        vec![(PathBuf::from(path), b"New File A".to_vec())]
    );

    // A collision error should be returned in case a directory is being
    // restored to a path where a directory already exists.
    let options = RemoveOptions {
        recursive: true,
        ..Default::default()
    };
    let path = path!("/root/src/").as_ref();
    let trashed_entry = fs.trash(path, options).await.unwrap();

    assert_eq!(fs.trash_entries().len(), 2);
    assert_eq!(fs.files(), vec![PathBuf::from(path!("/root/file_c.txt"))]);

    fs.create_dir(path).await.unwrap();

    assert_eq!(fs.files(), vec![PathBuf::from(path!("/root/file_c.txt"))]);
    assert_eq!(fs.trash_entries().len(), 2);

    let result = fs.restore(trashed_entry).await;
    assert!(result.is_err());

    assert_eq!(fs.files(), vec![PathBuf::from(path!("/root/file_c.txt"))]);
    assert_eq!(fs.trash_entries().len(), 2);
}
