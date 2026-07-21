use super::*;

/// If `path` is a git linked worktree checkout, resolves it to the main
/// repository's identity path. For regular linked worktrees this is the main
/// repository's working directory; for linked worktrees backed by a bare repo
/// such as `.bare`, this is the parent project directory users think of as the
/// repository root. Returns `None` if `path` is a normal repository, not a git
/// repo, or if resolution fails.
///
/// Resolution works by:
/// 1. Reading the `.git` file to get the `gitdir:` pointer
/// 2. Following that to the worktree-specific git directory
/// 3. Reading the `commondir` file to find the shared `.git` directory
/// 4. Deriving the main repo's identity path from the common dir
pub async fn resolve_git_worktree_to_main_repo(fs: &dyn Fs, path: &Path) -> Option<PathBuf> {
    let dot_git = path.join(".git");
    let metadata = fs.metadata(&dot_git).await.ok()??;
    if metadata.is_dir {
        return None; // Normal repo, not a linked worktree
    }
    // It's a .git file - parse the gitdir: pointer.
    let content = fs.load(&dot_git).await.ok()?;
    let gitdir_rel = content.strip_prefix("gitdir:")?.trim();
    let gitdir_abs = fs.canonicalize(&path.join(gitdir_rel)).await.ok()?;
    // Read commondir to find the main .git directory.
    let commondir_content = fs.load(&gitdir_abs.join("commondir")).await.ok()?;
    let common_dir = fs
        .canonicalize(&gitdir_abs.join(commondir_content.trim()))
        .await
        .ok()?;
    Some(repo_identity_path(&common_dir).to_path_buf())
}

/// Validates that the resolved worktree directory is acceptable:
/// - The setting must not be an absolute path.
/// - The resolved path must be either a subdirectory of the working
///   directory or a subdirectory of its parent (i.e., a sibling).
///
/// Returns `Ok(resolved_path)` or an error with a user-facing message.
pub fn worktrees_directory_for_repo(
    repository_anchor_path: &Path,
    worktree_directory_setting: &str,
    path_style: PathStyle,
) -> Result<PathBuf> {
    // Check the original setting before trimming, since a path like "///"
    // is absolute but becomes "" after stripping trailing separators.
    // Also check for leading `/` or `\` explicitly, because on Windows
    // `Path::is_absolute()` requires a drive letter - so `/tmp/worktrees`
    // would slip through even though it's clearly not a relative path.
    if path_style.is_absolute(worktree_directory_setting)
        || worktree_directory_setting.starts_with('\\')
    {
        anyhow::bail!(
            "git.worktree_directory must be a relative path, got: {worktree_directory_setting:?}"
        );
    }

    if worktree_directory_setting.is_empty() {
        anyhow::bail!("git.worktree_directory must not be empty");
    }

    let trimmed = worktree_directory_setting.trim_end_matches(['/', '\\']);
    if trimmed == ".." {
        anyhow::bail!("git.worktree_directory must not be \"..\" (use \"../some-name\" instead)");
    }

    let joined = path_style.join_path(repository_anchor_path, trimmed)?;
    let resolved = if path_style.is_posix() {
        joined
    } else {
        util::normalize_path(&joined)
    };
    let resolved = if resolved.starts_with(repository_anchor_path) {
        resolved
    } else if let Some(repo_dir_name) = repository_anchor_path
        .file_name()
        .and_then(|name| name.to_str())
    {
        path_style.join_path(&resolved, repo_dir_name)?
    } else {
        resolved
    };

    let parent = repository_anchor_path
        .parent()
        .unwrap_or(repository_anchor_path);

    if !resolved.starts_with(parent) {
        anyhow::bail!(
            "git.worktree_directory resolved to {resolved:?}, which is outside \
             the project root and its parent directory. It must resolve to a \
             subdirectory of {repository_anchor_path:?} or a sibling of it."
        );
    }

    Ok(resolved)
}

