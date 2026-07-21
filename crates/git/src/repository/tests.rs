use std::{
    ffi::{OsStr, OsString},
    fs,
};

use super::*;
use gpui::TestAppContext;

fn disable_git_global_config() {
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", "");
        std::env::set_var("GIT_CONFIG_SYSTEM", "");
    }
}

#[allow(clippy::disallowed_methods)]
#[track_caller]
fn git_command<I, S>(working_directory: &Path, arguments: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = std::process::Command::new("git")
        .args(arguments)
        .current_dir(working_directory)
        .env("GIT_CONFIG_GLOBAL", "")
        .env("GIT_CONFIG_SYSTEM", "")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@mav.dev")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@mav.dev")
        .output()
        .expect("failed to run git command");
    assert!(
        output.status.success(),
        "git command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_init_repo(path: &Path) {
    fs::create_dir_all(path).expect("failed to create repo directory");
    git_command(path, ["init", "-b", "main"]);
}

fn test_commit_envs() -> HashMap<String, String> {
    let mut env = checkpoint_author_envs();
    env.insert("GIT_ASKPASS".to_string(), "false".to_string());
    env
}

#[track_caller]
fn assert_same_path(left: impl AsRef<Path>, right: impl AsRef<Path>) {
    assert_eq!(
        fs::canonicalize(left.as_ref()).unwrap(),
        fs::canonicalize(right.as_ref()).unwrap()
    );
}

mod branch_parsing_tests;
mod checkpoint_tests;
mod command_tests;
mod graph_remote_tests;
mod repository_init_tests;
mod worktree_tests;
