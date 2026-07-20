use super::*;
use pretty_assertions::assert_eq;

#[track_caller]
/// We merge lhs into rhs.
fn merge_pending_ops_snapshots(
    source: Vec<pending_op::PendingOps>,
    mut target: Vec<pending_op::PendingOps>,
) -> Vec<pending_op::PendingOps> {
    for s_ops in source {
        if let Some(idx) = target.iter().zip(0..).find_map(|(ops, idx)| {
            if ops.repo_path == s_ops.repo_path {
                Some(idx)
            } else {
                None
            }
        }) {
            let t_ops = &mut target[idx];
            for s_op in s_ops.ops {
                if let Some(op_idx) = t_ops
                    .ops
                    .iter()
                    .zip(0..)
                    .find_map(|(op, idx)| if op.id == s_op.id { Some(idx) } else { None })
                {
                    let t_op = &mut t_ops.ops[op_idx];
                    match (s_op.job_status, t_op.job_status) {
                        (pending_op::JobStatus::Running, _) => {}
                        (s_st, pending_op::JobStatus::Running) => t_op.job_status = s_st,
                        (s_st, t_st) if s_st == t_st => {}
                        _ => unreachable!(),
                    }
                } else {
                    t_ops.ops.push(s_op);
                }
            }
            t_ops.ops.sort_by_key(|op| op.id);
        } else {
            target.push(s_ops);
        }
    }
    target
}

#[gpui::test]
async fn test_repository_pending_ops_staging(
    executor: gpui::BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "my-repo": {
                ".git": {},
                "a.txt": "a",
            }

        }),
    )
    .await;

    fs.set_status_for_repo(
        path!("/root/my-repo/.git").as_ref(),
        &[("a.txt", FileStatus::Untracked)],
    );

    let project = Project::test(fs.clone(), [path!("/root/my-repo").as_ref()], cx).await;
    let pending_ops_all = Arc::new(Mutex::new(SumTree::default()));
    project.update(cx, |project, cx| {
        let pending_ops_all = pending_ops_all.clone();
        cx.subscribe(project.git_store(), move |_, _, e, _| {
            if let GitStoreEvent::RepositoryUpdated(
                _,
                RepositoryEvent::PendingOpsChanged { pending_ops },
                _,
            ) = e
            {
                let merged = merge_pending_ops_snapshots(
                    pending_ops.items(()),
                    pending_ops_all.lock().items(()),
                );
                *pending_ops_all.lock() = SumTree::from_iter(merged.into_iter(), ());
            }
        })
        .detach();
    });
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let repo = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    // Ensure we have no pending ops for any of the untracked files
    repo.read_with(cx, |repo, _cx| {
        assert!(repo.pending_ops().next().is_none());
    });

    let mut id = 1u16;

    let mut assert_stage = async |path: RepoPath, stage| {
        let git_status = if stage {
            pending_op::GitStatus::Staged
        } else {
            pending_op::GitStatus::Unstaged
        };
        repo.update(cx, |repo, cx| {
            let task = if stage {
                repo.stage_entries(vec![path.clone()], cx)
            } else {
                repo.unstage_entries(vec![path.clone()], cx)
            };
            let ops = repo.pending_ops_for_path(&path).unwrap();
            assert_eq!(
                ops.ops.last(),
                Some(&pending_op::PendingOp {
                    id: id.into(),
                    git_status,
                    job_status: pending_op::JobStatus::Running
                })
            );
            task
        })
        .await
        .unwrap();

        repo.read_with(cx, |repo, _cx| {
            let ops = repo.pending_ops_for_path(&path).unwrap();
            assert_eq!(
                ops.ops.last(),
                Some(&pending_op::PendingOp {
                    id: id.into(),
                    git_status,
                    job_status: pending_op::JobStatus::Finished
                })
            );
        });

        id += 1;
    };

    assert_stage(repo_path("a.txt"), true).await;
    assert_stage(repo_path("a.txt"), false).await;
    assert_stage(repo_path("a.txt"), true).await;
    assert_stage(repo_path("a.txt"), false).await;
    assert_stage(repo_path("a.txt"), true).await;

    cx.run_until_parked();

    assert_eq!(
        pending_ops_all
            .lock()
            .get(&worktree::PathKey(repo_path("a.txt").as_ref().clone()), ())
            .unwrap()
            .ops,
        vec![
            pending_op::PendingOp {
                id: 1u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Finished
            },
            pending_op::PendingOp {
                id: 2u16.into(),
                git_status: pending_op::GitStatus::Unstaged,
                job_status: pending_op::JobStatus::Finished
            },
            pending_op::PendingOp {
                id: 3u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Finished
            },
            pending_op::PendingOp {
                id: 4u16.into(),
                git_status: pending_op::GitStatus::Unstaged,
                job_status: pending_op::JobStatus::Finished
            },
            pending_op::PendingOp {
                id: 5u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Finished
            }
        ],
    );

    repo.update(cx, |repo, _cx| {
        let git_statuses = repo.cached_status().collect::<Vec<_>>();

        assert_eq!(
            git_statuses,
            [StatusEntry {
                repo_path: repo_path("a.txt"),
                status: TrackedStatus {
                    index_status: StatusCode::Added,
                    worktree_status: StatusCode::Unmodified
                }
                .into(),
                diff_stat: Some(DiffStat {
                    added: 1,
                    deleted: 0,
                }),
            }]
        );
    });
}

