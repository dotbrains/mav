use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_git_worktrees_and_submodules(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {
                "worktrees": {
                    "some-worktree": {
                        "commondir": "../..\n",
                        // For is_git_dir
                        "HEAD": "",
                        "config": ""
                    }
                },
                "modules": {
                    "subdir": {
                        "some-submodule": {
                            // For is_git_dir
                            "HEAD": "",
                            "config": "",
                        }
                    }
                }
            },
            "src": {
                "a.txt": "A",
            },
            "some-worktree": {
                ".git": "gitdir: ../.git/worktrees/some-worktree\n",
                "src": {
                    "b.txt": "B",
                }
            },
            "subdir": {
                "some-submodule": {
                    ".git": "gitdir: ../../.git/modules/subdir/some-submodule\n",
                    "c.txt": "C",
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let scan_complete = project.update(cx, |project, cx| project.git_scans_complete(cx));
    scan_complete.await;

    let mut repositories = project.update(cx, |project, cx| {
        project
            .repositories(cx)
            .values()
            .map(|repo| repo.read(cx).work_directory_abs_path.clone())
            .collect::<Vec<_>>()
    });
    repositories.sort();
    pretty_assertions::assert_eq!(
        repositories,
        [
            Path::new(path!("/project")).into(),
            Path::new(path!("/project/some-worktree")).into(),
            Path::new(path!("/project/subdir/some-submodule")).into(),
        ]
    );

    // Generate a git-related event for the worktree and check that it's refreshed.
    fs.with_git_state(
        path!("/project/some-worktree/.git").as_ref(),
        true,
        |state| {
            state
                .head_contents
                .insert(repo_path("src/b.txt"), "b".to_owned());
            state
                .index_contents
                .insert(repo_path("src/b.txt"), "b".to_owned());
        },
    )
    .unwrap();
    cx.run_until_parked();

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/project/some-worktree/src/b.txt"), cx)
        })
        .await
        .unwrap();
    let (worktree_repo, barrier) = project.update(cx, |project, cx| {
        let (repo, _) = project
            .git_store()
            .read(cx)
            .repository_and_path_for_buffer_id(buffer.read(cx).remote_id(), cx)
            .unwrap();
        pretty_assertions::assert_eq!(
            repo.read(cx).work_directory_abs_path,
            Path::new(path!("/project/some-worktree")).into(),
        );
        pretty_assertions::assert_eq!(
            repo.read(cx).main_worktree_abs_path(),
            Some(Path::new(path!("/project"))),
        );
        assert!(
            repo.read(cx).linked_worktree_path().is_some(),
            "linked worktree should be detected as a linked worktree"
        );
        let barrier = repo.update(cx, |repo, _| repo.barrier());
        (repo.clone(), barrier)
    });
    barrier.await.unwrap();
    worktree_repo.update(cx, |repo, _| {
        pretty_assertions::assert_eq!(
            repo.status_for_path(&repo_path("src/b.txt"))
                .unwrap()
                .status,
            StatusCode::Modified.worktree(),
        );
    });

    // The same for the submodule.
    fs.with_git_state(
        path!("/project/subdir/some-submodule/.git").as_ref(),
        true,
        |state| {
            state
                .head_contents
                .insert(repo_path("c.txt"), "c".to_owned());
            state
                .index_contents
                .insert(repo_path("c.txt"), "c".to_owned());
        },
    )
    .unwrap();
    cx.run_until_parked();

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/project/subdir/some-submodule/c.txt"), cx)
        })
        .await
        .unwrap();
    let (submodule_repo, barrier) = project.update(cx, |project, cx| {
        let (repo, _) = project
            .git_store()
            .read(cx)
            .repository_and_path_for_buffer_id(buffer.read(cx).remote_id(), cx)
            .unwrap();
        pretty_assertions::assert_eq!(
            repo.read(cx).work_directory_abs_path,
            Path::new(path!("/project/subdir/some-submodule")).into(),
        );
        pretty_assertions::assert_eq!(
            repo.read(cx).main_worktree_abs_path(),
            Some(Path::new(path!("/project/subdir/some-submodule"))),
        );
        assert!(
            repo.read(cx).linked_worktree_path().is_none(),
            "submodule should not be detected as a linked worktree"
        );
        let barrier = repo.update(cx, |repo, _| repo.barrier());
        (repo.clone(), barrier)
    });
    barrier.await.unwrap();
    submodule_repo.update(cx, |repo, _| {
        pretty_assertions::assert_eq!(
            repo.status_for_path(&repo_path("c.txt")).unwrap().status,
            StatusCode::Modified.worktree(),
        );
    });
}

