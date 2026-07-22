use super::*;

impl RealGitRepository {
    pub(super) fn repository_checkpoint(
        &self,
    ) -> BoxFuture<'static, Result<GitRepositoryCheckpoint>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let mut git = git?.envs(checkpoint_author_envs());
                git.with_temp_index(async |git| {
                    let head_sha = git.run(&["rev-parse", "HEAD"]).await.ok();
                    let mut excludes = exclude_files(git).await?;

                    git.run(&["add", "--all"]).await?;
                    let tree = git.run(&["write-tree"]).await?;
                    let checkpoint_sha = if let Some(head_sha) = head_sha.as_deref() {
                        git.run(&["commit-tree", &tree, "-p", head_sha, "-m", "Checkpoint"])
                            .await?
                    } else {
                        git.run(&["commit-tree", &tree, "-m", "Checkpoint"]).await?
                    };

                    excludes.restore_original().await?;

                    Ok(GitRepositoryCheckpoint {
                        commit_sha: checkpoint_sha.parse()?,
                    })
                })
                .await
            })
            .boxed()
    }

    pub(super) fn repository_restore_checkpoint(
        &self,
        checkpoint: GitRepositoryCheckpoint,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let git = git?;
                git.run(&[
                    "restore",
                    "--source",
                    &checkpoint.commit_sha.to_string(),
                    "--worktree",
                    ".",
                ])
                .await?;

                // TODO: We don't track binary and large files anymore,
                //       so the following call would delete them.
                //       Implement an alternative way to track files added by agent.
                //
                // git.with_temp_index(async move |git| {
                //     git.run(&["read-tree", &checkpoint.commit_sha.to_string()])
                //         .await?;
                //     git.run(&["clean", "-d", "--force"]).await
                // })
                // .await?;

                Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_create_archive_checkpoint(
        &self,
    ) -> BoxFuture<'_, Result<(String, String)>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let mut git = git?.envs(checkpoint_author_envs());
                let head_sha = git
                    .run(&["rev-parse", "HEAD"])
                    .await
                    .context("failed to read HEAD")?;

                // Capture the staged state: write-tree reads the current index
                let staged_tree = git
                    .run(&["write-tree"])
                    .await
                    .context("failed to write staged tree")?;
                let staged_sha = git
                    .run(&[
                        "commit-tree",
                        &staged_tree,
                        "-p",
                        &head_sha,
                        "-m",
                        "WIP staged",
                    ])
                    .await
                    .context("failed to create staged commit")?;

                // Capture the full state (staged + unstaged + untracked) using
                // a temporary index so we don't disturb the real one.
                let unstaged_sha = git
                    .with_temp_index(async |git| {
                        git.run(&["add", "--all"]).await?;
                        let full_tree = git.run(&["write-tree"]).await?;
                        let sha = git
                            .run(&[
                                "commit-tree",
                                &full_tree,
                                "-p",
                                &staged_sha,
                                "-m",
                                "WIP unstaged",
                            ])
                            .await?;
                        Ok(sha)
                    })
                    .await
                    .context("failed to create unstaged commit")?;

                Ok((staged_sha, unstaged_sha))
            })
            .boxed()
    }

    pub(super) fn repository_restore_archive_checkpoint(
        &self,
        staged_sha: String,
        unstaged_sha: String,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let git = git?;
                // First, set the index AND working tree to match the unstaged
                // tree. --reset -u computes a tree-level diff between the
                // current index and unstaged_sha's tree and applies additions,
                // modifications, and deletions to the working directory.
                git.run(&["read-tree", "--reset", "-u", &unstaged_sha])
                    .await
                    .context("failed to restore working directory from unstaged commit")?;

                // Then replace just the index with the staged tree. Without -u
                // this doesn't touch the working directory, so the result is:
                // working tree = unstaged state, index = staged state.
                git.run(&["read-tree", &staged_sha])
                    .await
                    .context("failed to restore index from staged commit")?;

                Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_compare_checkpoints(
        &self,
        left: GitRepositoryCheckpoint,
        right: GitRepositoryCheckpoint,
    ) -> BoxFuture<'_, Result<bool>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let git = git?;
                let result = git
                    .run(&[
                        "diff-tree",
                        "--quiet",
                        &left.commit_sha.to_string(),
                        &right.commit_sha.to_string(),
                    ])
                    .await;
                match result {
                    Ok(_) => Ok(true),
                    Err(error) => {
                        if let Some(GitBinaryCommandError { status, .. }) =
                            error.downcast_ref::<GitBinaryCommandError>()
                            && status.code() == Some(1)
                        {
                            return Ok(false);
                        }

                        Err(error)
                    }
                }
            })
            .boxed()
    }

    pub(super) fn repository_diff_checkpoints(
        &self,
        base_checkpoint: GitRepositoryCheckpoint,
        target_checkpoint: GitRepositoryCheckpoint,
    ) -> BoxFuture<'_, Result<String>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let git = git?;
                git.run(&[
                    "diff",
                    "--find-renames",
                    "--patch",
                    &base_checkpoint.commit_sha.to_string(),
                    &target_checkpoint.commit_sha.to_string(),
                ])
                .await
            })
            .boxed()
    }
}
