use super::*;

#[test]
fn test_parse_worktrees_from_str() {
    // Empty input
    let result = parse_worktrees_from_str("", None);
    assert!(result.is_empty());

    // Single worktree (main)
    let input = "worktree /home/user/project\nHEAD abc123def\nbranch refs/heads/main\n\n";
    let result = parse_worktrees_from_str(input, Some(Path::new("/home/user/project")));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, PathBuf::from("/home/user/project"));
    assert_eq!(result[0].sha.as_ref(), "abc123def");
    assert_eq!(result[0].ref_name, Some("refs/heads/main".into()));
    assert!(result[0].is_main);
    assert!(!result[0].is_bare);

    // Multiple worktrees
    let input = "worktree /home/user/project-wt\nHEAD def456\nbranch refs/heads/feature\n\n\
                  worktree /home/user/project\nHEAD abc123\nbranch refs/heads/main\n\n";
    let result = parse_worktrees_from_str(input, Some(Path::new("/home/user/project")));
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].path, PathBuf::from("/home/user/project-wt"));
    assert_eq!(result[0].ref_name, Some("refs/heads/feature".into()));
    assert!(!result[0].is_main);
    assert!(!result[0].is_bare);
    assert_eq!(result[1].path, PathBuf::from("/home/user/project"));
    assert_eq!(result[1].ref_name, Some("refs/heads/main".into()));
    assert!(result[1].is_main);
    assert!(!result[1].is_bare);

    // Detached HEAD entry (included with ref_name: None)
    let input = "worktree /home/user/project\nHEAD abc123\nbranch refs/heads/main\n\n\
                  worktree /home/user/detached\nHEAD def456\ndetached\n\n";
    let result = parse_worktrees_from_str(input, Some(Path::new("/home/user/project")));
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].path, PathBuf::from("/home/user/project"));
    assert_eq!(result[0].ref_name, Some("refs/heads/main".into()));
    assert!(result[0].is_main);
    assert_eq!(result[1].path, PathBuf::from("/home/user/detached"));
    assert_eq!(result[1].ref_name, None);
    assert_eq!(result[1].sha.as_ref(), "def456");
    assert!(!result[1].is_main);
    assert!(!result[1].is_bare);

    // Bare repo entry with no main worktree.
    let input = "worktree /home/user/bare.git\nHEAD abc123\nbare\n\n\
                  worktree /home/user/project\nHEAD def456\nbranch refs/heads/main\n\n";
    let result = parse_worktrees_from_str(input, None);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].path, PathBuf::from("/home/user/bare.git"));
    assert_eq!(result[0].ref_name, None);
    assert!(!result[0].is_main);
    assert!(result[0].is_bare);
    assert_eq!(result[1].path, PathBuf::from("/home/user/project"));
    assert_eq!(result[1].ref_name, Some("refs/heads/main".into()));
    assert!(!result[1].is_main);
    assert!(!result[1].is_bare);

    // Extra porcelain lines (locked, prunable) should be ignored
    let input = "worktree /home/user/project\nHEAD abc123\nbranch refs/heads/main\n\n\
                  worktree /home/user/locked-wt\nHEAD def456\nbranch refs/heads/locked-branch\nlocked\n\n\
                  worktree /home/user/prunable-wt\nHEAD 789aaa\nbranch refs/heads/prunable-branch\nprunable\n\n";
    let result = parse_worktrees_from_str(input, Some(Path::new("/home/user/project")));
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].path, PathBuf::from("/home/user/project"));
    assert_eq!(result[0].ref_name, Some("refs/heads/main".into()));
    assert!(result[0].is_main);
    assert_eq!(result[1].path, PathBuf::from("/home/user/locked-wt"));
    assert_eq!(result[1].ref_name, Some("refs/heads/locked-branch".into()));
    assert!(!result[1].is_main);
    assert_eq!(result[2].path, PathBuf::from("/home/user/prunable-wt"));
    assert_eq!(
        result[2].ref_name,
        Some("refs/heads/prunable-branch".into())
    );
    assert!(!result[2].is_main);

    // Leading/trailing whitespace on lines should be tolerated
    let input = "  worktree /home/user/project  \n  HEAD abc123  \n  branch refs/heads/main  \n\n";
    let result = parse_worktrees_from_str(input, Some(Path::new("/home/user/project")));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, PathBuf::from("/home/user/project"));
    assert_eq!(result[0].sha.as_ref(), "abc123");
    assert_eq!(result[0].ref_name, Some("refs/heads/main".into()));
    assert!(result[0].is_main);

    // Windows-style line endings should be handled
    let input = "worktree /home/user/project\r\nHEAD abc123\r\nbranch refs/heads/main\r\n\r\n";
    let result = parse_worktrees_from_str(input, Some(Path::new("/home/user/project")));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, PathBuf::from("/home/user/project"));
    assert_eq!(result[0].sha.as_ref(), "abc123");
    assert_eq!(result[0].ref_name, Some("refs/heads/main".into()));
    assert!(result[0].is_main);
}

