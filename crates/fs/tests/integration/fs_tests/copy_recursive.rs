use super::*;

#[gpui::test]
async fn test_copy_recursive_with_single_file(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/outer"),
        json!({
            "a": "A",
            "b": "B",
            "inner": {}
        }),
    )
    .await;

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/a")),
            PathBuf::from(path!("/outer/b")),
        ]
    );

    let source = Path::new(path!("/outer/a"));
    let target = Path::new(path!("/outer/a copy"));
    copy_recursive(fs.as_ref(), source, target, Default::default())
        .await
        .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/a")),
            PathBuf::from(path!("/outer/a copy")),
            PathBuf::from(path!("/outer/b")),
        ]
    );

    let source = Path::new(path!("/outer/a"));
    let target = Path::new(path!("/outer/inner/a copy"));
    copy_recursive(fs.as_ref(), source, target, Default::default())
        .await
        .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/a")),
            PathBuf::from(path!("/outer/a copy")),
            PathBuf::from(path!("/outer/b")),
            PathBuf::from(path!("/outer/inner/a copy")),
        ]
    );
}

#[gpui::test]
async fn test_copy_recursive_with_single_dir(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/outer"),
        json!({
            "a": "A",
            "empty": {},
            "non-empty": {
                "b": "B",
            }
        }),
    )
    .await;

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/a")),
            PathBuf::from(path!("/outer/non-empty/b")),
        ]
    );
    assert_eq!(
        fs.directories(false),
        vec![
            PathBuf::from(path!("/")),
            PathBuf::from(path!("/outer")),
            PathBuf::from(path!("/outer/empty")),
            PathBuf::from(path!("/outer/non-empty")),
        ]
    );

    let source = Path::new(path!("/outer/empty"));
    let target = Path::new(path!("/outer/empty copy"));
    copy_recursive(fs.as_ref(), source, target, Default::default())
        .await
        .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/a")),
            PathBuf::from(path!("/outer/non-empty/b")),
        ]
    );
    assert_eq!(
        fs.directories(false),
        vec![
            PathBuf::from(path!("/")),
            PathBuf::from(path!("/outer")),
            PathBuf::from(path!("/outer/empty")),
            PathBuf::from(path!("/outer/empty copy")),
            PathBuf::from(path!("/outer/non-empty")),
        ]
    );

    let source = Path::new(path!("/outer/non-empty"));
    let target = Path::new(path!("/outer/non-empty copy"));
    copy_recursive(fs.as_ref(), source, target, Default::default())
        .await
        .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/a")),
            PathBuf::from(path!("/outer/non-empty/b")),
            PathBuf::from(path!("/outer/non-empty copy/b")),
        ]
    );
    assert_eq!(
        fs.directories(false),
        vec![
            PathBuf::from(path!("/")),
            PathBuf::from(path!("/outer")),
            PathBuf::from(path!("/outer/empty")),
            PathBuf::from(path!("/outer/empty copy")),
            PathBuf::from(path!("/outer/non-empty")),
            PathBuf::from(path!("/outer/non-empty copy")),
        ]
    );
}

#[gpui::test]
async fn test_copy_recursive(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/outer"),
        json!({
            "inner1": {
                "a": "A",
                "b": "B",
                "inner3": {
                    "d": "D",
                },
                "inner4": {}
            },
            "inner2": {
                "c": "C",
            }
        }),
    )
    .await;

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/inner3/d")),
        ]
    );
    assert_eq!(
        fs.directories(false),
        vec![
            PathBuf::from(path!("/")),
            PathBuf::from(path!("/outer")),
            PathBuf::from(path!("/outer/inner1")),
            PathBuf::from(path!("/outer/inner2")),
            PathBuf::from(path!("/outer/inner1/inner3")),
            PathBuf::from(path!("/outer/inner1/inner4")),
        ]
    );

    let source = Path::new(path!("/outer"));
    let target = Path::new(path!("/outer/inner1/outer"));
    copy_recursive(fs.as_ref(), source, target, Default::default())
        .await
        .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/inner3/d")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner1/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/inner3/d")),
        ]
    );
    assert_eq!(
        fs.directories(false),
        vec![
            PathBuf::from(path!("/")),
            PathBuf::from(path!("/outer")),
            PathBuf::from(path!("/outer/inner1")),
            PathBuf::from(path!("/outer/inner2")),
            PathBuf::from(path!("/outer/inner1/inner3")),
            PathBuf::from(path!("/outer/inner1/inner4")),
            PathBuf::from(path!("/outer/inner1/outer")),
            PathBuf::from(path!("/outer/inner1/outer/inner1")),
            PathBuf::from(path!("/outer/inner1/outer/inner2")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/inner3")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/inner4")),
        ]
    );
}

#[gpui::test]
async fn test_copy_recursive_with_overwriting(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/outer"),
        json!({
            "inner1": {
                "a": "A",
                "b": "B",
                "outer": {
                    "inner1": {
                        "a": "B"
                    }
                }
            },
            "inner2": {
                "c": "C",
            }
        }),
    )
    .await;

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/a")),
        ]
    );
    assert_eq!(
        fs.load(path!("/outer/inner1/outer/inner1/a").as_ref())
            .await
            .unwrap(),
        "B",
    );

    let source = Path::new(path!("/outer"));
    let target = Path::new(path!("/outer/inner1/outer"));
    copy_recursive(
        fs.as_ref(),
        source,
        target,
        CopyOptions {
            overwrite: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner1/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/outer/inner1/a")),
        ]
    );
    assert_eq!(
        fs.load(path!("/outer/inner1/outer/inner1/a").as_ref())
            .await
            .unwrap(),
        "A"
    );
}

#[gpui::test]
async fn test_copy_recursive_with_ignoring(executor: BackgroundExecutor) {
    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(
        path!("/outer"),
        json!({
            "inner1": {
                "a": "A",
                "b": "B",
                "outer": {
                    "inner1": {
                        "a": "B"
                    }
                }
            },
            "inner2": {
                "c": "C",
            }
        }),
    )
    .await;

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/a")),
        ]
    );
    assert_eq!(
        fs.load(path!("/outer/inner1/outer/inner1/a").as_ref())
            .await
            .unwrap(),
        "B",
    );

    let source = Path::new(path!("/outer"));
    let target = Path::new(path!("/outer/inner1/outer"));
    copy_recursive(
        fs.as_ref(),
        source,
        target,
        CopyOptions {
            ignore_if_exists: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(
        fs.files(),
        vec![
            PathBuf::from(path!("/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/a")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/b")),
            PathBuf::from(path!("/outer/inner1/outer/inner2/c")),
            PathBuf::from(path!("/outer/inner1/outer/inner1/outer/inner1/a")),
        ]
    );
    assert_eq!(
        fs.load(path!("/outer/inner1/outer/inner1/a").as_ref())
            .await
            .unwrap(),
        "B"
    );
}
