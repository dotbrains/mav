use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_find_project_path_abs(
    background_executor: BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    // find_project_path should work with absolute paths
    init_test(cx);

    let fs = FakeFs::new(background_executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "project1": {
                "file1.txt": "content1",
                "subdir": {
                    "file2.txt": "content2"
                }
            },
            "project2": {
                "file3.txt": "content3"
            }
        }),
    )
    .await;

    let project = Project::test(
        fs.clone(),
        [
            path!("/root/project1").as_ref(),
            path!("/root/project2").as_ref(),
        ],
        cx,
    )
    .await;

    // Make sure the worktrees are fully initialized
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    let (project1_abs_path, project1_id, project2_abs_path, project2_id) =
        project.read_with(cx, |project, cx| {
            let worktrees: Vec<_> = project.worktrees(cx).collect();
            let abs_path1 = worktrees[0].read(cx).abs_path().to_path_buf();
            let id1 = worktrees[0].read(cx).id();
            let abs_path2 = worktrees[1].read(cx).abs_path().to_path_buf();
            let id2 = worktrees[1].read(cx).id();
            (abs_path1, id1, abs_path2, id2)
        });

    project.update(cx, |project, cx| {
        let abs_path = project1_abs_path.join("file1.txt");
        let found_path = project.find_project_path(abs_path, cx).unwrap();
        assert_eq!(found_path.worktree_id, project1_id);
        assert_eq!(&*found_path.path, rel_path("file1.txt"));

        let abs_path = project1_abs_path.join("subdir").join("file2.txt");
        let found_path = project.find_project_path(abs_path, cx).unwrap();
        assert_eq!(found_path.worktree_id, project1_id);
        assert_eq!(&*found_path.path, rel_path("subdir/file2.txt"));

        let abs_path = project2_abs_path.join("file3.txt");
        let found_path = project.find_project_path(abs_path, cx).unwrap();
        assert_eq!(found_path.worktree_id, project2_id);
        assert_eq!(&*found_path.path, rel_path("file3.txt"));

        let abs_path = project1_abs_path.join("nonexistent.txt");
        let found_path = project.find_project_path(abs_path, cx);
        assert!(
            found_path.is_some(),
            "Should find project path for nonexistent file in worktree"
        );

        // Test with an absolute path outside any worktree
        let abs_path = Path::new("/some/other/path");
        let found_path = project.find_project_path(abs_path, cx);
        assert!(
            found_path.is_none(),
            "Should not find project path for path outside any worktree"
        );
    });
}

#[gpui::test]
async fn test_git_worktree_remove(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "a": {
                ".git": {},
                "src": {
                    "main.rs": "fn main() {}",
                }
            },
            "b": {
                ".git": {},
                "src": {
                    "main.rs": "fn main() {}",
                },
                "script": {
                    "run.sh": "#!/bin/bash"
                }
            }
        }),
    )
    .await;

    let project = Project::test(
        fs.clone(),
        [
            path!("/root/a").as_ref(),
            path!("/root/b/script").as_ref(),
            path!("/root/b").as_ref(),
        ],
        cx,
    )
    .await;
    let scan_complete = project.update(cx, |project, cx| project.git_scans_complete(cx));
    scan_complete.await;

    let worktrees = project.update(cx, |project, cx| project.worktrees(cx).collect::<Vec<_>>());
    assert_eq!(worktrees.len(), 3);

    let worktree_id_by_abs_path = worktrees
        .into_iter()
        .map(|worktree| worktree.read_with(cx, |w, _| (w.abs_path(), w.id())))
        .collect::<HashMap<_, _>>();
    let worktree_id = worktree_id_by_abs_path
        .get(Path::new(path!("/root/b/script")))
        .unwrap();

    let repos = project.update(cx, |p, cx| p.git_store().read(cx).repositories().clone());
    assert_eq!(repos.len(), 2);

    project.update(cx, |project, cx| {
        project.remove_worktree(*worktree_id, cx);
    });
    cx.run_until_parked();

    let mut repo_paths = project
        .update(cx, |p, cx| p.git_store().read(cx).repositories().clone())
        .values()
        .map(|repo| repo.read_with(cx, |r, _| r.work_directory_abs_path.clone()))
        .collect::<Vec<_>>();
    repo_paths.sort();

    pretty_assertions::assert_eq!(
        repo_paths,
        [
            Path::new(path!("/root/a")).into(),
            Path::new(path!("/root/b")).into(),
        ]
    );

    let active_repo_path = project
        .read_with(cx, |p, cx| {
            p.active_repository(cx)
                .map(|r| r.read(cx).work_directory_abs_path.clone())
        })
        .unwrap();
    assert_eq!(active_repo_path.as_ref(), Path::new(path!("/root/a")));

    let worktree_id = worktree_id_by_abs_path
        .get(Path::new(path!("/root/a")))
        .unwrap();
    project.update(cx, |project, cx| {
        project.remove_worktree(*worktree_id, cx);
    });
    cx.run_until_parked();

    let active_repo_path = project
        .read_with(cx, |p, cx| {
            p.active_repository(cx)
                .map(|r| r.read(cx).work_directory_abs_path.clone())
        })
        .unwrap();
    assert_eq!(active_repo_path.as_ref(), Path::new(path!("/root/b")));

    let worktree_id = worktree_id_by_abs_path
        .get(Path::new(path!("/root/b")))
        .unwrap();
    project.update(cx, |project, cx| {
        project.remove_worktree(*worktree_id, cx);
    });
    cx.run_until_parked();

    let active_repo_path = project.read_with(cx, |p, cx| {
        p.active_repository(cx)
            .map(|r| r.read(cx).work_directory_abs_path.clone())
    });
    assert!(active_repo_path.is_none());
}

