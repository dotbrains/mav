use super::*;

impl RealGitRepository {
    pub(super) fn repository_push(
        &self,
        branch_name: String,
        remote_branch_name: String,
        remote_name: String,
        options: Option<PushOptions>,
        ask_pass: AskPassDelegate,
        env: Arc<HashMap<String, String>>,
        cx: AsyncApp,
    ) -> BoxFuture<'_, Result<RemoteCommandOutput>> {
        let working_directory = self.command_directory();
        let git_directory = self.path();
        let executor = cx.background_executor().clone();
        let git_binary_path = self.system_git_binary_path.clone();
        let is_trusted = self.is_trusted();
        // Note: Do not spawn this command on the background thread, it might pop open the credential helper
        // which we want to block on.
        async move {
            let git_binary_path = git_binary_path.context("git not found on $PATH, can't push")?;
            let git = GitBinary::new(
                git_binary_path,
                working_directory,
                git_directory,
                executor.clone(),
                is_trusted,
            );
            let mut command = git.build_command(&["push"]);
            command
                .envs(env.iter())
                .args(options.map(|option| match option {
                    PushOptions::SetUpstream => "--set-upstream",
                    PushOptions::Force => "--force-with-lease",
                }))
                .arg(remote_name)
                .arg(format!("{}:{}", branch_name, remote_branch_name))
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            run_git_command(env, ask_pass, command, executor).await
        }
        .boxed()
    }

    pub(super) fn repository_pull(
        &self,
        branch_name: Option<String>,
        remote_name: String,
        rebase: bool,
        ask_pass: AskPassDelegate,
        env: Arc<HashMap<String, String>>,
        cx: AsyncApp,
    ) -> BoxFuture<'_, Result<RemoteCommandOutput>> {
        let working_directory = self.command_directory();
        let git_directory = self.path();
        let executor = cx.background_executor().clone();
        let git_binary_path = self.system_git_binary_path.clone();
        let is_trusted = self.is_trusted();
        // Note: Do not spawn this command on the background thread, it might pop open the credential helper
        // which we want to block on.
        async move {
            let git_binary_path = git_binary_path.context("git not found on $PATH, can't pull")?;
            let git = GitBinary::new(
                git_binary_path,
                working_directory,
                git_directory,
                executor.clone(),
                is_trusted,
            );
            let mut command = git.build_command(&["pull"]);
            command.envs(env.iter());

            if rebase {
                command.arg("--rebase");
            }

            command
                .arg(remote_name)
                .args(branch_name)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            run_git_command(env, ask_pass, command, executor).await
        }
        .boxed()
    }

    pub(super) fn repository_fetch(
        &self,
        fetch_options: FetchOptions,
        ask_pass: AskPassDelegate,
        env: Arc<HashMap<String, String>>,
        cx: AsyncApp,
    ) -> BoxFuture<'_, Result<RemoteCommandOutput>> {
        let working_directory = self.command_directory();
        let git_directory = self.path();
        let remote_name = format!("{}", fetch_options);
        let git_binary_path = self.system_git_binary_path.clone();
        let executor = cx.background_executor().clone();
        let is_trusted = self.is_trusted();
        // Note: Do not spawn this command on the background thread, it might pop open the credential helper
        // which we want to block on.
        async move {
            let git_binary_path = git_binary_path.context("git not found on $PATH, can't fetch")?;
            let git = GitBinary::new(
                git_binary_path,
                working_directory,
                git_directory,
                executor.clone(),
                is_trusted,
            );
            let mut command = git.build_command(&["fetch", &remote_name]);
            command
                .envs(env.iter())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            run_git_command(env, ask_pass, command, executor).await
        }
        .boxed()
    }

    pub(super) fn repository_get_push_remote(
        &self,
        branch: String,
    ) -> BoxFuture<'_, Result<Option<Remote>>> {
        let git = self.git_binary();
        self.executor
            .spawn(async move {
                let output = git
                    .build_command(&["rev-parse", "--abbrev-ref"])
                    .arg(format!("{branch}@{{push}}"))
                    .output()
                    .await?;
                if !output.status.success() {
                    return Ok(None);
                }
                let remote_name = String::from_utf8_lossy(&output.stdout)
                    .split('/')
                    .next()
                    .map(|name| Remote {
                        name: name.trim().to_string().into(),
                    });

                Ok(remote_name)
            })
            .boxed()
    }

    pub(super) fn repository_get_branch_remote(
        &self,
        branch: String,
    ) -> BoxFuture<'_, Result<Option<Remote>>> {
        let git = self.git_binary();
        self.executor
            .spawn(async move {
                let output = git
                    .build_command(&["config", "--get"])
                    .arg(format!("branch.{branch}.remote"))
                    .output()
                    .await?;
                if !output.status.success() {
                    return Ok(None);
                }

                let remote_name = String::from_utf8_lossy(&output.stdout);
                return Ok(Some(Remote {
                    name: remote_name.trim().to_string().into(),
                }));
            })
            .boxed()
    }

    pub(super) fn repository_get_all_remotes(&self) -> BoxFuture<'_, Result<Vec<Remote>>> {
        let git = self.git_binary();
        self.executor
            .spawn(async move {
                let output = git.build_command(&["remote", "-v"]).output().await?;

                anyhow::ensure!(
                    output.status.success(),
                    "Failed to get all remotes:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let remote_names: HashSet<Remote> = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .filter(|line| !line.is_empty())
                    .filter_map(|line| {
                        let mut split_line = line.split_whitespace();
                        let remote_name = split_line.next()?;

                        Some(Remote {
                            name: remote_name.trim().to_string().into(),
                        })
                    })
                    .collect();

                Ok(remote_names.into_iter().collect())
            })
            .boxed()
    }

    pub(super) fn repository_remove_remote(&self, name: String) -> BoxFuture<'_, Result<()>> {
        let git_binary = self.git_binary();
        self.executor
            .spawn(async move {
                git_binary.run(&["remote", "remove", &name]).await?;
                Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_create_remote(
        &self,
        name: String,
        url: String,
    ) -> BoxFuture<'_, Result<()>> {
        let git_binary = self.git_binary();
        self.executor
            .spawn(async move {
                git_binary.run(&["remote", "add", &name, &url]).await?;
                Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_check_for_pushed_commit(
        &self,
    ) -> BoxFuture<'_, Result<Vec<SharedString>>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                // This command outputs a list of remote tracking refs, e.g.:
                // refs/remotes/origin/HEAD
                // refs/remotes/origin/main
                let Ok(output) = git?
                    .run(&[
                        "for-each-ref",
                        "--format=%(refname)",
                        "--contains",
                        "HEAD",
                        "refs/remotes/",
                    ])
                    .await
                else {
                    return Ok(Vec::new());
                };

                Ok(output
                    .lines()
                    .map(|line| line.trim())
                    .filter(|line| !line.ends_with("/HEAD"))
                    .filter_map(|line| line.strip_prefix("refs/remotes/"))
                    .map(SharedString::from)
                    .collect())
            })
            .boxed()
    }
}