#[gpui::test]
async fn test_create_and_list_worktrees(cx: &mut TestAppContext) {
    disable_git_global_config();
    cx.executor().allow_parking();

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_dir = temp_dir.path().join("repo");
    let worktrees_dir = temp_dir.path().join("worktrees");

    fs::create_dir_all(&repo_dir).unwrap();
    fs::create_dir_all(&worktrees_dir).unwrap();

    git_init_repo(&repo_dir);

    let repo = RealGitRepository::new(
        &repo_dir.join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    // Create an initial commit (required for worktrees)
    smol::fs::write(repo_dir.join("file.txt"), "content")
        .await
        .unwrap();
    repo.stage_paths(vec![repo_path("file.txt")], Arc::new(HashMap::default()))
        .await
        .unwrap();
    repo.commit(
        "Initial commit".into(),
        None,
        CommitOptions::default(),
        AskPassDelegate::new(&mut cx.to_async(), |_, _, _| {}),
        Arc::new(test_commit_envs()),
    )
    .await
    .unwrap();

    // List worktrees — should have just the main one
    let worktrees = repo.worktrees().await.unwrap();
    assert_eq!(worktrees.len(), 1);
    assert_eq!(
        worktrees[0].path.canonicalize().unwrap(),
        repo_dir.canonicalize().unwrap()
    );

    let worktree_path = worktrees_dir.join("some-worktree");

    // Create a new worktree
    repo.create_worktree(
        CreateWorktreeTarget::NewBranch {
            branch_name: "test-branch".to_string(),
            base_sha: Some("HEAD".to_string()),
        },
        worktree_path.clone(),
    )
    .await
    .unwrap();

    // List worktrees — should have two
    let worktrees = repo.worktrees().await.unwrap();
    assert_eq!(worktrees.len(), 2);

    let new_worktree = worktrees
        .iter()
        .find(|w| w.display_name() == "test-branch")
        .expect("should find worktree with test-branch");
    assert_eq!(
        new_worktree.path.canonicalize().unwrap(),
        worktree_path.canonicalize().unwrap(),
    );

    // The new worktree's git metadata directory should report a creation
    // time, resolved via the worktree's `.git` file.
    let created_at = repo
        .worktree_created_at(worktree_path.clone())
        .await
        .unwrap();
    assert!(
        created_at.is_some(),
        "creation time should be available for a freshly created worktree"
    );

    // A path with no worktree at all reports `None`.
    let missing = repo
        .worktree_created_at(worktrees_dir.join("does-not-exist"))
        .await
        .unwrap();
    assert_eq!(missing, None);
}

#[gpui::test]
async fn test_remove_worktree(cx: &mut TestAppContext) {
    disable_git_global_config();
    cx.executor().allow_parking();

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_dir = temp_dir.path().join("repo");
    let worktrees_dir = temp_dir.path().join("worktrees");
    git_init_repo(&repo_dir);

    let repo = RealGitRepository::new(
        &repo_dir.join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    // Create an initial commit
    smol::fs::write(repo_dir.join("file.txt"), "content")
        .await
        .unwrap();
    repo.stage_paths(vec![repo_path("file.txt")], Arc::new(HashMap::default()))
        .await
        .unwrap();
    repo.commit(
        "Initial commit".into(),
        None,
        CommitOptions::default(),
        AskPassDelegate::new(&mut cx.to_async(), |_, _, _| {}),
        Arc::new(test_commit_envs()),
    )
    .await
    .unwrap();

    // Create a worktree
    let worktree_path = worktrees_dir.join("worktree-to-remove");
    repo.create_worktree(
        CreateWorktreeTarget::NewBranch {
            branch_name: "to-remove".to_string(),
            base_sha: Some("HEAD".to_string()),
        },
        worktree_path.clone(),
    )
    .await
    .unwrap();

    // Remove the worktree
    repo.remove_worktree(worktree_path.clone(), false)
        .await
        .unwrap();

    // Verify the directory is removed
    let worktrees = repo.worktrees().await.unwrap();
    assert_eq!(worktrees.len(), 1);
    assert!(
        worktrees.iter().all(|w| w.display_name() != "to-remove"),
        "removed worktree should not appear in list"
    );
    assert!(!worktree_path.exists());

    // Create a worktree
    let worktree_path = worktrees_dir.join("dirty-wt");
    repo.create_worktree(
        CreateWorktreeTarget::NewBranch {
            branch_name: "dirty-wt".to_string(),
            base_sha: Some("HEAD".to_string()),
        },
        worktree_path.clone(),
    )
    .await
    .unwrap();

    assert!(worktree_path.exists());

    // Add uncommitted changes in the worktree
    smol::fs::write(worktree_path.join("dirty-file.txt"), "uncommitted")
        .await
        .unwrap();

    // Non-force removal should fail with dirty worktree
    let result = repo.remove_worktree(worktree_path.clone(), false).await;
    assert!(
        result.is_err(),
        "non-force removal of dirty worktree should fail"
    );

    // Force removal should succeed
    repo.remove_worktree(worktree_path.clone(), true)
        .await
        .unwrap();

    let worktrees = repo.worktrees().await.unwrap();
    assert_eq!(worktrees.len(), 1);
    assert!(!worktree_path.exists());
}

#[gpui::test]
async fn test_rename_worktree(cx: &mut TestAppContext) {
    disable_git_global_config();
    cx.executor().allow_parking();

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_dir = temp_dir.path().join("repo");
    let worktrees_dir = temp_dir.path().join("worktrees");

    git_init_repo(&repo_dir);

    let repo = RealGitRepository::new(
        &repo_dir.join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    // Create an initial commit
    smol::fs::write(repo_dir.join("file.txt"), "content")
        .await
        .unwrap();
    repo.stage_paths(vec![repo_path("file.txt")], Arc::new(HashMap::default()))
        .await
        .unwrap();
    repo.commit(
        "Initial commit".into(),
        None,
        CommitOptions::default(),
        AskPassDelegate::new(&mut cx.to_async(), |_, _, _| {}),
        Arc::new(test_commit_envs()),
    )
    .await
    .unwrap();

    // Create a worktree
    let old_path = worktrees_dir.join("old-worktree-name");
    repo.create_worktree(
        CreateWorktreeTarget::NewBranch {
            branch_name: "old-name".to_string(),
            base_sha: Some("HEAD".to_string()),
        },
        old_path.clone(),
    )
    .await
    .unwrap();

    assert!(old_path.exists());

    // Move the worktree to a new path
    let new_path = worktrees_dir.join("new-worktree-name");
    repo.rename_worktree(old_path.clone(), new_path.clone())
        .await
        .unwrap();

    // Verify the old path is gone and new path exists
    assert!(!old_path.exists());
    assert!(new_path.exists());

    // Verify it shows up in worktree list at the new path
    let worktrees = repo.worktrees().await.unwrap();
    assert_eq!(worktrees.len(), 2);
    let moved_worktree = worktrees
        .iter()
        .find(|w| w.display_name() == "old-name")
        .expect("should find worktree by branch name");
    assert_eq!(
        moved_worktree.path.canonicalize().unwrap(),
        new_path.canonicalize().unwrap()
    );
}
