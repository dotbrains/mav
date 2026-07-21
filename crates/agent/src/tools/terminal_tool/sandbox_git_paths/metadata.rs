use super::utils::{normalize_sandbox_git_path, parse_core_worktree};
use fs::Fs;
use std::path::{Path, PathBuf};

pub(super) async fn read_gitfile_path(dot_git_abs_path: &Path, fs: &dyn Fs) -> Option<PathBuf> {
    let contents = match fs.load(dot_git_abs_path).await {
        Ok(contents) => contents,
        Err(error) => {
            log::debug!(
                "Could not verify Git metadata path: failed to read gitfile `{}`: {error}",
                dot_git_abs_path.display()
            );
            return None;
        }
    };
    let Some(gitdir) = contents.strip_prefix("gitdir:") else {
        log::debug!(
            "Could not verify Git metadata path: gitfile `{}` does not start with `gitdir:`",
            dot_git_abs_path.display()
        );
        return None;
    };
    let gitdir = Path::new(gitdir.trim());
    let Some(dot_git_parent) = dot_git_abs_path.parent() else {
        log::debug!(
            "Could not verify Git metadata path: gitfile `{}` has no parent directory",
            dot_git_abs_path.display()
        );
        return None;
    };
    let path = if gitdir.is_absolute() {
        gitdir.to_path_buf()
    } else {
        dot_git_parent.join(gitdir)
    };
    match fs.metadata(&path).await {
        Ok(Some(metadata)) if metadata.is_symlink => {
            log::debug!(
                "Could not verify Git metadata path: gitfile `{}` points to symlinked gitdir `{}`",
                dot_git_abs_path.display(),
                path.display()
            );
            return None;
        }
        Ok(_) => {}
        Err(error) => {
            log::debug!(
                "Could not check whether gitfile `{}` points to a symlink at `{}`: {error}",
                dot_git_abs_path.display(),
                path.display()
            );
        }
    }
    let normalized_path = normalize_sandbox_git_path(&path, fs).await;
    if normalized_path.is_none() {
        log::debug!(
            "Could not verify Git metadata path: gitfile `{}` points to gitdir `{}` that could not be normalized",
            dot_git_abs_path.display(),
            path.display()
        );
    }
    normalized_path
}

pub(super) async fn read_commondir_path(
    repository_dir_abs_path: &Path,
    fs: &dyn Fs,
) -> Option<PathBuf> {
    let commondir_abs_path = repository_dir_abs_path.join("commondir");
    let commondir_contents = match fs.load(&commondir_abs_path).await {
        Ok(contents) => contents,
        Err(error) => {
            log::debug!(
                "Could not verify Git metadata path: failed to read commondir file `{}`: {error}",
                commondir_abs_path.display()
            );
            return None;
        }
    };
    let commondir_path = Path::new(commondir_contents.trim());
    let path = if commondir_path.is_absolute() {
        commondir_path.to_path_buf()
    } else {
        repository_dir_abs_path.join(commondir_path)
    };
    let normalized_path = normalize_sandbox_git_path(&path, fs).await;
    if normalized_path.is_none() {
        log::debug!(
            "Could not verify Git metadata path: commondir file `{}` points to `{}` which could not be normalized",
            commondir_abs_path.display(),
            path.display()
        );
    }
    normalized_path
}

