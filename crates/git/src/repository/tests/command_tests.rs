use super::*;

#[gpui::test]
async fn test_build_command_untrusted_includes_both_safety_args(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let git = GitBinary::new(
        PathBuf::from("git"),
        dir.path().to_path_buf(),
        dir.path().join(".git"),
        cx.executor(),
        false,
    );
    let output = git
        .build_command(&["version"])
        .output()
        .await
        .expect("git version should succeed");
    assert!(output.status.success());

    let git = GitBinary::new(
        PathBuf::from("git"),
        dir.path().to_path_buf(),
        dir.path().join(".git"),
        cx.executor(),
        false,
    );
    let output = git
        .build_command(&["config", "--get", "core.fsmonitor"])
        .output()
        .await
        .expect("git config should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "false",
        "fsmonitor should be disabled for untrusted repos"
    );

    git_init_repo(dir.path());
    let git = GitBinary::new(
        PathBuf::from("git"),
        dir.path().to_path_buf(),
        dir.path().join(".git"),
        cx.executor(),
        false,
    );
    let output = git
        .build_command(&["config", "--get", "core.hooksPath"])
        .output()
        .await
        .expect("git config should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "/dev/null",
        "hooksPath should be /dev/null for untrusted repos"
    );
}

#[gpui::test]
async fn test_build_command_trusted_only_disables_fsmonitor(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    git_init_repo(dir.path());

    let git = GitBinary::new(
        PathBuf::from("git"),
        dir.path().to_path_buf(),
        dir.path().join(".git"),
        cx.executor(),
        true,
    );
    let output = git
        .build_command(&["config", "--get", "core.fsmonitor"])
        .output()
        .await
        .expect("git config should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "false",
        "fsmonitor should be disabled even for trusted repos"
    );

    let git = GitBinary::new(
        PathBuf::from("git"),
        dir.path().to_path_buf(),
        dir.path().join(".git"),
        cx.executor(),
        true,
    );
    let output = git
        .build_command(&["config", "--get", "core.hooksPath"])
        .output()
        .await
        .expect("git config should run");
    assert!(
        !output.status.success(),
        "hooksPath should NOT be overridden for trusted repos"
    );
}

#[gpui::test]
async fn test_build_command_disables_log_show_signature(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    git_init_repo(dir.path());

    let git = GitBinary::new(
        PathBuf::from("git"),
        dir.path().to_path_buf(),
        dir.path().join(".git"),
        cx.executor(),
        true,
    );
    let output = git
        .build_command(&["config", "--get", "log.showSignature"])
        .output()
        .await
        .expect("git config should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "false",
        "log.showSignature should be disabled for trusted repos"
    );

    let git = GitBinary::new(
        PathBuf::from("git"),
        dir.path().to_path_buf(),
        dir.path().join(".git"),
        cx.executor(),
        false,
    );
    let output = git
        .build_command(&["config", "--get", "log.showSignature"])
        .output()
        .await
        .expect("git config should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "false",
        "log.showSignature should be disabled for untrusted repos"
    );
}

#[gpui::test]
async fn test_path_for_index_id_uses_real_git_directory(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let working_directory = PathBuf::from("/code/worktree");
    let git_directory = PathBuf::from("/code/repo/.git/modules/worktree");
    let git = GitBinary::new(
        PathBuf::from("git"),
        working_directory,
        git_directory.clone(),
        cx.executor(),
        false,
    );

    let path = git.path_for_index_id(Uuid::nil());

    assert_eq!(
        path,
        git_directory.join(format!("index-{}.tmp", Uuid::nil()))
    );
}
