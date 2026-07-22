use super::*;

impl RealGitRepository {
    pub(super) fn repository_worktrees(&self) -> BoxFuture<'_, Result<Vec<Worktree>>> {
        let git = self.git_binary();
        let main_worktree_path = original_repo_path_from_common_dir(&self.common_dir);
        self.executor
            .spawn(async move {
                let output = git
                    .build_command(&["worktree", "list", "--porcelain"])
                    .output()
                    .await?;
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    Ok(parse_worktrees_from_str(
                        &stdout,
                        main_worktree_path.as_deref(),
                    ))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("git worktree list failed: {stderr}");
                }
            })
            .boxed()
    }

    pub(super) fn repository_worktree_created_at(
        &self,
        worktree_path: PathBuf,
    ) -> BoxFuture<'_, Result<Option<SystemTime>>> {
        self.executor
            .spawn(async move {
                match std::fs::metadata(&worktree_path) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(None);
                    }
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!("failed to stat {}", worktree_path.display())
                        });
                    }
                    Ok(_) => {}
                }
                let git_dir = linked_worktree_git_dir(&worktree_path)?;
                let metadata = std::fs::metadata(&git_dir)
                    .with_context(|| format!("failed to stat {}", git_dir.display()))?;
                let created_at = metadata.created().with_context(|| {
                    format!("creation time unavailable for {}", git_dir.display())
                })?;
                Ok(Some(created_at))
            })
            .boxed()
    }

    pub(super) fn repository_create_worktree(
        &self,
        target: CreateWorktreeTarget,
        path: PathBuf,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary();
        let mut args = vec![OsString::from("worktree"), OsString::from("add")];

        match &target {
            CreateWorktreeTarget::ExistingBranch { branch_name } => {
                args.push(OsString::from("--"));
                args.push(OsString::from(path.as_os_str()));
                args.push(OsString::from(branch_name));
            }
            CreateWorktreeTarget::NewBranch {
                branch_name,
                base_sha: start_point,
            } => {
                args.push(OsString::from("-b"));
                args.push(OsString::from(branch_name));
                args.push(OsString::from("--"));
                args.push(OsString::from(path.as_os_str()));
                args.push(OsString::from(start_point.as_deref().unwrap_or("HEAD")));
            }
            CreateWorktreeTarget::Detached {
                base_sha: start_point,
            } => {
                args.push(OsString::from("--detach"));
                args.push(OsString::from("--"));
                args.push(OsString::from(path.as_os_str()));
                args.push(OsString::from(start_point.as_deref().unwrap_or("HEAD")));
            }
        }

        self.executor
            .spawn(async move {
                std::fs::create_dir_all(path.parent().unwrap_or(&path))?;
                let output = git.build_command(&args).output().await?;
                if output.status.success() {
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("git worktree add failed: {stderr}");
                }
            })
            .boxed()
    }

    pub(super) fn repository_remove_worktree(
        &self,
        path: PathBuf,
        force: bool,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary();

        self.executor
            .spawn(async move {
                let mut args: Vec<OsString> = vec!["worktree".into(), "remove".into()];
                if force {
                    args.push("--force".into());
                }
                args.push("--".into());
                args.push(path.as_os_str().into());
                git.run(&args).await?;
                anyhow::Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_rename_worktree(
        &self,
        old_path: PathBuf,
        new_path: PathBuf,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary();

        self.executor
            .spawn(async move {
                let args: Vec<OsString> = vec![
                    "worktree".into(),
                    "move".into(),
                    "--".into(),
                    old_path.as_os_str().into(),
                    new_path.as_os_str().into(),
                ];
                git.run(&args).await?;
                anyhow::Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_checkout_branch_in_worktree(
        &self,
        branch_name: String,
        worktree_path: PathBuf,
        create: bool,
    ) -> BoxFuture<'_, Result<()>> {
        let git_binary = GitBinary::new(
            self.any_git_binary_path.clone(),
            worktree_path,
            self.path(),
            self.executor.clone(),
            self.is_trusted(),
        );

        self.executor
            .spawn(async move {
                if create {
                    git_binary.run(&["checkout", "-b", &branch_name]).await?;
                } else {
                    git_binary.run(&["checkout", &branch_name]).await?;
                }
                anyhow::Ok(())
            })
            .boxed()
    }
}
