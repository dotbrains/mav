use crate::commit::parse_git_diff_name_status;
use crate::stash::GitStash;
use crate::status::{DiffTreeType, GitStatus, StatusCode, TreeDiff};
use crate::{Oid, RunHook};
use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::Sender;
use collections::HashMap;
use futures::channel::oneshot;
use futures::future::BoxFuture;
use futures::io::BufWriter;
use futures::{AsyncWriteExt, FutureExt as _, select_biased};
use gpui::{AppContext as _, AsyncApp, BackgroundExecutor, SharedString, Task};
use parking_lot::Mutex;
use rope::Rope;
use schemars::JsonSchema;
use serde::Deserialize;
use smallvec::SmallVec;
use smol::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use text::LineEnding;

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::sync::atomic::AtomicBool;

use std::process::{ExitStatus, Output};
use std::str::FromStr;
use std::time::SystemTime;
use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
    sync::Arc,
};
use sum_tree::MapSeekTarget;
use thiserror::Error;
use util::command::{Stdio, new_command};
use util::paths::PathStyle;
use util::rel_path::RelPath;
use util::{ResultExt, paths};
use uuid::Uuid;

#[path = "repository/api.rs"]
mod api;
mod commit_data;
mod exclude_override;
mod git_binary;
mod log_options;
mod parsers;
mod repo_path;
#[path = "repository/repository_impl_checkpoints.rs"]
mod repository_impl_checkpoints;
#[path = "repository/repository_impl_commit.rs"]
mod repository_impl_commit;
#[path = "repository/repository_impl_content.rs"]
mod repository_impl_content;
#[path = "repository/repository_impl_history.rs"]
mod repository_impl_history;
#[path = "repository/repository_impl_refs.rs"]
mod repository_impl_refs;
#[path = "repository/repository_impl_remote.rs"]
mod repository_impl_remote;
#[path = "repository/repository_impl_remotes.rs"]
mod repository_impl_remotes;
#[path = "repository/repository_impl_staging.rs"]
mod repository_impl_staging;
#[path = "repository/repository_impl_status.rs"]
mod repository_impl_status;
#[path = "repository/repository_impl_worktree_refs.rs"]
mod repository_impl_worktree_refs;
#[path = "repository/repository_impl_worktrees.rs"]
mod repository_impl_worktrees;
#[path = "repository/trait_impl.rs"]
mod trait_impl;
mod types;
mod worktree;

pub use api::*;
pub use askpass::{AskPassDelegate, AskPassResult, AskPassSession};
pub use commit_data::{CommitData, CommitDataReader, InitialGraphCommitData};
use commit_data::{CommitDataRequest, parse_cat_file_commit};
pub use exclude_override::GitExcludeOverride;
pub(crate) use git_binary::GitBinary;
use git_binary::{GitBinaryCommandError, run_git_command};
pub use log_options::{
    LogOrder, LogSource, SearchCommitArgs, commit_hash_search_query, delete_branch_flag,
};
use parsers::{
    checkpoint_author_envs, exclude_files, format_branch_scan_error, git_status_args,
    parse_branch_input, parse_file_history_changed_files_output, parse_initial_graph_output,
};
#[cfg(any(test, feature = "test-support"))]
pub use repo_path::repo_path;
pub use repo_path::{RepoPath, RepoPathDescendants};
pub use types::*;
pub use worktree::{
    CreateWorktreeTarget, Worktree, original_repo_path_from_common_dir, parse_worktrees_from_str,
};
use worktree::{linked_worktree_git_dir, normalize_git_metadata_path};

impl RealGitRepository {
    pub fn new(
        dotgit_path: &Path,
        bundled_git_binary_path: Option<PathBuf>,
        system_git_binary_path: Option<PathBuf>,
        executor: BackgroundExecutor,
    ) -> Result<Self> {
        let any_git_binary_path = system_git_binary_path
            .clone()
            .or(bundled_git_binary_path)
            .context("no git binary available")?;
        log::info!(
            "opening git repository at {dotgit_path:?} using git binary {any_git_binary_path:?}"
        );
        let dotgit_parent = dotgit_path.parent().context(".git has no parent")?;
        let has_working_directory =
            dotgit_path.is_file() || dotgit_path.file_name() == Some(OsStr::new(".git"));
        let working_directory = if has_working_directory {
            Some(normalize_git_metadata_path(dotgit_parent.to_path_buf())?)
        } else {
            None
        };

        let git_dir = if dotgit_path.is_file() {
            let content =
                std::fs::read_to_string(dotgit_path).context("reading .git worktree file")?;
            let path_str = content
                .strip_prefix("gitdir: ")
                .context("expected .git file to start with 'gitdir: '")?
                .trim();
            let resolved = PathBuf::from(path_str);
            let resolved = if resolved.is_absolute() {
                resolved
            } else {
                dotgit_parent.join(resolved)
            };
            normalize_git_metadata_path(resolved)?
        } else {
            normalize_git_metadata_path(dotgit_path.to_path_buf())?
        };

        let common_dir = {
            let commondir_file = git_dir.join("commondir");
            if commondir_file.is_file() {
                let content =
                    std::fs::read_to_string(&commondir_file).context("reading commondir file")?;
                let path_str = content.trim();
                let resolved = PathBuf::from(path_str);
                let resolved = if resolved.is_absolute() {
                    resolved
                } else {
                    git_dir.join(resolved)
                };
                normalize_git_metadata_path(resolved)?
            } else {
                git_dir.clone()
            }
        };

        Ok(Self {
            git_dir,
            common_dir,
            working_directory,
            system_git_binary_path,
            any_git_binary_path,
            executor,
            any_git_binary_help_output: Arc::new(Mutex::new(None)),
            is_trusted: Arc::new(AtomicBool::new(false)),
        })
    }

