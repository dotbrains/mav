use super::*;

impl RealGitRepository {
    pub(super) fn repository_stage_paths(
        &self,
        paths: Vec<RepoPath>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let git = git?;
                if !paths.is_empty() {
                    let output = git
                        .build_command(&["update-index", "--add", "--remove", "--"])
                        .envs(env.iter())
                        .args(paths.iter().map(|p| p.as_unix_str()))
                        .output()
                        .await?;
                    anyhow::ensure!(
                        output.status.success(),
                        "Failed to stage paths:\n{}",
                        String::from_utf8_lossy(&output.stderr),
                    );
                }
                Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_unstage_paths(
        &self,
        paths: Vec<RepoPath>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary_in_worktree();

        self.executor
            .spawn(async move {
                let git = git?;
                if !paths.is_empty() {
                    let output = git
                        .build_command(&["reset", "--quiet", "--"])
                        .envs(env.iter())
                        .args(paths.iter().map(|p| p.as_std_path()))
                        .output()
                        .await?;

                    anyhow::ensure!(
                        output.status.success(),
                        "Failed to unstage:\n{}",
                        String::from_utf8_lossy(&output.stderr),
                    );
                }
                Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_stash_paths(
        &self,
        paths: Vec<RepoPath>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let git = git?;
                let output = git
                    .build_command(&["stash", "push", "--quiet", "--include-untracked", "--"])
                    .envs(env.iter())
                    .args(paths.iter().map(|p| p.as_unix_str()))
                    .output()
                    .await?;

                anyhow::ensure!(
                    output.status.success(),
                    "Failed to stash:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_stash_pop(
        &self,
        index: Option<usize>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let git = git?;
                let mut args = vec!["stash".to_string(), "pop".to_string()];
                if let Some(index) = index {
                    args.push(format!("stash@{{{}}}", index));
                }
                let output = git.build_command(&args).envs(env.iter()).output().await?;

                anyhow::ensure!(
                    output.status.success(),
                    "Failed to stash pop:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_stash_apply(
        &self,
        index: Option<usize>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let git = git?;
                let mut args = vec!["stash".to_string(), "apply".to_string()];
                if let Some(index) = index {
                    args.push(format!("stash@{{{}}}", index));
                }
                let output = git.build_command(&args).envs(env.iter()).output().await?;

                anyhow::ensure!(
                    output.status.success(),
                    "Failed to apply stash:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_stash_drop(
        &self,
        index: Option<usize>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let git = git?;
                let mut args = vec!["stash".to_string(), "drop".to_string()];
                if let Some(index) = index {
                    args.push(format!("stash@{{{}}}", index));
                }
                let output = git.build_command(&args).envs(env.iter()).output().await?;

                anyhow::ensure!(
                    output.status.success(),
                    "Failed to stash drop:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_commit(
        &self,
        message: SharedString,
        name_and_email: Option<(SharedString, SharedString)>,
        options: CommitOptions,
        ask_pass: AskPassDelegate,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary_in_worktree();
        let executor = self.executor.clone();
        // Note: Do not spawn this command on the background thread, it might pop open the credential helper
        // which we want to block on.
        async move {
            let git = git?;
            let mut cmd = git.build_command(&["commit", "--quiet", "-m"]);
            cmd.envs(env.iter())
                .arg(&message.to_string())
                .arg("--cleanup=strip")
                .arg("--no-verify")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            if options.amend {
                cmd.arg("--amend");
            }

            if options.signoff {
                cmd.arg("--signoff");
            }

            if options.allow_empty {
                cmd.arg("--allow-empty");
            }

            if let Some((name, email)) = name_and_email {
                cmd.arg("--author").arg(&format!("{name} <{email}>"));
            }

            run_git_command(env, ask_pass, cmd, executor).await?;

            Ok(())
        }
        .boxed()
    }

    pub(super) fn repository_update_ref(
        &self,
        ref_name: String,
        commit: String,
    ) -> BoxFuture<'_, Result<()>> {
        self.edit_ref(RefEdit::Update { ref_name, commit })
    }

    pub(super) fn repository_delete_ref(&self, ref_name: String) -> BoxFuture<'_, Result<()>> {
        self.edit_ref(RefEdit::Delete { ref_name })
    }

    pub(super) fn repository_repair_worktrees(&self) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary();
        self.executor
            .spawn(async move {
                let args: Vec<OsString> = vec!["worktree".into(), "repair".into()];
                git.run(&args).await?;
                Ok(())
            })
            .boxed()
    }
}
