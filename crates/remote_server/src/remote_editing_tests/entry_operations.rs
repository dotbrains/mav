use super::*;

#[gpui::test]
async fn test_remote_root_rename(cx: &mut TestAppContext, server_cx: &mut TestAppContext) {
    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        "/code",
        json!({
            "project1": {
                ".git": {},
                "README.md": "# project 1",
            },
        }),
    )
    .await;

    let (project, _) = init_test(&fs, cx, server_cx).await;

    let (worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/code/project1", true, cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    fs.rename(
        &PathBuf::from("/code/project1"),
        &PathBuf::from("/code/project2"),
        Default::default(),
    )
    .await
    .unwrap();

    cx.run_until_parked();
    worktree.update(cx, |worktree, _| {
        assert_eq!(worktree.root_name(), "project2")
    })
}

#[gpui::test]
async fn test_remote_rename_entry(cx: &mut TestAppContext, server_cx: &mut TestAppContext) {
    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        "/code",
        json!({
            "project1": {
                ".git": {},
                "README.md": "# project 1",
            },
        }),
    )
    .await;

    let (project, _) = init_test(&fs, cx, server_cx).await;
    let (worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/code/project1", true, cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    let entry = project
        .update(cx, |project, cx| {
            let worktree = worktree.read(cx);
            let entry = worktree.entry_for_path(rel_path("README.md")).unwrap();
            project.rename_entry(entry.id, (worktree.id(), rel_path("README.rst")).into(), cx)
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();

    cx.run_until_parked();

    worktree.update(cx, |worktree, _| {
        assert_eq!(
            worktree.entry_for_path(rel_path("README.rst")).unwrap().id,
            entry.id
        )
    });
}

#[gpui::test]
async fn test_copy_file_into_remote_project(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    let remote_fs = FakeFs::new(server_cx.executor());
    remote_fs
        .insert_tree(
            path!("/code"),
            json!({
                "project1": {
                    ".git": {},
                    "README.md": "# project 1",
                    "src": {
                        "main.rs": ""
                    }
                },
            }),
        )
        .await;

    let (project, _) = init_test(&remote_fs, cx, server_cx).await;
    let (worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/code/project1"), true, cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    let local_fs = project
        .read_with(cx, |project, _| project.fs().clone())
        .as_fake();
    local_fs
        .insert_tree(
            path!("/local-code"),
            json!({
                "dir1": {
                    "file1": "file 1 content",
                    "dir2": {
                        "file2": "file 2 content",
                        "dir3": {
                            "file3": ""
                        },
                        "dir4": {}
                    },
                    "dir5": {}
                },
                "file4": "file 4 content"
            }),
        )
        .await;

    worktree
        .update(cx, |worktree, cx| {
            worktree.copy_external_entries(
                rel_path("src").into(),
                vec![
                    Path::new(path!("/local-code/dir1/file1")).into(),
                    Path::new(path!("/local-code/dir1/dir2")).into(),
                ],
                local_fs.clone(),
                cx,
            )
        })
        .await
        .unwrap();

    assert_eq!(
        remote_fs.paths(true),
        vec![
            PathBuf::from(path!("/")),
            PathBuf::from(path!("/code")),
            PathBuf::from(path!("/code/project1")),
            PathBuf::from(path!("/code/project1/.git")),
            PathBuf::from(path!("/code/project1/README.md")),
            PathBuf::from(path!("/code/project1/src")),
            PathBuf::from(path!("/code/project1/src/dir2")),
            PathBuf::from(path!("/code/project1/src/file1")),
            PathBuf::from(path!("/code/project1/src/main.rs")),
            PathBuf::from(path!("/code/project1/src/dir2/dir3")),
            PathBuf::from(path!("/code/project1/src/dir2/dir4")),
            PathBuf::from(path!("/code/project1/src/dir2/file2")),
            PathBuf::from(path!("/code/project1/src/dir2/dir3/file3")),
        ]
    );
    assert_eq!(
        remote_fs
            .load(path!("/code/project1/src/file1").as_ref())
            .await
            .unwrap(),
        "file 1 content"
    );
    assert_eq!(
        remote_fs
            .load(path!("/code/project1/src/dir2/file2").as_ref())
            .await
            .unwrap(),
        "file 2 content"
    );
    assert_eq!(
        remote_fs
            .load(path!("/code/project1/src/dir2/dir3/file3").as_ref())
            .await
            .unwrap(),
        ""
    );
}