pub(super) async fn remove_empty_managed_worktree_ancestors(
    fs: &dyn Fs,
    child_path: &Path,
    base_path: &Path,
) {
    let mut current = child_path;
    while let Some(parent) = current.parent() {
        if parent == base_path {
            break;
        }
        if !parent.starts_with(base_path) {
            break;
        }

        let result = fs
            .remove_dir(
                parent,
                RemoveOptions {
                    recursive: false,
                    ignore_if_not_exists: true,
                },
            )
            .await;

        match result {
            Ok(()) => {
                log::info!(
                    "Removed empty managed worktree directory: {}",
                    parent.display()
                );
            }
            Err(error) => {
                log::debug!(
                    "Stopped removing managed worktree parent directories at {}: {error}",
                    parent.display()
                );
                break;
            }
        }

        current = parent;
    }
}

/// Returns the repository's identity path given its common Git directory.
///
/// This is the canonical, on-disk path used for project grouping and as the
/// basis for display names. The goal is to return the directory the user
/// thinks of as "the project":
///
/// - If `common_dir`'s last component starts with `.` (e.g. `.git` for a
///   normal checkout, or `.bare` for a bare clone), the parent directory is
///   returned. Both of these are internal Git directories; the parent is the
///   meaningful project root.
/// - Otherwise (e.g. `mav.git` for a bare clone), `common_dir` itself is
///   returned - it is already a meaningful on-disk path.
pub fn repo_identity_path(common_dir: &Path) -> &Path {
    let is_dot_entry = common_dir
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'));
    if is_dot_entry {
        common_dir.parent().unwrap_or(common_dir)
    } else {
        common_dir
    }
}

/// Returns a short name for a linked worktree suitable for UI display
///
/// Uses the main worktree path to come up with a short name that disambiguates
/// the linked worktree from the main worktree.
pub fn linked_worktree_short_name(
    main_worktree_path: &Path,
    linked_worktree_path: &Path,
) -> Option<SharedString> {
    if main_worktree_path == linked_worktree_path {
        return None;
    }

    let project_name = main_worktree_path.file_name()?.to_str()?;
    let directory_name = linked_worktree_path.file_name()?.to_str()?;
    let name = if directory_name != project_name {
        directory_name.to_string()
    } else {
        linked_worktree_path
            .parent()?
            .file_name()?
            .to_str()?
            .to_string()
    };
    Some(name.into())
}

pub(super) fn get_permalink_in_rust_registry_src(
    provider_registry: Arc<GitHostingProviderRegistry>,
    path: PathBuf,
    selection: Range<u32>,
) -> Result<url::Url> {
    #[derive(Deserialize)]
    struct CargoVcsGit {
        sha1: String,
    }

    #[derive(Deserialize)]
    struct CargoVcsInfo {
        git: CargoVcsGit,
        path_in_vcs: String,
    }

    #[derive(Deserialize)]
    struct CargoPackage {
        repository: String,
    }

    #[derive(Deserialize)]
    struct CargoToml {
        package: CargoPackage,
    }

    let Some((dir, cargo_vcs_info_json)) = path.ancestors().skip(1).find_map(|dir| {
        let json = std::fs::read_to_string(dir.join(".cargo_vcs_info.json")).ok()?;
        Some((dir, json))
    }) else {
        bail!("No .cargo_vcs_info.json found in parent directories")
    };
    let cargo_vcs_info = serde_json::from_str::<CargoVcsInfo>(&cargo_vcs_info_json)?;
    let cargo_toml = std::fs::read_to_string(dir.join("Cargo.toml"))?;
    let manifest = toml::from_str::<CargoToml>(&cargo_toml)?;
    let (provider, remote) = parse_git_remote_url(provider_registry, &manifest.package.repository)
        .context("parsing package.repository field of manifest")?;
    let path = PathBuf::from(cargo_vcs_info.path_in_vcs).join(path.strip_prefix(dir).unwrap());
    let permalink = provider.build_permalink(
        remote,
        BuildPermalinkParams::new(
            &cargo_vcs_info.git.sha1,
            &RepoPath::from_rel_path(
                &RelPath::new(&path, PathStyle::local()).context("invalid path")?,
            ),
            Some(selection),
        ),
    );
    Ok(permalink)
}
