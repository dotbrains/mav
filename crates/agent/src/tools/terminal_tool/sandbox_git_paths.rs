use fs::Fs;
use gpui::App;
use metadata::{
    gitdir_belongs_to_submodule_worktree, linked_worktree_points_back, read_commondir_path,
    read_gitfile_path,
};
use project::Project;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use utils::{normalize_sandbox_git_path, path_is_within_any};

#[path = "sandbox_git_paths/metadata.rs"]
mod metadata;
#[path = "sandbox_git_paths/utils.rs"]
mod utils;

#[cfg(test)]
use utils::parse_core_worktree;

#[derive(Default)]
pub(crate) struct SandboxGitPathCandidates {
    pub(crate) writable_paths: Vec<PathBuf>,
    pub(crate) git_paths: Vec<PathBuf>,
    repositories: Vec<SandboxGitRepositoryPaths>,
}

struct SandboxGitRepositoryPaths {
    work_directory_abs_path: PathBuf,
    dot_git_abs_path: PathBuf,
    repository_dir_abs_path: PathBuf,
    common_dir_abs_path: PathBuf,
}

pub(crate) struct SandboxGitPaths {
    pub(crate) writable_paths: Vec<PathBuf>,
    pub(crate) git_dirs: Vec<PathBuf>,
    pub(crate) allow_git_access: bool,
}

impl SandboxGitPathCandidates {
    pub(crate) fn cache_key_repositories(&self) -> Vec<(PathBuf, PathBuf, PathBuf, PathBuf)> {
        let mut repositories = self
            .repositories
            .iter()
            .map(|repository| {
                (
                    repository.work_directory_abs_path.clone(),
                    repository.dot_git_abs_path.clone(),
                    repository.repository_dir_abs_path.clone(),
                    repository.common_dir_abs_path.clone(),
                )
            })
            .collect::<Vec<_>>();
        repositories.sort();
        repositories
    }

    pub(crate) fn from_project(project: &Project, cx: &App) -> Self {
        let mut candidates = Self::default();

        for worktree in project.worktrees(cx) {
            let worktree = worktree.read(cx);
            let worktree_abs_path = worktree.abs_path();
            candidates
                .writable_paths
                .push(worktree_abs_path.to_path_buf());
            // Protect `<worktree>/.git` even when it doesn't exist yet, so a command
            // can't `git init` and then write to the freshly created metadata.
            candidates.git_paths.push(worktree_abs_path.join(".git"));

            // `Worktree` derefs to `Snapshot`; read the field directly instead of
            // cloning the whole snapshot just for this path.
            if let Some(root_repo_common_dir) = worktree.root_repo_common_dir() {
                candidates
                    .git_paths
                    .push(root_repo_common_dir.to_path_buf());
            }
        }

        // `Repository` derefs to `RepositorySnapshot`, so read the few path fields
        // directly rather than cloning the entire snapshot (which carries the
        // per-path status tree) for each repository.
        for repository in project.git_store().read(cx).repositories().values() {
            let repository = repository.read(cx);
            let repository_paths = SandboxGitRepositoryPaths {
                work_directory_abs_path: repository.work_directory_abs_path.to_path_buf(),
                dot_git_abs_path: repository.dot_git_abs_path.to_path_buf(),
                repository_dir_abs_path: repository.repository_dir_abs_path.to_path_buf(),
                common_dir_abs_path: repository.common_dir_abs_path.to_path_buf(),
            };
            candidates
                .git_paths
                .push(repository_paths.dot_git_abs_path.clone());
            candidates
                .git_paths
                .push(repository_paths.repository_dir_abs_path.clone());
            candidates
                .git_paths
                .push(repository_paths.common_dir_abs_path.clone());
            candidates.repositories.push(repository_paths);
        }

        candidates.git_paths.sort();
        candidates.git_paths.dedup();
        candidates.writable_paths.sort();
        candidates.writable_paths.dedup();

        candidates
    }
}

