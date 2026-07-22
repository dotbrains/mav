use super::*;

impl RealGitRepository {
    pub(super) fn repository_change_branch(&self, name: String) -> BoxFuture<'_, Result<()>> {
        let git_binary = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let git_binary = git_binary?;
                let local_ref = format!("refs/heads/{name}");
                if git_binary
                    .run(&["show-ref", "--verify", "--quiet", &local_ref])
                    .await
                    .is_ok()
                {
                    git_binary.run(&["checkout", &name]).await?;
                    return anyhow::Ok(());
                }

                let remote_ref = format!("refs/remotes/{name}");
                if git_binary
                    .run(&["show-ref", "--verify", "--quiet", &remote_ref])
                    .await
                    .is_ok()
                {
                    let (_, branch_name) =
                        name.split_once('/').context("Unexpected branch format")?;
                    let local_branch_ref = format!("refs/heads/{branch_name}");
                    if git_binary
                        .run(&["show-ref", "--verify", "--quiet", &local_branch_ref])
                        .await
                        .is_ok()
                    {
                        git_binary
                            .run(&["branch", "--set-upstream-to", &name, branch_name])
                            .await?;
                    } else {
                        git_binary
                            .run(&["branch", "--track", branch_name, &name])
                            .await?;
                    }

                    git_binary.run(&["checkout", branch_name]).await?;
                    return anyhow::Ok(());
                }

                anyhow::bail!("Branch '{}' not found", name);
            })
            .boxed()
    }

    pub(super) fn repository_create_branch(
        &self,
        name: String,
        base_branch: Option<String>,
    ) -> BoxFuture<'_, Result<()>> {
        let git_binary = self.git_binary_in_worktree();

        self.executor
            .spawn(async move {
                let git_binary = git_binary?;
                let mut args = vec!["switch", "-c", &name];
                let base_branch_str;
                if let Some(ref base) = base_branch {
                    base_branch_str = base.clone();
                    args.push(&base_branch_str);
                }

                git_binary.run(&args).await?;
                anyhow::Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_rename_branch(
        &self,
        branch: String,
        new_name: String,
    ) -> BoxFuture<'_, Result<()>> {
        let git_binary = self.git_binary_in_worktree();

        self.executor
            .spawn(async move {
                let git_binary = git_binary?;
                git_binary
                    .run(&["branch", "-m", &branch, &new_name])
                    .await?;
                anyhow::Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_delete_branch(
        &self,
        is_remote: bool,
        name: String,
        force: bool,
    ) -> BoxFuture<'_, Result<()>> {
        let git_binary = self.git_binary_in_worktree();

        self.executor
            .spawn(async move {
                let git_binary = git_binary?;
                let flag = delete_branch_flag(is_remote, force);
                git_binary.run(&["branch", flag, &name]).await?;
                anyhow::Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_blame(
        &self,
        path: RepoPath,
        content: Rope,
        line_ending: LineEnding,
    ) -> BoxFuture<'_, Result<crate::blame::Blame>> {
        let git = self.git_binary_in_worktree();

        self.executor
            .spawn(async move {
                let git = git?;
                crate::blame::Blame::for_path(&git, &path, &content, line_ending).await
            })
            .boxed()
    }

    pub(super) fn repository_diff(&self, diff: DiffType) -> BoxFuture<'_, Result<String>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let git = git?;
                let output = match diff {
                    DiffType::HeadToIndex => {
                        git.build_command(&["diff", "--staged"]).output().await?
                    }
                    DiffType::HeadToWorktree => git.build_command(&["diff"]).output().await?,
                    DiffType::MergeBase { base_ref } => {
                        git.build_command(&["diff", "--merge-base", base_ref.as_ref()])
                            .output()
                            .await?
                    }
                };

                anyhow::ensure!(
                    output.status.success(),
                    "Failed to run git diff:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            })
            .boxed()
    }

    pub(super) fn repository_diff_stat(
        &self,
        path_prefixes: &[RepoPath],
    ) -> BoxFuture<'static, Result<crate::status::GitDiffStat>> {
        let path_prefixes = path_prefixes.to_vec();
        let git_binary = self.git_binary_in_worktree();

        self.executor
            .spawn(async move {
                let git_binary = git_binary?;
                let mut args: Vec<String> = vec![
                    "diff".into(),
                    "--numstat".into(),
                    "--no-renames".into(),
                    "HEAD".into(),
                ];
                if !path_prefixes.is_empty() {
                    args.push("--".into());
                    args.extend(
                        path_prefixes
                            .iter()
                            .map(|p| p.as_std_path().to_string_lossy().into_owned()),
                    );
                }
                let output = git_binary.run(&args).await?;
                Ok(crate::status::parse_numstat(&output))
            })
            .boxed()
    }
}
