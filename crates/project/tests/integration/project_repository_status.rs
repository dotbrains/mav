use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_git_repository_status(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let root = TempTree::new(json!({
        "project": {
            "a.txt": "a",    // Modified
            "b.txt": "bb",   // Added
            "c.txt": "ccc",  // Unchanged
            "d.txt": "dddd", // Deleted
        },
    }));

    // Set up git repository before creating the project.
    let work_dir = root.path().join("project");
    let repo = git_init(work_dir.as_path());
    git_add("a.txt", &repo);
    git_add("c.txt", &repo);
    git_add("d.txt", &repo);
    git_commit("Initial commit", &repo);
    std::fs::remove_file(work_dir.join("d.txt")).unwrap();
    std::fs::write(work_dir.join("a.txt"), "aa").unwrap();

    let project = Project::test(
        Arc::new(RealFs::new(None, cx.executor())),
        [root.path()],
        cx,
    )
    .await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    // Check that the right git state is observed on startup
    repository.read_with(cx, |repository, _| {
        let entries = repository.cached_status().collect::<Vec<_>>();
        assert_eq!(
            entries,
            [
                StatusEntry {
                    repo_path: repo_path("a.txt"),
                    status: StatusCode::Modified.worktree(),
                    diff_stat: Some(DiffStat {
                        added: 1,
                        deleted: 1,
                    }),
                },
                StatusEntry {
                    repo_path: repo_path("b.txt"),
                    status: FileStatus::Untracked,
                    diff_stat: None,
                },
                StatusEntry {
                    repo_path: repo_path("d.txt"),
                    status: StatusCode::Deleted.worktree(),
                    diff_stat: Some(DiffStat {
                        added: 0,
                        deleted: 1,
                    }),
                },
            ]
        );
    });

    std::fs::write(work_dir.join("c.txt"), "some changes").unwrap();

    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    repository.read_with(cx, |repository, _| {
        let entries = repository.cached_status().collect::<Vec<_>>();
        assert_eq!(
            entries,
            [
                StatusEntry {
                    repo_path: repo_path("a.txt"),
                    status: StatusCode::Modified.worktree(),
                    diff_stat: Some(DiffStat {
                        added: 1,
                        deleted: 1,
                    }),
                },
                StatusEntry {
                    repo_path: repo_path("b.txt"),
                    status: FileStatus::Untracked,
                    diff_stat: None,
                },
                StatusEntry {
                    repo_path: repo_path("c.txt"),
                    status: StatusCode::Modified.worktree(),
                    diff_stat: Some(DiffStat {
                        added: 1,
                        deleted: 1,
                    }),
                },
                StatusEntry {
                    repo_path: repo_path("d.txt"),
                    status: StatusCode::Deleted.worktree(),
                    diff_stat: Some(DiffStat {
                        added: 0,
                        deleted: 1,
                    }),
                },
            ]
        );
    });

    git_add("a.txt", &repo);
    git_add("c.txt", &repo);
    git_remove_index(Path::new("d.txt"), &repo);
    git_commit("Another commit", &repo);
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    std::fs::remove_file(work_dir.join("a.txt")).unwrap();
    std::fs::remove_file(work_dir.join("b.txt")).unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    repository.read_with(cx, |repository, _cx| {
        let entries = repository.cached_status().collect::<Vec<_>>();

        // Deleting an untracked entry, b.txt, should leave no status
        // a.txt was tracked, and so should have a status
        assert_eq!(
            entries,
            [StatusEntry {
                repo_path: repo_path("a.txt"),
                status: StatusCode::Deleted.worktree(),
                diff_stat: Some(DiffStat {
                    added: 0,
                    deleted: 1,
                }),
            }]
        );
    });
}