pub(crate) async fn sandbox_git_paths(
    candidates: SandboxGitPathCandidates,
    fs: &dyn Fs,
    allow_git_access: bool,
) -> SandboxGitPaths {
    let mut writable_paths = candidates.writable_paths;
    let mut git_dirs = candidates.git_paths;

    let mut allow_verified_git_access = false;
    if allow_git_access {
        let mut verified_git_paths = Vec::new();
        for repository in candidates.repositories {
            verified_git_paths.extend(verified_sandbox_git_paths(repository, fs).await);
        }
        verified_git_paths.sort();
        verified_git_paths.dedup();

        // A Git path inside a writable worktree root is already writable, so
        // granting it can never escalate access beyond the project. A non-repo
        // `.git` placeholder there (a plain folder opened alongside a repo, or a
        // not-yet-initialized repo) would never appear in `verified_git_paths`,
        // so requiring it to verify would wrongly deny the whole grant. Only
        // paths that fall *outside* every writable root can leak access to
        // unrelated metadata, so those are the only ones that must verify.
        let mut all_external_git_paths_verified = true;
        for path in &git_dirs {
            if path_is_within_any(path, &writable_paths) {
                continue;
            }
            let Some(normalized_path) = normalize_sandbox_git_path(path, fs).await else {
                log::warn!(
                    "Denying requested agent terminal Git metadata access because external Git metadata path `{}` could not be normalized",
                    path.display()
                );
                all_external_git_paths_verified = false;
                break;
            };
            if verified_git_paths.binary_search(&normalized_path).is_err() {
                log::warn!(
                    "Denying requested agent terminal Git metadata access because external Git metadata path `{}` (normalized to `{}`) was not verified from project repository metadata",
                    path.display(),
                    normalized_path.display()
                );
                all_external_git_paths_verified = false;
                break;
            }
        }

        // The current sandbox policy can make one Git directory set either all
        // writable or all protected. Only grant Git access when every external
        // candidate still verifies; otherwise keep protecting the original
        // candidate set. The granted set is the verified paths only, so even
        // when the grant proceeds, unverified `.git` metadata never becomes
        // writable.
        if all_external_git_paths_verified {
            git_dirs = verified_git_paths;
            allow_verified_git_access = true;
        }
    }

    git_dirs.sort();
    git_dirs.dedup();
    writable_paths.sort();
    writable_paths.dedup();

    SandboxGitPaths {
        writable_paths,
        git_dirs,
        allow_git_access: allow_verified_git_access,
    }
}