#[gpui::test]
async fn test_optimistic_hunks_in_staged_files(cx: &mut gpui::TestAppContext) {
    use DiffHunkSecondaryStatus::*;
    init_test(cx);

    let committed_contents = r#"
        one
        two
        three
    "#
    .unindent();
    let file_contents = r#"
        one
        TWO
        three
    "#
    .unindent();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            ".git": {},
            "file.txt": file_contents.clone()
        }),
    )
    .await;

    fs.set_head_and_index_for_repo(
        path!("/dir/.git").as_ref(),
        &[("file.txt", committed_contents.clone())],
    );

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/file.txt"), cx)
        })
        .await
        .unwrap();
    let snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());
    let uncommitted_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();

    // The hunk is initially unstaged.
    uncommitted_diff.read_with(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[(
                1..2,
                "two\n",
                "TWO\n",
                DiffHunkStatus::modified(HasSecondaryHunk),
            )],
        );
    });

    // Get the repository handle.
    let repo = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    // Stage the file.
    let stage_task = repo.update(cx, |repo, cx| {
        repo.stage_entries(vec![repo_path("file.txt")], cx)
    });

    // Run a few ticks to let the job start and mark hunks as pending,
    // but don't run_until_parked which would complete the entire operation.
    for _ in 0..10 {
        cx.executor().tick();
        let [hunk]: [_; 1] = uncommitted_diff
            .read_with(cx, |diff, cx| {
                diff.snapshot(cx).hunks(&snapshot).collect::<Vec<_>>()
            })
            .try_into()
            .unwrap();
        match hunk.secondary_status {
            HasSecondaryHunk => {}
            SecondaryHunkRemovalPending => break,
            NoSecondaryHunk => panic!("hunk was not optimistically staged"),
            _ => panic!("unexpected hunk state"),
        }
    }
    uncommitted_diff.read_with(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[(
                1..2,
                "two\n",
                "TWO\n",
                DiffHunkStatus::modified(SecondaryHunkRemovalPending),
            )],
        );
    });

    // Let the staging complete.
    stage_task.await.unwrap();
    cx.run_until_parked();

    // The hunk is now fully staged.
    uncommitted_diff.read_with(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[(
                1..2,
                "two\n",
                "TWO\n",
                DiffHunkStatus::modified(NoSecondaryHunk),
            )],
        );
    });

    // Simulate a commit by updating HEAD to match the current file contents.
    // The FakeGitRepository's commit method is a no-op, so we need to manually
    // update HEAD to simulate the commit completing.
    fs.set_head_for_repo(
        path!("/dir/.git").as_ref(),
        &[("file.txt", file_contents.clone())],
        "newhead",
    );
    cx.run_until_parked();

    // After committing, there are no more hunks.
    uncommitted_diff.read_with(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[] as &[(Range<u32>, &str, &str, DiffHunkStatus)],
        );
    });
}