#[gpui::test]
async fn test_git_repository_status_removes_directory_descendants(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                ".git": {},
                "ci2": {
                    "Dockerfile.namespace": "untracked",
                },
            },
        }),
    )
    .await;
    fs.set_status_for_repo(
        path!("/root/project/.git").as_ref(),
        &[("ci2/Dockerfile.namespace", FileStatus::Untracked)],
    );

    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    repository.read_with(cx, |repository, _| {
        assert_eq!(
            repository.cached_status().collect::<Vec<_>>(),
            [StatusEntry {
                repo_path: repo_path("ci2/Dockerfile.namespace"),
                status: FileStatus::Untracked,
                diff_stat: None,
            }]
        );
    });

    fs.pause_events();
    fs.create_dir(path!("/root/project/ci3").as_ref())
        .await
        .unwrap();
    fs.copy_file(
        path!("/root/project/ci2/Dockerfile.namespace").as_ref(),
        path!("/root/project/ci3/Dockerfile.namespace").as_ref(),
        Default::default(),
    )
    .await
    .unwrap();
    fs.remove_dir(
        path!("/root/project/ci2").as_ref(),
        RemoveOptions {
            recursive: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    fs.clear_buffered_events();
    fs.unpause_events_and_flush();
    fs.emit_fs_event(path!("/root/project/ci2"), Some(PathEventKind::Removed));
    fs.emit_fs_event(path!("/root/project/ci3"), Some(PathEventKind::Created));

    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    repository.read_with(cx, |repository, _| {
        assert_eq!(
            repository.cached_status().collect::<Vec<_>>(),
            [StatusEntry {
                repo_path: repo_path("ci3/Dockerfile.namespace"),
                status: FileStatus::Untracked,
                diff_stat: None,
            }]
        );
    });
}

#[cfg(target_os = "linux")]
#[gpui::test(retries = 5)]
async fn test_git_events_after_project_excludes_dot_git(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions = Some(vec!["foo".to_string()]);
            });
        });
    });

    let root = TempTree::new(json!({
        "project": {
            "a.txt": "a",
        },
    }));

    let work_dir = root.path().join("project");
    let repo = git_init(&work_dir);
    git_add("a.txt", &repo);
    git_commit("Initial commit", &repo);
    git_branch("other-branch", &repo);

    let project = Project::test(
        Arc::new(RealFs::new(None, cx.executor())),
        [work_dir.as_path()],
        cx,
    )
    .await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });
    let branch = repository.read_with(cx, |repository, _| {
        repository
            .snapshot()
            .branch
            .as_ref()
            .map(|branch| branch.ref_name.to_string())
    });
    assert_eq!(branch.as_deref(), Some("refs/heads/main"));

    let worktree_id = tree.read_with(cx, |tree, _| tree.id());
    cx.update_global::<SettingsStore, _>(|store, cx| {
        store
            .set_local_settings(
                worktree_id,
                LocalSettingsPath::InWorktree(Arc::from(RelPath::empty())),
                LocalSettingsKind::Settings,
                Some(r#"{ "file_scan_exclusions": ["**/.git"] }"#),
                cx,
            )
            .unwrap();
    });
    cx.read(|cx| tree.read(cx).as_local().unwrap().scan_complete())
        .await;
    cx.executor().run_until_parked();

    cx.update(|cx| {
        assert!(tree.read(cx).entry_for_path(rel_path(".git")).is_none());
    });

    git_checkout("other-branch", &repo);

    let mut events = cx.events::<RepositoryEvent, _>(&repository);
    let timeout = futures::FutureExt::fuse(cx.background_executor.timer(Duration::from_secs(5)));
    futures::pin_mut!(timeout);
    loop {
        let branch = repository.read_with(cx, |repository, _| {
            repository
                .snapshot()
                .branch
                .as_ref()
                .map(|branch| branch.ref_name.to_string())
        });
        if branch.as_deref() == Some("refs/heads/other-branch") {
            break;
        }

        futures::select_biased! {
            _ = events.next() => {}
            _ = timeout => panic!("timed out waiting for repository HEAD update after .git was excluded"),
        }
    }
}

#[gpui::test]
#[ignore]
async fn test_git_status_postprocessing(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let root = TempTree::new(json!({
        "project": {
            "sub": {},
            "a.txt": "",
        },
    }));

    let work_dir = root.path().join("project");
    let repo = git_init(work_dir.as_path());
    // a.txt exists in HEAD and the working copy but is deleted in the index.
    git_add("a.txt", &repo);
    git_commit("Initial commit", &repo);
    git_remove_index("a.txt".as_ref(), &repo);
    // `sub` is a nested git repository.
    let _sub = git_init(&work_dir.join("sub"));

    let project = Project::test(
        Arc::new(RealFs::new(None, cx.executor())),
        [root.path()],
        cx,
    )
    .await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project
            .repositories(cx)
            .values()
            .find(|repo| repo.read(cx).work_directory_abs_path.ends_with("project"))
            .unwrap()
            .clone()
    });

    repository.read_with(cx, |repository, _cx| {
        let entries = repository.cached_status().collect::<Vec<_>>();

        // `sub` doesn't appear in our computed statuses.
        // a.txt appears with a combined `DA` status.
        assert_eq!(
            entries,
            [StatusEntry {
                repo_path: repo_path("a.txt"),
                status: TrackedStatus {
                    index_status: StatusCode::Deleted,
                    worktree_status: StatusCode::Added
                }
                .into(),
                diff_stat: None,
            }]
        )
    });
}