#[gpui::test]
async fn test_repository_deduplication(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                ".git": {},
                "child1": {
                    "a.txt": "A",
                },
                "child2": {
                    "b.txt": "B",
                }
            }
        }),
    )
    .await;

    let project = Project::test(
        fs.clone(),
        [
            path!("/root/project/child1").as_ref(),
            path!("/root/project/child2").as_ref(),
        ],
        cx,
    )
    .await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repos = project.read_with(cx, |project, cx| {
        project
            .repositories(cx)
            .values()
            .map(|repo| repo.read(cx).work_directory_abs_path.clone())
            .collect::<Vec<_>>()
    });
    pretty_assertions::assert_eq!(repos, [Path::new(path!("/root/project")).into()]);
}

#[gpui::test]
async fn test_buffer_changed_file_path_updates_git_diff(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let file_1_committed = String::from(r#"file_1_committed"#);
    let file_1_staged = String::from(r#"file_1_staged"#);
    let file_2_committed = String::from(r#"file_2_committed"#);
    let file_2_staged = String::from(r#"file_2_staged"#);
    let buffer_contents = String::from(r#"buffer"#);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            ".git": {},
           "src": {
               "file_1.rs": file_1_committed.clone(),
               "file_2.rs": file_2_committed.clone(),
           }
        }),
    )
    .await;

    fs.set_head_for_repo(
        path!("/dir/.git").as_ref(),
        &[
            ("src/file_1.rs", file_1_committed.clone()),
            ("src/file_2.rs", file_2_committed.clone()),
        ],
        "deadbeef",
    );
    fs.set_index_for_repo(
        path!("/dir/.git").as_ref(),
        &[
            ("src/file_1.rs", file_1_staged.clone()),
            ("src/file_2.rs", file_2_staged.clone()),
        ],
    );

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/src/file_1.rs"), cx)
        })
        .await
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..buffer.len(), buffer_contents.as_str())], None, cx);
    });

    let unstaged_diff = project
        .update(cx, |project, cx| {
            project.open_unstaged_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    unstaged_diff.update(cx, |unstaged_diff, cx| {
        let base_text = unstaged_diff.base_text_string(cx).unwrap();
        assert_eq!(base_text, file_1_staged, "Should start with file_1 staged");
    });

    // Save the buffer as `file_2.rs`, which should trigger the
    // `BufferChangedFilePath` event.
    project
        .update(cx, |project, cx| {
            let worktree_id = project.worktrees(cx).next().unwrap().read(cx).id();
            let path = ProjectPath {
                worktree_id,
                path: rel_path("src/file_2.rs").into(),
            };
            project.save_buffer_as(buffer.clone(), path, cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    // Verify that the diff bases have been updated to file_2's contents due to
    // the `BufferChangedFilePath` event being handled.
    unstaged_diff.update(cx, |unstaged_diff, cx| {
        let snapshot = buffer.read(cx).snapshot();
        let base_text = unstaged_diff.base_text_string(cx).unwrap();
        assert_eq!(
            base_text, file_2_staged,
            "Diff bases should be automatically updated to file_2 staged content"
        );

        let hunks: Vec<_> = unstaged_diff.snapshot(cx).hunks(&snapshot).collect();
        assert!(!hunks.is_empty(), "Should have diff hunks for file_2");
    });

    let uncommitted_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    uncommitted_diff.update(cx, |uncommitted_diff, cx| {
        let base_text = uncommitted_diff.base_text_string(cx).unwrap();
        assert_eq!(
            base_text, file_2_committed,
            "Uncommitted diff should compare against file_2 committed content"
        );
    });
}
