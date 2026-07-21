use super::*;

#[gpui::test]
async fn test_real_git_repository_new_resolves_normal_repository_paths(cx: &mut TestAppContext) {
    disable_git_global_config();
    cx.executor().allow_parking();

    let repo_dir = tempfile::tempdir().unwrap();
    git_init_repo(repo_dir.path());

    let repository = RealGitRepository::new(
        &repo_dir.path().join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    assert_same_path(&repository.git_dir, repo_dir.path().join(".git"));
    assert_same_path(&repository.common_dir, repo_dir.path().join(".git"));
    assert_same_path(
        repository.working_directory.as_ref().unwrap(),
        repo_dir.path(),
    );
    assert_same_path(
        original_repo_path_from_common_dir(&repository.common_dir).unwrap(),
        repo_dir.path(),
    );
}

#[gpui::test]
async fn test_check_access(cx: &mut TestAppContext) {
    disable_git_global_config();
    cx.executor().allow_parking();

    let repo_dir = tempfile::tempdir().unwrap();
    let repository = RealGitRepository::new(
        &repo_dir.path().join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    assert!(repository.check_access().await.is_err());
    git_init_repo(repo_dir.path());
    assert!(repository.check_access().await.is_ok());
}

#[gpui::test]
async fn test_real_git_repository_new_resolves_linked_worktree_paths(cx: &mut TestAppContext) {
    disable_git_global_config();
    cx.executor().allow_parking();

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_dir = temp_dir.path().join("repo");
    let worktree_dir = temp_dir.path().join("worktree");
    git_init_repo(&repo_dir);
    fs::write(repo_dir.join("file.txt"), "initial").unwrap();
    git_command(&repo_dir, ["add", "file.txt"]);
    git_command(&repo_dir, ["commit", "-m", "initial"]);
    git_command(
        &repo_dir,
        [
            OsString::from("worktree"),
            OsString::from("add"),
            OsString::from("-b"),
            OsString::from("feature"),
            worktree_dir.as_os_str().into(),
        ],
    );

    let repository = RealGitRepository::new(
        &worktree_dir.join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    assert_same_path(
        repository.working_directory.as_ref().unwrap(),
        &worktree_dir,
    );
    assert_same_path(&repository.common_dir, repo_dir.join(".git"));
    assert_same_path(
        original_repo_path_from_common_dir(&repository.common_dir).unwrap(),
        repo_dir,
    );
}

#[gpui::test]
async fn test_real_git_repository_new_supports_bare_repositories(cx: &mut TestAppContext) {
    disable_git_global_config();
    cx.executor().allow_parking();

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_dir = temp_dir.path().join("repo.git");
    git_command(
        temp_dir.path(),
        [
            OsString::from("init"),
            OsString::from("--bare"),
            repo_dir.as_os_str().into(),
        ],
    );

    let repository =
        RealGitRepository::new(&repo_dir, None, Some("git".into()), cx.executor()).unwrap();

    assert_same_path(&repository.git_dir, &repo_dir);
    assert_same_path(&repository.common_dir, &repo_dir);
    assert_eq!(repository.working_directory, None);
    assert_same_path(repository.main_repository_path(), &repo_dir);
    assert_eq!(
        repository
            .git_binary()
            .run(&["rev-parse", "--is-bare-repository"])
            .await
            .unwrap(),
        "true"
    );
}

#[gpui::test]
async fn test_change_branch_creates_local_tracking_branch_from_remote(cx: &mut TestAppContext) {
    disable_git_global_config();
    cx.executor().allow_parking();

    let temp_dir = tempfile::tempdir().unwrap();
    let remote_dir = temp_dir.path().join("remote.git");
    let seed_dir = temp_dir.path().join("seed");
    let clone_dir = temp_dir.path().join("clone");

    git_command(
        temp_dir.path(),
        [
            OsString::from("init"),
            OsString::from("--bare"),
            OsString::from("-b"),
            OsString::from("main"),
            remote_dir.as_os_str().into(),
        ],
    );
    git_init_repo(&seed_dir);
    fs::write(seed_dir.join("file.txt"), "main").unwrap();
    git_command(&seed_dir, ["add", "file.txt"]);
    git_command(&seed_dir, ["commit", "-m", "initial"]);
    git_command(&seed_dir, ["switch", "-c", "feature"]);
    fs::write(seed_dir.join("feature.txt"), "feature").unwrap();
    git_command(&seed_dir, ["add", "feature.txt"]);
    git_command(&seed_dir, ["commit", "-m", "feature"]);
    git_command(
        &seed_dir,
        [
            OsString::from("remote"),
            OsString::from("add"),
            OsString::from("origin"),
            remote_dir.as_os_str().into(),
        ],
    );
    git_command(&seed_dir, ["push", "-u", "origin", "main"]);
    git_command(&seed_dir, ["push", "-u", "origin", "feature"]);
    git_command(
        temp_dir.path(),
        [
            OsString::from("clone"),
            remote_dir.as_os_str().into(),
            clone_dir.as_os_str().into(),
        ],
    );

    let repository = RealGitRepository::new(
        &clone_dir.join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();
    let git = repository.git_binary_in_worktree().unwrap();
    assert!(
        git.run(&[
            "show-ref",
            "--verify",
            "--quiet",
            "refs/remotes/origin/feature"
        ])
        .await
        .is_ok()
    );
    assert!(
        git.run(&["show-ref", "--verify", "--quiet", "refs/heads/feature"])
            .await
            .is_err()
    );

    repository
        .change_branch("origin/feature".to_string())
        .await
        .unwrap();

    let git = repository.git_binary_in_worktree().unwrap();
    assert_eq!(
        git.run(&["branch", "--show-current"]).await.unwrap(),
        "feature"
    );
    assert_eq!(
        git.run(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}",])
            .await
            .unwrap(),
        "origin/feature"
    );

    git.run(&["checkout", "main"]).await.unwrap();
    git.run(&["branch", "--unset-upstream", "feature"])
        .await
        .unwrap();

    repository
        .change_branch("origin/feature".to_string())
        .await
        .unwrap();

    let git = repository.git_binary_in_worktree().unwrap();
    assert_eq!(
        git.run(&["branch", "--show-current"]).await.unwrap(),
        "feature"
    );
    assert_eq!(
        git.run(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}",])
            .await
            .unwrap(),
        "origin/feature"
    );
}

#[gpui::test]
fn test_real_git_repository_new_rejects_malformed_git_file(cx: &mut TestAppContext) {
    disable_git_global_config();

    let temp_dir = tempfile::tempdir().unwrap();
    let worktree_dir = temp_dir.path().join("worktree");
    fs::create_dir_all(&worktree_dir).unwrap();
    fs::write(worktree_dir.join(".git"), "not a gitdir file\n").unwrap();

    let error = match RealGitRepository::new(
        &worktree_dir.join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    ) {
        Ok(_) => panic!("malformed .git file should be rejected"),
        Err(error) => error,
    };

    assert!(
        error
            .to_string()
            .contains("expected .git file to start with 'gitdir: '"),
        "unexpected error: {error:#}"
    );
}
