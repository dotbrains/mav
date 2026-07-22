use super::*;

#[gpui::test]
async fn test_remote_root_repo_common_dir(cx: &mut TestAppContext, server_cx: &mut TestAppContext) {
    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        "/code",
        json!({
            "main_repo": {
                ".git": {},
                "file.txt": "content",
            },
            "no_git": {
                "file.txt": "content",
            },
        }),
    )
    .await;

    // Create a linked worktree that points back to main_repo's .git.
    fs.add_linked_worktree_for_repo(
        Path::new("/code/main_repo/.git"),
        false,
        GitWorktree {
            path: PathBuf::from("/code/linked_worktree"),
            ref_name: Some("refs/heads/feature-branch".into()),
            sha: "abc123".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    let (project, _headless) = init_test(&fs, cx, server_cx).await;

    // Main repo: root_repo_common_dir should be the .git directory itself.
    let (worktree_main, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/code/main_repo", true, cx)
        })
        .await
        .unwrap();
    cx.executor().run_until_parked();

    let common_dir = worktree_main.read_with(cx, |worktree, _| {
        worktree.snapshot().root_repo_common_dir().cloned()
    });
    assert_eq!(
        common_dir.as_deref(),
        Some(Path::new("/code/main_repo/.git")),
    );

    // Linked worktree: root_repo_common_dir should point to the main repo's .git.
    let (worktree_linked, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/code/linked_worktree", true, cx)
        })
        .await
        .unwrap();
    cx.executor().run_until_parked();

    let common_dir = worktree_linked.read_with(cx, |worktree, _| {
        worktree.snapshot().root_repo_common_dir().cloned()
    });
    assert_eq!(
        common_dir.as_deref(),
        Some(Path::new("/code/main_repo/.git")),
    );

    // No git repo: root_repo_common_dir should be None.
    let (worktree_no_git, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/code/no_git", true, cx)
        })
        .await
        .unwrap();
    cx.executor().run_until_parked();

    let common_dir = worktree_no_git.read_with(cx, |worktree, _| {
        worktree.snapshot().root_repo_common_dir().cloned()
    });
    assert_eq!(common_dir, None);
}

#[gpui::test]
async fn test_remote_search_commits_streams_proto_chunks(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    const COMMIT_COUNT: usize = 900;
    const RESPONSE_MAX_SIZE: usize = 100;

    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        path!("/code"),
        json!({
            "project1": {
                ".git": {},
                "file.txt": "content",
            },
        }),
    )
    .await;

    let commit_data = (0..COMMIT_COUNT)
        .map(|index| {
            let sha = Oid::from_str(&format!("{:040x}", index + 1)).unwrap();
            (
                CommitData {
                    sha,
                    parents: Default::default(),
                    author_name: SharedString::from("Author"),
                    author_email: SharedString::from("author@example.com"),
                    commit_timestamp: index as i64,
                    subject: SharedString::from(format!("Subject {index}")),
                    message: SharedString::from(format!("needle commit {index}")),
                },
                false,
            )
        })
        .collect::<Vec<_>>();
    let expected_shas = commit_data
        .iter()
        .map(|(commit_data, _)| commit_data.sha.to_string())
        .collect::<HashSet<_>>();
    fs.set_commit_data(Path::new(path!("/code/project1/.git")), commit_data);

    let (project, _headless) = init_test(&fs, cx, server_cx).await;
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/code/project1"), true, cx)
        })
        .await
        .expect("should open remote worktree");
    server_cx.run_until_parked();
    cx.run_until_parked();
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let (remote_client, repository_id) = project.read_with(cx, |project, cx| {
        let repository = project
            .active_repository(cx)
            .expect("remote project should have an active repository");
        let repository_id = repository.read(cx).snapshot().id;
        let remote_client = project
            .remote_client()
            .expect("project should have a remote client");
        (remote_client, repository_id)
    });
    let proto_client = remote_client.read_with(cx, |remote_client, _| remote_client.proto_client());
    let mut stream = proto_client
        .request_stream(proto::SearchCommits {
            project_id: proto::REMOTE_SERVER_PROJECT_ID,
            repository_id: repository_id.to_proto(),
            log_source: Some(proto::GitLogSource {
                source: Some(proto::git_log_source::Source::All(
                    proto::GitLogSourceAll {},
                )),
            }),
            query: "needle".to_string(),
            case_sensitive: true,
        })
        .await
        .expect("search commits stream should start");

    let mut chunks = Vec::new();
    while let Some(response) = futures::StreamExt::next(&mut stream).await {
        chunks.push(response.expect("search commits chunk should succeed").shas);
    }

    assert!(
        chunks.len() > 1,
        "expected search results to stream in multiple chunks"
    );
    for chunk in chunks.iter().take(chunks.len() - 1) {
        assert!(
            chunk.len() <= RESPONSE_MAX_SIZE,
            "non-final chunks should meet the target byte size"
        );
    }

    let actual_shas = chunks.into_iter().flatten().collect::<HashSet<_>>();
    assert_eq!(actual_shas, expected_shas);
}

#[gpui::test]
async fn test_remote_archive_git_operations_are_supported(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            ".git": {},
            "file.txt": "content",
        }),
    )
    .await;
    fs.set_branch_name(Path::new("/project/.git"), Some("main"));
    fs.set_head_for_repo(
        Path::new("/project/.git"),
        &[("file.txt", "content".into())],
        "head-sha",
    );

    let (project, _headless) = init_test(&fs, cx, server_cx).await;
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(Path::new("/project"), true, cx)
        })
        .await
        .expect("should open remote worktree");
    cx.run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project
            .active_repository(cx)
            .expect("remote project should have an active repository")
    });

    let head_sha = cx
        .update(|cx| repository.update(cx, |repository, _| repository.head_sha()))
        .await
        .expect("head_sha request should complete")
        .expect("head_sha should succeed")
        .expect("HEAD should exist");

    cx.run_until_parked();

    cx.update(|cx| {
        repository.update(cx, |repository, _| {
            repository.update_ref("refs/mav-tests/archive-checkpoint".to_string(), head_sha)
        })
    })
    .await
    .expect("update_ref request should complete")
    .expect("update_ref should succeed for remote repository");

    cx.run_until_parked();

    cx.update(|cx| {
        repository.update(cx, |repository, _| {
            repository.delete_ref("refs/mav-tests/archive-checkpoint".to_string())
        })
    })
    .await
    .expect("delete_ref request should complete")
    .expect("delete_ref should succeed for remote repository");

    cx.run_until_parked();

    cx.update(|cx| repository.update(cx, |repository, _| repository.repair_worktrees()))
        .await
        .expect("repair_worktrees request should complete")
        .expect("repair_worktrees should succeed for remote repository");

    cx.run_until_parked();

    let (staged_commit_sha, unstaged_commit_sha) = cx
        .update(|cx| repository.update(cx, |repository, _| repository.create_archive_checkpoint()))
        .await
        .expect("create_archive_checkpoint request should complete")
        .expect("create_archive_checkpoint should succeed for remote repository");

    cx.run_until_parked();

    cx.update(|cx| {
        repository.update(cx, |repository, _| {
            repository.restore_archive_checkpoint(staged_commit_sha, unstaged_commit_sha)
        })
    })
    .await
    .expect("restore_archive_checkpoint request should complete")
    .expect("restore_archive_checkpoint should succeed for remote repository");
}