pub(super) async fn linked_worktree_points_back(
    common_dir_abs_path: &Path,
    repository_dir_abs_path: &Path,
    dot_git_abs_path: &Path,
    work_directory_abs_path: &Path,
    fs: &dyn Fs,
) -> bool {
    let expected_repository_parent = common_dir_abs_path.join("worktrees");
    if repository_dir_abs_path.parent() != Some(expected_repository_parent.as_path()) {
        log::debug!(
            "Could not verify linked worktree Git metadata: repository dir `{}` is not under expected worktrees dir `{}`",
            repository_dir_abs_path.display(),
            expected_repository_parent.display()
        );
        return false;
    }

    match fs.metadata(repository_dir_abs_path).await {
        Ok(Some(metadata)) if metadata.is_dir && !metadata.is_symlink => {}
        Ok(Some(metadata)) => {
            log::debug!(
                "Could not verify linked worktree Git metadata: repository dir `{}` has invalid metadata (is_dir: {}, is_symlink: {})",
                repository_dir_abs_path.display(),
                metadata.is_dir,
                metadata.is_symlink
            );
            return false;
        }
        Ok(None) => {
            log::debug!(
                "Could not verify linked worktree Git metadata: repository dir `{}` does not exist",
                repository_dir_abs_path.display()
            );
            return false;
        }
        Err(error) => {
            log::debug!(
                "Could not verify linked worktree Git metadata: failed to read metadata for repository dir `{}`: {error}",
                repository_dir_abs_path.display()
            );
            return false;
        }
    }

    let expected_dot_git_abs_path = work_directory_abs_path.join(".git");
    let Some(expected_dot_git_abs_path) =
        normalize_sandbox_git_path(&expected_dot_git_abs_path, fs).await
    else {
        log::debug!(
            "Could not verify linked worktree Git metadata: expected .git path `{}` could not be normalized",
            expected_dot_git_abs_path.display()
        );
        return false;
    };
    if dot_git_abs_path != expected_dot_git_abs_path {
        log::debug!(
            "Could not verify linked worktree Git metadata: .git path `{}` does not match expected worktree .git path `{}`",
            dot_git_abs_path.display(),
            expected_dot_git_abs_path.display()
        );
        return false;
    }

    let Some(listed_dot_git_path) = read_listed_worktree_gitdir(repository_dir_abs_path, fs).await
    else {
        return false;
    };
    if listed_dot_git_path != dot_git_abs_path {
        log::debug!(
            "Could not verify linked worktree Git metadata: repository dir `{}` lists .git path `{}`, expected `{}`",
            repository_dir_abs_path.display(),
            listed_dot_git_path.display(),
            dot_git_abs_path.display()
        );
        return false;
    }

    true
}

async fn read_listed_worktree_gitdir(worktree_entry_dir: &Path, fs: &dyn Fs) -> Option<PathBuf> {
    let gitdir_abs_path = worktree_entry_dir.join("gitdir");
    let gitdir_contents = match fs.load(&gitdir_abs_path).await {
        Ok(contents) => contents,
        Err(error) => {
            log::debug!(
                "Could not verify linked worktree Git metadata: failed to read worktree gitdir file `{}`: {error}",
                gitdir_abs_path.display()
            );
            return None;
        }
    };
    let gitdir_path = Path::new(gitdir_contents.trim());
    let path = if gitdir_path.is_absolute() {
        gitdir_path.to_path_buf()
    } else {
        worktree_entry_dir.join(gitdir_path)
    };
    let normalized_path = normalize_sandbox_git_path(&path, fs).await;
    if normalized_path.is_none() {
        log::debug!(
            "Could not verify linked worktree Git metadata: worktree gitdir file `{}` points to `{}` which could not be normalized",
            gitdir_abs_path.display(),
            path.display()
        );
    }
    normalized_path
}

pub(super) async fn gitdir_belongs_to_submodule_worktree(
    repository_dir_abs_path: &Path,
    work_directory_abs_path: &Path,
    fs: &dyn Fs,
) -> bool {
    let Some(work_directory_abs_path) =
        normalize_sandbox_git_path(work_directory_abs_path, fs).await
    else {
        log::debug!(
            "Could not verify submodule Git metadata: worktree path `{}` could not be normalized",
            work_directory_abs_path.display()
        );
        return false;
    };

    let Some(core_worktree) = read_core_worktree(repository_dir_abs_path, fs).await else {
        return false;
    };
    if core_worktree != work_directory_abs_path {
        log::debug!(
            "Could not verify submodule Git metadata: repository dir `{}` has core.worktree `{}`, expected `{}`",
            repository_dir_abs_path.display(),
            core_worktree.display(),
            work_directory_abs_path.display()
        );
        return false;
    }

    true
}

async fn read_core_worktree(repository_dir_abs_path: &Path, fs: &dyn Fs) -> Option<PathBuf> {
    let config_abs_path = repository_dir_abs_path.join("config");
    let config = match fs.load(&config_abs_path).await {
        Ok(config) => config,
        Err(error) => {
            log::debug!(
                "Could not verify submodule Git metadata: failed to read config `{}`: {error}",
                config_abs_path.display()
            );
            return None;
        }
    };
    let Some(core_worktree) = parse_core_worktree(&config) else {
        log::debug!(
            "Could not verify submodule Git metadata: config `{}` did not contain exactly one supported core.worktree value",
            config_abs_path.display()
        );
        return None;
    };
    let path = Path::new(&core_worktree);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        repository_dir_abs_path.join(path)
    };
    let normalized_path = normalize_sandbox_git_path(&path, fs).await;
    if normalized_path.is_none() {
        log::debug!(
            "Could not verify submodule Git metadata: core.worktree value `{}` from config `{}` resolved to `{}` which could not be normalized",
            core_worktree,
            config_abs_path.display(),
            path.display()
        );
    }
    normalized_path
}
