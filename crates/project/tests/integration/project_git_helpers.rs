use super::*;

#[cfg(target_os = "linux")]
pub(crate) const GIT_STATUS_CONFLICTED: &str = "UU";

#[allow(clippy::disallowed_methods)]
pub(crate) fn git_cmd(work_dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(work_dir)
        .env("GIT_CONFIG_GLOBAL", "")
        .env("GIT_CONFIG_SYSTEM", "")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@mav.dev")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@mav.dev");
    cmd
}

#[allow(clippy::disallowed_methods)]
#[track_caller]
pub(crate) fn git_init(path: &Path) -> PathBuf {
    let output = git_cmd(path)
        .args(["init", "-b", "main"])
        .output()
        .expect("Failed to run git init");
    assert!(
        output.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    path.to_path_buf()
}

#[allow(clippy::disallowed_methods)]
#[track_caller]
pub(crate) fn git_add<P: AsRef<Path>>(path: P, work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["add"])
        .arg(path.as_ref())
        .output()
        .expect("Failed to run git add");
    assert!(
        output.status.success(),
        "git add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[allow(clippy::disallowed_methods)]
#[track_caller]
pub(crate) fn git_remove_index(path: &Path, work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["rm", "--cached"])
        .arg(path)
        .output()
        .expect("Failed to run git rm");
    assert!(
        output.status.success(),
        "git rm --cached failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[allow(clippy::disallowed_methods)]
#[track_caller]
pub(crate) fn git_commit(msg: &str, work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["commit", "-m", msg])
        .output()
        .expect("Failed to run git commit");
    assert!(
        output.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[allow(clippy::disallowed_methods)]
#[track_caller]
pub(crate) fn git_stash(work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["stash"])
        .output()
        .expect("Failed to run git stash");
    assert!(
        output.status.success(),
        "git stash failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[allow(clippy::disallowed_methods)]
#[track_caller]
pub(crate) fn git_reset(offset: usize, work_dir: &Path) {
    let target = format!("HEAD~{}", offset + 1);
    let output = git_cmd(work_dir)
        .args(["reset", "--soft", &target])
        .output()
        .expect("Failed to run git reset");
    assert!(
        output.status.success(),
        "git reset failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(target_os = "linux")]
#[allow(clippy::disallowed_methods)]
#[track_caller]
pub(crate) fn git_branch(name: &str, work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["branch", name])
        .output()
        .expect("Failed to run git branch");
    assert!(
        output.status.success(),
        "git branch failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(target_os = "linux")]
#[allow(clippy::disallowed_methods)]
#[track_caller]
pub(crate) fn git_checkout(name: &str, work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["checkout", name])
        .output()
        .expect("Failed to run git checkout");
    assert!(
        output.status.success(),
        "git checkout failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(any())]
#[allow(clippy::disallowed_methods)]
#[track_caller]
pub(crate) fn git_rev_parse(rev: &str, work_dir: &Path) -> String {
    let output = git_cmd(work_dir)
        .args(["rev-parse", rev])
        .output()
        .expect("Failed to run git rev-parse");
    assert!(
        output.status.success(),
        "git rev-parse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[cfg(any())]
#[allow(clippy::disallowed_methods)]
#[track_caller]
pub(crate) fn git_cherry_pick_expect_conflict(commit: &str, work_dir: &Path) {
    let output = git_cmd(work_dir)
        .args(["cherry-pick", "--no-commit", commit])
        .output()
        .expect("Failed to run git cherry-pick");
    assert!(
        !output.status.success(),
        "git cherry-pick unexpectedly succeeded"
    );
}

#[cfg(any())]
#[allow(clippy::disallowed_methods)]
#[track_caller]
pub(crate) fn git_status(work_dir: &Path) -> collections::HashMap<String, String> {
    let output = git_cmd(work_dir)
        .args(["status", "--porcelain=v1"])
        .output()
        .expect("Failed to run git status");
    assert!(
        output.status.success(),
        "git status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let status = line[..2].to_string();
            let path = line[3..].to_string();
            (path, status)
        })
        .collect()
}