#[gpui::test]
async fn test_repository_pending_ops_long_running_staging(
    executor: gpui::BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "my-repo": {
                ".git": {},
                "a.txt": "a",
            }

        }),
    )
    .await;

    fs.set_status_for_repo(
        path!("/root/my-repo/.git").as_ref(),
        &[("a.txt", FileStatus::Untracked)],
    );

    let project = Project::test(fs.clone(), [path!("/root/my-repo").as_ref()], cx).await;
    let pending_ops_all = Arc::new(Mutex::new(SumTree::default()));
    project.update(cx, |project, cx| {
        let pending_ops_all = pending_ops_all.clone();
        cx.subscribe(project.git_store(), move |_, _, e, _| {
            if let GitStoreEvent::RepositoryUpdated(
                _,
                RepositoryEvent::PendingOpsChanged { pending_ops },
                _,
            ) = e
            {
                let merged = merge_pending_ops_snapshots(
                    pending_ops.items(()),
                    pending_ops_all.lock().items(()),
                );
                *pending_ops_all.lock() = SumTree::from_iter(merged.into_iter(), ());
            }
        })
        .detach();
    });

    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let repo = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    repo.update(cx, |repo, cx| {
        repo.stage_entries(vec![repo_path("a.txt")], cx)
    })
    .detach();

    repo.update(cx, |repo, cx| {
        repo.stage_entries(vec![repo_path("a.txt")], cx)
    })
    .unwrap()
    .with_timeout(Duration::from_secs(1), &cx.executor())
    .await
    .unwrap();

    cx.run_until_parked();

    assert_eq!(
        pending_ops_all
            .lock()
            .get(&worktree::PathKey(repo_path("a.txt").as_ref().clone()), ())
            .unwrap()
            .ops,
        vec![
            pending_op::PendingOp {
                id: 1u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Skipped
            },
            pending_op::PendingOp {
                id: 2u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Finished
            }
        ],
    );

    repo.update(cx, |repo, _cx| {
        let git_statuses = repo.cached_status().collect::<Vec<_>>();

        assert_eq!(
            git_statuses,
            [StatusEntry {
                repo_path: repo_path("a.txt"),
                status: TrackedStatus {
                    index_status: StatusCode::Added,
                    worktree_status: StatusCode::Unmodified
                }
                .into(),
                diff_stat: Some(DiffStat {
                    added: 1,
                    deleted: 0,
                }),
            }]
        );
    });
}

#[gpui::test]
async fn test_repository_pending_ops_stage_all(
    executor: gpui::BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "my-repo": {
                ".git": {},
                "a.txt": "a",
                "b.txt": "b"
            }

        }),
    )
    .await;

    fs.set_status_for_repo(
        path!("/root/my-repo/.git").as_ref(),
        &[
            ("a.txt", FileStatus::Untracked),
            ("b.txt", FileStatus::Untracked),
        ],
    );

    let project = Project::test(fs.clone(), [path!("/root/my-repo").as_ref()], cx).await;
    let pending_ops_all = Arc::new(Mutex::new(SumTree::default()));
    project.update(cx, |project, cx| {
        let pending_ops_all = pending_ops_all.clone();
        cx.subscribe(project.git_store(), move |_, _, e, _| {
            if let GitStoreEvent::RepositoryUpdated(
                _,
                RepositoryEvent::PendingOpsChanged { pending_ops },
                _,
            ) = e
            {
                let merged = merge_pending_ops_snapshots(
                    pending_ops.items(()),
                    pending_ops_all.lock().items(()),
                );
                *pending_ops_all.lock() = SumTree::from_iter(merged.into_iter(), ());
            }
        })
        .detach();
    });
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let repo = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    repo.update(cx, |repo, cx| {
        repo.stage_entries(vec![repo_path("a.txt")], cx)
    })
    .await
    .unwrap();
    repo.update(cx, |repo, cx| repo.stage_all(cx))
        .await
        .unwrap();
    repo.update(cx, |repo, cx| repo.unstage_all(cx))
        .await
        .unwrap();

    cx.run_until_parked();

    assert_eq!(
        pending_ops_all
            .lock()
            .get(&worktree::PathKey(repo_path("a.txt").as_ref().clone()), ())
            .unwrap()
            .ops,
        vec![
            pending_op::PendingOp {
                id: 1u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Finished
            },
            pending_op::PendingOp {
                id: 2u16.into(),
                git_status: pending_op::GitStatus::Unstaged,
                job_status: pending_op::JobStatus::Finished
            },
        ],
    );
    assert_eq!(
        pending_ops_all
            .lock()
            .get(&worktree::PathKey(repo_path("b.txt").as_ref().clone()), ())
            .unwrap()
            .ops,
        vec![
            pending_op::PendingOp {
                id: 1u16.into(),
                git_status: pending_op::GitStatus::Staged,
                job_status: pending_op::JobStatus::Finished
            },
            pending_op::PendingOp {
                id: 2u16.into(),
                git_status: pending_op::GitStatus::Unstaged,
                job_status: pending_op::JobStatus::Finished
            },
        ],
    );

    repo.update(cx, |repo, _cx| {
        let git_statuses = repo.cached_status().collect::<Vec<_>>();

        assert_eq!(
            git_statuses,
            [
                StatusEntry {
                    repo_path: repo_path("a.txt"),
                    status: FileStatus::Untracked,
                    diff_stat: None,
                },
                StatusEntry {
                    repo_path: repo_path("b.txt"),
                    status: FileStatus::Untracked,
                    diff_stat: None,
                },
            ]
        );
    });
}
