use crate::SHORT_SHA_LENGTH;
use anyhow::{Context as _, Result, anyhow};
use gpui::SharedString;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};
use util::paths;

/// Given the git common directory (from `commondir()`), derive the original
/// repository's working directory.
///
/// For a standard checkout, `common_dir` is `<work_dir>/.git`, so the parent
/// is the working directory. For a git worktree, `common_dir` is the **main**
/// repo's `.git` directory, so the parent is the original repo's working directory.
///
/// Returns `None` if `common_dir` doesn't end with `.git` (e.g. bare repos),
/// because there is no working-tree root to resolve to in that case.
pub fn original_repo_path_from_common_dir(common_dir: &Path) -> Option<PathBuf> {
    if common_dir.file_name() == Some(OsStr::new(".git")) {
        common_dir.parent().map(|p| p.to_path_buf())
    } else {
        None
    }
}

pub(super) fn linked_worktree_git_dir(worktree_path: &Path) -> Result<PathBuf> {
    let dot_git_path = worktree_path.join(".git");
    let git_file = std::fs::read_to_string(&dot_git_path)
        .with_context(|| format!("failed to read {}", dot_git_path.display()))?;
    let git_dir = git_file
        .strip_prefix("gitdir:")
        .context("worktree .git file missing gitdir pointer")?
        .trim();
    Ok(worktree_path.join(git_dir))
}

pub(super) fn normalize_git_metadata_path(path: PathBuf) -> Result<PathBuf> {
    paths::normalize_lexically(&path)
        .map_err(|_| anyhow!("git metadata path escapes its filesystem root: {path:?}"))
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub ref_name: Option<SharedString>,
    // todo(git_worktree) This type should be a Oid
    pub sha: SharedString,
    pub is_main: bool,
    pub is_bare: bool,
}

/// Describes how a new worktree should choose or create its checked-out HEAD.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum CreateWorktreeTarget {
    /// Check out an existing local branch in the new worktree.
    ExistingBranch {
        /// The existing local branch to check out.
        branch_name: String,
    },
    /// Create a new local branch for the new worktree.
    NewBranch {
        /// The new local branch to create and check out.
        branch_name: String,
        /// The commit or ref to create the branch from. Uses `HEAD` when `None`.
        base_sha: Option<String>,
    },
    /// Check out a commit or ref in detached HEAD state.
    Detached {
        /// The commit or ref to check out. Uses `HEAD` when `None`.
        base_sha: Option<String>,
    },
}

impl CreateWorktreeTarget {
    pub fn branch_name(&self) -> Option<&str> {
        match self {
            Self::ExistingBranch { branch_name } | Self::NewBranch { branch_name, .. } => {
                Some(branch_name)
            }
            Self::Detached { .. } => None,
        }
    }
}

impl Worktree {
    /// Returns the branch name if the worktree is attached to a branch.
    pub fn branch_name(&self) -> Option<&str> {
        self.ref_name.as_ref().map(|ref_name| {
            ref_name
                .strip_prefix("refs/heads/")
                .or_else(|| ref_name.strip_prefix("refs/remotes/"))
                .unwrap_or(ref_name)
        })
    }

    /// Returns a display name for the worktree, suitable for use in the UI.
    ///
    /// If the worktree is attached to a branch, returns the branch name.
    /// Otherwise, returns the short SHA of the worktree's HEAD commit.
    pub fn display_name(&self) -> &str {
        self.branch_name()
            .unwrap_or(&self.sha[..self.sha.len().min(SHORT_SHA_LENGTH)])
    }

    pub fn directory_name(&self, main_worktree_path: Option<&Path>) -> String {
        if self.is_main {
            return "main worktree".to_string();
        }

        let dir_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(self.display_name());

        if let Some(main_path) = main_worktree_path {
            let main_dir = main_path.file_name().and_then(|n| n.to_str());
            if main_dir == Some(dir_name) {
                if let Some(parent_name) = self
                    .path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                {
                    return parent_name.to_string();
                }
            }
        }

        dir_name.to_string()
    }
}

pub fn parse_worktrees_from_str<T: AsRef<str>>(
    raw_worktrees: T,
    main_worktree_path: Option<&Path>,
) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let normalized = raw_worktrees.as_ref().replace("\r\n", "\n");
    let entries = normalized.split("\n\n");
    for entry in entries {
        let mut path = None;
        let mut sha = None;
        let mut ref_name = None;

        let mut is_bare = false;

        for line in entry.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix("worktree ") {
                path = Some(rest.to_string());
            } else if let Some(rest) = line.strip_prefix("HEAD ") {
                sha = Some(rest.to_string());
            } else if let Some(rest) = line.strip_prefix("branch ") {
                ref_name = Some(rest.to_string());
            } else if line == "bare" {
                is_bare = true;
            }
            // Ignore other lines: detached, locked, prunable, etc.
        }

        if let (Some(path), Some(sha)) = (path, sha) {
            let path = PathBuf::from(path);
            let is_main =
                main_worktree_path.is_some_and(|main_worktree_path| path == main_worktree_path);
            worktrees.push(Worktree {
                path,
                ref_name: ref_name.map(Into::into),
                sha: sha.into(),
                is_main,
                is_bare,
            });
        }
    }

    worktrees
}
