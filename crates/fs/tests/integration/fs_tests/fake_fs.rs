use super::*;

#[gpui::test]
async fn test_fake_fs(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "a": "A",
                "b": "B"
            },
            "dir2": {
                "c": "C",
                "dir3": {
                    "d": "D"
                }
            }
        }),
    )
    .await;

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/root/dir1/a")),
            PathBuf::from(path!("/root/dir1/b")),
            PathBuf::from(path!("/root/dir2/c")),
            PathBuf::from(path!("/root/dir2/dir3/d")),
        ]
    );

    fs.create_symlink(path!("/root/dir2/link-to-dir3").as_ref(), "./dir3".into())
        .await
        .unwrap();

    assert_eq!(
        fs.canonicalize(path!("/root/dir2/link-to-dir3").as_ref())
            .await
            .unwrap(),
        PathBuf::from(path!("/root/dir2/dir3")),
    );
    assert_eq!(
        fs.canonicalize(path!("/root/dir2/link-to-dir3/d").as_ref())
            .await
            .unwrap(),
        PathBuf::from(path!("/root/dir2/dir3/d")),
    );
    assert_eq!(
        fs.load(path!("/root/dir2/link-to-dir3/d").as_ref())
            .await
            .unwrap(),
        "D",
    );
}