async fn verified_sandbox_git_paths(
    repository: SandboxGitRepositoryPaths,
    fs: &dyn Fs,
) -> Vec<PathBuf> {
    macro_rules! deny {
        ($($arg:tt)*) => {{
            log::debug!(
                "Denying agent terminal Git metadata access for repository `{}` (dot_git: `{}`, repository_dir: `{}`, common_dir: `{}`): {}",
                repository.work_directory_abs_path.display(),
                repository.dot_git_abs_path.display(),
                repository.repository_dir_abs_path.display(),
                repository.common_dir_abs_path.display(),
                format_args!($($arg)*)
            );
            return Vec::new();
        }};
    }

    let Some(dot_git_abs_path) = normalize_sandbox_git_path(&repository.dot_git_abs_path, fs).await
    else {
        deny!(
            "could not normalize .git path `{}`",
            repository.dot_git_abs_path.display()
        );
    };
    let Some(repository_dir_abs_path) =
        normalize_sandbox_git_path(&repository.repository_dir_abs_path, fs).await
    else {
        deny!(
            "could not normalize repository dir `{}`",
            repository.repository_dir_abs_path.display()
        );
    };
    let Some(common_dir_abs_path) =
        normalize_sandbox_git_path(&repository.common_dir_abs_path, fs).await
    else {
        deny!(
            "could not normalize common dir `{}`",
            repository.common_dir_abs_path.display()
        );
    };

    let dot_git_metadata = match fs.metadata(&repository.dot_git_abs_path).await {
        Ok(Some(metadata)) => metadata,
        Ok(None) => deny!(
            ".git path `{}` does not exist",
            repository.dot_git_abs_path.display()
        ),
        Err(error) => deny!(
            "failed to read metadata for .git path `{}`: {error}",
            repository.dot_git_abs_path.display()
        ),
    };
    if dot_git_metadata.is_symlink {
        deny!(
            ".git path `{}` is a symlink",
            repository.dot_git_abs_path.display()
        );
    }

    if dot_git_metadata.is_dir {
        if dot_git_abs_path != repository_dir_abs_path {
            deny!(
                "directory .git path `{}` normalized to `{}`, which does not match repository dir `{}` normalized to `{}`",
                repository.dot_git_abs_path.display(),
                dot_git_abs_path.display(),
                repository.repository_dir_abs_path.display(),
                repository_dir_abs_path.display()
            );
        }

        if repository_dir_abs_path == common_dir_abs_path {
            return vec![
                dot_git_abs_path,
                repository_dir_abs_path,
                common_dir_abs_path,
            ];
        }

        let Some(common_dir) = read_commondir_path(&repository_dir_abs_path, fs).await else {
            deny!(
                "repository dir `{}` did not contain a readable commondir pointing at expected common dir `{}`",
                repository_dir_abs_path.display(),
                common_dir_abs_path.display()
            );
        };
        if common_dir == common_dir_abs_path {
            return vec![
                dot_git_abs_path,
                repository_dir_abs_path,
                common_dir_abs_path,
            ];
        }
        deny!(
            "repository dir `{}` commondir resolved to `{}`, expected `{}`",
            repository_dir_abs_path.display(),
            common_dir.display(),
            common_dir_abs_path.display()
        );
    }

    let Some(expected_dot_git_abs_path) =
        normalize_sandbox_git_path(repository.work_directory_abs_path.join(".git"), fs).await
    else {
        deny!(
            "could not normalize expected worktree .git path `{}`",
            repository.work_directory_abs_path.join(".git").display()
        );
    };
    if dot_git_abs_path != expected_dot_git_abs_path {
        deny!(
            ".git path `{}` normalized to `{}`, expected worktree .git path `{}`",
            repository.dot_git_abs_path.display(),
            dot_git_abs_path.display(),
            expected_dot_git_abs_path.display()
        );
    }

    let Some(stated_repository_dir) = read_gitfile_path(&repository.dot_git_abs_path, fs).await
    else {
        deny!(
            "gitfile `{}` did not resolve to a readable, non-symlink repository dir",
            repository.dot_git_abs_path.display()
        );
    };

    if stated_repository_dir != repository_dir_abs_path {
        deny!(
            "gitfile `{}` resolved to repository dir `{}`, expected `{}`",
            repository.dot_git_abs_path.display(),
            stated_repository_dir.display(),
            repository_dir_abs_path.display()
        );
    }

    let Some(common_dir) = read_commondir_path(&stated_repository_dir, fs).await else {
        if repository_dir_abs_path == common_dir_abs_path
            && gitdir_belongs_to_submodule_worktree(
                &repository_dir_abs_path,
                &repository.work_directory_abs_path,
                fs,
            )
            .await
        {
            return vec![dot_git_abs_path, repository_dir_abs_path];
        }
        deny!(
            "repository dir `{}` has no verified commondir and did not verify as a submodule gitdir for worktree `{}`",
            repository_dir_abs_path.display(),
            repository.work_directory_abs_path.display()
        );
    };

    if common_dir != common_dir_abs_path {
        deny!(
            "repository dir `{}` commondir resolved to `{}`, expected `{}`",
            stated_repository_dir.display(),
            common_dir.display(),
            common_dir_abs_path.display()
        );
    }

    if repository_dir_abs_path != common_dir_abs_path
        && !linked_worktree_points_back(
            &common_dir_abs_path,
            &repository_dir_abs_path,
            &dot_git_abs_path,
            &repository.work_directory_abs_path,
            fs,
        )
        .await
    {
        deny!(
            "linked worktree repository dir `{}` did not point back to .git path `{}` and worktree `{}` under common dir `{}`",
            repository_dir_abs_path.display(),
            dot_git_abs_path.display(),
            repository.work_directory_abs_path.display(),
            common_dir_abs_path.display()
        );
    }

    vec![
        dot_git_abs_path,
        repository_dir_abs_path,
        common_dir_abs_path,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs::Fs;

    mod basic_tests {
        use super::*;

        include!("../terminal_tool_tests/sandbox_git_paths/basic.rs");
    }

    mod denial_tests {
        use super::*;

        include!("../terminal_tool_tests/sandbox_git_paths/denials.rs");
    }

    mod parse_tests {
        use super::*;

        include!("../terminal_tool_tests/sandbox_git_paths/parse.rs");
    }
}
