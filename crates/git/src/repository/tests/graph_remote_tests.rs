use super::*;

#[gpui::test]
async fn test_initial_graph_data_ref_set(cx: &mut TestAppContext) {
    disable_git_global_config();
    cx.executor().allow_parking();

    let repo_dir = tempfile::tempdir().unwrap();
    git_init_repo(repo_dir.path());

    let repo = RealGitRepository::new(
        &repo_dir.path().join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();
    let git = repo.git_binary();

    let graph_commits = async || {
        let (tx, rx) = smol::channel::unbounded();
        repo.initial_graph_data(LogSource::All, LogOrder::DateOrder, tx)
            .await
            .unwrap();
        let mut commits = std::collections::HashSet::new();
        while let Ok(chunk) = rx.try_recv() {
            for commit in chunk {
                commits.insert(commit.sha);
            }
        }
        commits
    };

    smol::fs::write(repo_dir.path().join("file1"), "1")
        .await
        .unwrap();
    let branch_sha = repo.checkpoint().await.unwrap().commit_sha;
    repo.update_ref("refs/heads/main".into(), branch_sha.to_string())
        .await
        .unwrap();

    smol::fs::write(repo_dir.path().join("file2"), "2")
        .await
        .unwrap();
    let hidden_sha = repo.checkpoint().await.unwrap().commit_sha;
    repo.update_ref("refs/custom/hidden".into(), hidden_sha.to_string())
        .await
        .unwrap();

    let graph = graph_commits().await;
    assert!(graph.contains(&branch_sha));
    assert!(!graph.contains(&hidden_sha));

    git.build_command(&["update-ref", "--no-deref", "HEAD", &hidden_sha.to_string()])
        .output()
        .await
        .unwrap();

    let graph = graph_commits().await;
    assert!(graph.contains(&branch_sha));
    assert!(graph.contains(&hidden_sha));
}

#[gpui::test]
async fn test_check_for_pushed_commit(cx: &mut TestAppContext) {
    disable_git_global_config();
    cx.executor().allow_parking();

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_dir = temp_dir.path().join("repo");
    git_init_repo(&repo_dir);

    let repo = RealGitRepository::new(
        &repo_dir.join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    // New repo doesn't have any commits yet
    assert!(repo.check_for_pushed_commit().await.unwrap().is_empty());

    git_command(
        &repo_dir,
        ["commit", "--allow-empty", "-m", "Initial commit"],
    );

    // No remote branches exist yet
    assert!(repo.check_for_pushed_commit().await.unwrap().is_empty());

    // Create simulated remote branches
    git_command(
        &repo_dir,
        ["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    git_command(
        &repo_dir,
        ["update-ref", "refs/remotes/origin/other-branch", "HEAD"],
    );
    assert_eq!(
        repo.check_for_pushed_commit().await.unwrap(),
        vec![
            SharedString::from("origin/main"),
            SharedString::from("origin/other-branch")
        ]
    );

    // Switch to a new branch, commit but do not push
    git_command(&repo_dir, ["switch", "-c", "local-feature"]);
    git_command(&repo_dir, ["commit", "--allow-empty", "-m", "Local commit"]);

    // New commit has not been pushed
    assert!(repo.check_for_pushed_commit().await.unwrap().is_empty());
}

#[test]
fn test_original_repo_path_from_common_dir() {
    // Normal repo: common_dir is <work_dir>/.git
    assert_eq!(
        original_repo_path_from_common_dir(Path::new("/code/mav5/.git")),
        Some(PathBuf::from("/code/mav5"))
    );

    // Worktree: common_dir is the main repo's .git
    // (same result — that's the point, it always traces back to the original)
    assert_eq!(
        original_repo_path_from_common_dir(Path::new("/code/mav5/.git")),
        Some(PathBuf::from("/code/mav5"))
    );

    // Bare repo: no .git suffix, returns None (no working-tree root)
    assert_eq!(
        original_repo_path_from_common_dir(Path::new("/code/mav5.git")),
        None
    );

    // Root-level .git directory
    assert_eq!(
        original_repo_path_from_common_dir(Path::new("/.git")),
        Some(PathBuf::from("/"))
    );
}

#[gpui::test]
async fn test_default_branch(cx: &mut TestAppContext) {
    disable_git_global_config();
    cx.executor().allow_parking();

    let repo_dir = tempfile::tempdir().unwrap();
    git_init_repo(repo_dir.path());

    let repo = RealGitRepository::new(
        &repo_dir.path().join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    assert_eq!(repo.default_branch(false).await.unwrap(), None);

    git_command(
        repo_dir.path(),
        ["commit", "--allow-empty", "-m", "Initial commit"],
    );

    assert_eq!(
        repo.default_branch(false).await.unwrap(),
        Some("main".into())
    );

    git_command(
        repo_dir.path(),
        ["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    git_command(
        repo_dir.path(),
        [
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );

    assert_eq!(
        repo.default_branch(false).await.unwrap(),
        Some("main".into())
    );
    assert_eq!(
        repo.default_branch(true).await.unwrap(),
        Some("origin/main".into())
    );
}

impl RealGitRepository {
    /// Force a Git garbage collection on the repository.
    fn gc(&self) -> BoxFuture<'_, Result<()>> {
        let working_directory = self.command_directory();
        let git_directory = self.path();
        let git_binary_path = self.any_git_binary_path.clone();
        let executor = self.executor.clone();
        self.executor
            .spawn(async move {
                let git_binary_path = git_binary_path.clone();
                let git = GitBinary::new(
                    git_binary_path,
                    working_directory,
                    git_directory,
                    executor,
                    true,
                );
                git.run(&["gc", "--prune"]).await?;
                Ok(())
            })
            .boxed()
    }
}

#[gpui::test]
async fn test_remote_urls(cx: &mut TestAppContext) {
    disable_git_global_config();
    cx.executor().allow_parking();

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_dir = temp_dir.path().join("repo");
    std::fs::create_dir_all(&repo_dir).unwrap();

    git_init_repo(&repo_dir);

    let repo = RealGitRepository::new(
        &repo_dir.join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    let git = repo.git_binary();
    git.run(&[
        "remote",
        "add",
        "origin",
        "https://github.com/mav-industries/mav.git",
    ])
    .await
    .unwrap();
    git.run(&[
        "remote",
        "add",
        "upstream",
        "/Users/user/My Projects/upstream.git",
    ])
    .await
    .unwrap();

    let remote_urls = repo.remote_urls().await;
    assert_eq!(remote_urls.len(), 2);
    assert_eq!(
        remote_urls.get("origin").unwrap(),
        "https://github.com/mav-industries/mav.git"
    );
    assert_eq!(
        remote_urls.get("upstream").unwrap(),
        "/Users/user/My Projects/upstream.git"
    );
}