    fn working_directory(&self) -> Result<PathBuf> {
        self.working_directory
            .clone()
            .context("bare repositories do not have a working directory")
    }

    fn command_directory(&self) -> PathBuf {
        self.working_directory
            .clone()
            .unwrap_or_else(|| self.git_dir.clone())
    }

    fn git_binary_in_worktree(&self) -> Result<GitBinary> {
        Ok(GitBinary::new(
            self.any_git_binary_path.clone(),
            self.working_directory()?,
            self.path(),
            self.executor.clone(),
            self.is_trusted(),
        ))
    }

    fn git_binary(&self) -> GitBinary {
        GitBinary::new(
            self.any_git_binary_path.clone(),
            self.command_directory(),
            self.path(),
            self.executor.clone(),
            self.is_trusted(),
        )
    }

    fn edit_ref(&self, edit: RefEdit) -> BoxFuture<'_, Result<()>> {
        let git_binary = self.git_binary();
        self.executor
            .spawn(async move {
                let git = git_binary;
                let args = edit.into_args();
                git.run(&args).await?;
                Ok(())
            })
            .boxed()
    }

    async fn any_git_binary_help_output(&self) -> SharedString {
        if let Some(output) = self.any_git_binary_help_output.lock().clone() {
            return output;
        }
        let git = self.git_binary();
        let output: SharedString = self
            .executor
            .spawn(async move { git.run(&["help", "-a"]).await })
            .await
            .unwrap_or_default()
            .into();
        *self.any_git_binary_help_output.lock() = Some(output.clone());
        output
    }
}

#[derive(Clone, Debug)]
pub struct GitRepositoryCheckpoint {
    pub commit_sha: Oid,
}

#[derive(Debug)]
pub struct GitCommitter {
    pub name: Option<String>,
    pub email: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GitCommitTemplate {
    pub template: String,
}

pub async fn get_git_committer(cx: &AsyncApp) -> GitCommitter {
    if cfg!(any(feature = "test-support", test)) {
        return GitCommitter {
            name: None,
            email: None,
        };
    }

    let git_binary_path =
        if cfg!(target_os = "macos") && option_env!("MAV_BUNDLE").as_deref() == Some("true") {
            cx.update(|cx| {
                cx.path_for_auxiliary_executable("git")
                    .context("could not find git binary path")
                    .log_err()
            })
        } else {
            None
        };

    let git = GitBinary::new(
        git_binary_path.unwrap_or(PathBuf::from("git")),
        paths::home_dir().clone(),
        paths::home_dir().join(".git"),
        cx.background_executor().clone(),
        true,
    );

    cx.background_spawn(async move {
        let name = git
            .run(&["config", "--global", "user.name"])
            .await
            .log_err();
        let email = git
            .run(&["config", "--global", "user.email"])
            .await
            .log_err();
        GitCommitter { name, email }
    })
    .await
}

async fn run_commit_data_reader(
    git: GitBinary,
    request_rx: async_channel::Receiver<CommitDataRequest>,
) -> Result<()> {
    let mut process = git
        .build_command(&["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("starting git cat-file --batch process")?;

    let mut stdin = BufWriter::new(process.stdin.take().context("no stdin")?);
    let mut stdout = BufReader::new(process.stdout.take().context("no stdout")?);

    const MAX_BATCH_SIZE: usize = 64;

    while let Ok(first_request) = request_rx.recv().await {
        let mut pending_requests = vec![first_request];

        while pending_requests.len() < MAX_BATCH_SIZE {
            match request_rx.try_recv() {
                Ok(request) => pending_requests.push(request),
                Err(_) => break,
            }
        }

        for request in &pending_requests {
            stdin.write_all(request.sha.to_string().as_bytes()).await?;
            stdin.write_all(b"\n").await?;
        }
        stdin.flush().await?;

        for request in pending_requests {
            let result = read_single_commit_response(&mut stdout, &request.sha).await;
            request.response_tx.send(result).ok();
        }
    }

    drop(stdin);
    process.kill().ok();

    Ok(())
}

async fn read_single_commit_response<R: smol::io::AsyncBufRead + Unpin>(
    stdout: &mut R,
    sha: &Oid,
) -> Result<CommitData> {
    let mut header_bytes = Vec::new();
    stdout.read_until(b'\n', &mut header_bytes).await?;
    let header_line = String::from_utf8_lossy(&header_bytes);

    let parts: Vec<&str> = header_line.trim().split(' ').collect();
    if parts.len() < 3 {
        bail!("invalid cat-file header: {header_line}");
    }

    let object_type = parts[1];
    if object_type == "missing" {
        bail!("object not found: {}", sha);
    }

    if object_type != "commit" {
        bail!("expected commit object, got {object_type}");
    }

    let size: usize = parts[2]
        .parse()
        .with_context(|| format!("invalid object size: {}", parts[2]))?;

    let mut content = vec![0u8; size];
    stdout.read_exact(&mut content).await?;

    let mut newline = [0u8; 1];
    stdout.read_exact(&mut newline).await?;

    let content_str = String::from_utf8_lossy(&content);
    parse_cat_file_commit(*sha, &content_str)
        .ok_or_else(|| anyhow!("failed to parse commit {}", sha))
}

#[cfg(test)]
mod tests;
