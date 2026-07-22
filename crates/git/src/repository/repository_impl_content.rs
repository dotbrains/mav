use super::*;

impl RealGitRepository {
    pub(super) fn repository_path(&self) -> PathBuf {
        self.git_dir.clone()
    }

    pub(super) fn repository_main_repository_path(&self) -> PathBuf {
        self.common_dir.clone()
    }

    pub(super) fn repository_show(&self, commit: String) -> BoxFuture<'_, Result<CommitDetails>> {
        let git = self.git_binary();
        self.executor
            .spawn(async move {
                let output = git
                    .build_command(&[
                        "show",
                        "--no-patch",
                        "--format=%H%x00%B%x00%at%x00%ae%x00%an%x00",
                        &commit,
                    ])
                    .output()
                    .await?;
                let output = std::str::from_utf8(&output.stdout)?;
                let fields = output.split('\0').collect::<Vec<_>>();
                if fields.len() != 6 {
                    bail!("unexpected git-show output for {commit:?}: {output:?}")
                }
                let sha = fields[0].to_string().into();
                let message = fields[1].to_string().into();
                let commit_timestamp = fields[2].parse()?;
                let author_email = fields[3].to_string().into();
                let author_name = fields[4].to_string().into();
                Ok(CommitDetails {
                    sha,
                    message,
                    commit_timestamp,
                    author_email,
                    author_name,
                })
            })
            .boxed()
    }

    pub(super) fn repository_reset(
        &self,
        commit: String,
        mode: ResetMode,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary_in_worktree();
        async move {
            let git = git?;
            let mode_flag = match mode {
                ResetMode::Mixed => "--mixed",
                ResetMode::Soft => "--soft",
            };

            let output = git
                .build_command(&["reset", mode_flag, &commit])
                .envs(env.iter())
                .output()
                .await?;
            anyhow::ensure!(
                output.status.success(),
                "Failed to reset:\n{}",
                String::from_utf8_lossy(&output.stderr),
            );
            Ok(())
        }
        .boxed()
    }

    pub(super) fn repository_checkout_files(
        &self,
        commit: String,
        paths: Vec<RepoPath>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary_in_worktree();
        async move {
            let git = git?;
            if paths.is_empty() {
                return Ok(());
            }

            let output = git
                .build_command(&["checkout", &commit, "--"])
                .envs(env.iter())
                .args(paths.iter().map(|path| path.as_unix_str()))
                .output()
                .await?;
            anyhow::ensure!(
                output.status.success(),
                "Failed to checkout files:\n{}",
                String::from_utf8_lossy(&output.stderr),
            );
            Ok(())
        }
        .boxed()
    }

    pub(super) fn repository_load_index_text(
        &self,
        path: RepoPath,
    ) -> BoxFuture<'_, Option<String>> {
        let git_binary = self.git_binary();
        let path_str = format!(":{}", path.as_unix_str());
        self.executor
            .spawn(async move {
                let git = git_binary;
                let output = git
                    .build_command(&["show", &path_str])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .log_err()?;
                if !output.status.success() {
                    return None;
                }
                String::from_utf8(output.stdout).ok()
            })
            .boxed()
    }

    pub(super) fn repository_load_committed_text(
        &self,
        path: RepoPath,
    ) -> BoxFuture<'_, Option<String>> {
        let git = self.git_binary();
        let path_str = format!("HEAD:{}", path.as_unix_str());
        self.executor
            .spawn(async move {
                let output = git
                    .build_command(&["show", &path_str])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .log_err()?;
                if !output.status.success() {
                    return None;
                }
                String::from_utf8(output.stdout).ok()
            })
            .boxed()
    }

    pub(super) fn repository_load_blob_content(&self, oid: Oid) -> BoxFuture<'_, Result<String>> {
        let git_binary = self.git_binary();
        let oid_str = oid.to_string();
        self.executor
            .spawn(async move { git_binary.run_raw(&["cat-file", "blob", &oid_str]).await })
            .boxed()
    }
}
