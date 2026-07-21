use super::*;

fn parse_gitfile(content: &str) -> anyhow::Result<&Path> {
    let path = content
        .strip_prefix("gitdir:")
        .with_context(|| format!("parsing gitfile content {content:?}"))?;
    Ok(Path::new(path.trim()))
}

fn resolve_gitfile_path(dot_git_abs_path: &Path, gitfile_path: &Path) -> PathBuf {
    if gitfile_path.is_absolute() {
        gitfile_path.into()
    } else {
        dot_git_abs_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(gitfile_path)
    }
}

fn resolve_commondir_path(repository_dir_abs_path: &Path, commondir_path: &str) -> PathBuf {
    let commondir_path = Path::new(commondir_path.trim());
    if commondir_path.is_absolute() {
        commondir_path.into()
    } else {
        repository_dir_abs_path.join(commondir_path)
    }
}

pub async fn discover_root_repo_common_dir(root_abs_path: &Path, fs: &dyn Fs) -> Option<Arc<Path>> {
    let root_dot_git = root_abs_path.join(DOT_GIT);
    if !fs.metadata(&root_dot_git).await.is_ok_and(|m| m.is_some()) {
        return None;
    }
    let dot_git_path: Arc<Path> = root_dot_git.into();
    let (_, common_dir) = discover_git_paths(&dot_git_path, fs).await;
    Some(common_dir)
}

pub(super) async fn discover_git_paths(
    dot_git_abs_path: &Arc<Path>,
    fs: &dyn Fs,
) -> (Arc<Path>, Arc<Path>) {
    let mut repository_dir_abs_path = dot_git_abs_path.clone();
    let mut common_dir_abs_path = dot_git_abs_path.clone();

    if let Some(path) = fs
        .load(dot_git_abs_path)
        .await
        .ok()
        .as_ref()
        .and_then(|contents| parse_gitfile(contents).log_err())
    {
        let path = resolve_gitfile_path(dot_git_abs_path, path);
        if let Some(path) = fs.canonicalize(&path).await.log_err() {
            repository_dir_abs_path = Path::new(&path).into();
            common_dir_abs_path = repository_dir_abs_path.clone();

            if let Some(commondir_contents) = fs.load(&path.join("commondir")).await.ok()
                && let Some(commondir_path) = fs
                    .canonicalize(&resolve_commondir_path(&path, &commondir_contents))
                    .await
                    .log_err()
            {
                common_dir_abs_path = commondir_path.as_path().into();
            }
        }
    };
    (repository_dir_abs_path, common_dir_abs_path)
}

pub(super) struct NullWatcher;

impl fs::Watcher for NullWatcher {
    fn add(&self, _path: &Path) -> Result<()> {
        Ok(())
    }

    fn remove(&self, _path: &Path) -> Result<()> {
        Ok(())
    }
}
