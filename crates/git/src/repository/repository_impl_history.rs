use super::*;

impl RealGitRepository {
    pub(super) fn repository_default_branch(
        &self,
        include_remote_name: bool,
    ) -> BoxFuture<'_, Result<Option<SharedString>>> {
        let git = self.git_binary();
        self.executor
            .spawn(async move {
                let output = git
                    .run(&[
                        "for-each-ref",
                        "--format=%(refname)\t%(symref)",
                        "refs/remotes/upstream/HEAD",
                        "refs/remotes/origin/HEAD",
                        "refs/heads/",
                    ])
                    .await
                    .unwrap_or_default();
                let refs: HashMap<&str, &str> = output
                    .lines()
                    .filter_map(|line| line.split_once('\t'))
                    .collect();

                if let Some(target) = refs.get("refs/remotes/upstream/HEAD") {
                    let strip_prefix = if include_remote_name {
                        "refs/remotes/"
                    } else {
                        "refs/remotes/upstream/"
                    };
                    if let Some(branch) = target.strip_prefix(strip_prefix) {
                        return Ok(Some(branch.into()));
                    }
                }

                if let Some(target) = refs.get("refs/remotes/origin/HEAD") {
                    let strip_prefix = if include_remote_name {
                        "refs/remotes/"
                    } else {
                        "refs/remotes/origin/"
                    };
                    if let Some(branch) = target.strip_prefix(strip_prefix) {
                        return Ok(Some(branch.into()));
                    }
                }

                let local_branch_exists =
                    |branch: &str| refs.contains_key(format!("refs/heads/{branch}").as_str());

                if let Ok(default_branch) = git.run(&["config", "init.defaultBranch"]).await {
                    if local_branch_exists(&default_branch) {
                        return Ok(Some(default_branch.into()));
                    }
                }

                if local_branch_exists("main") {
                    return Ok(Some("main".into()));
                }

                if local_branch_exists("master") {
                    return Ok(Some("master".into()));
                }

                Ok(None)
            })
            .boxed()
    }

    pub(super) fn repository_run_hook(
        &self,
        hook: RunHook,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        let git_binary = self.git_binary_in_worktree();
        let git_dir = self.git_dir.clone();
        let help_output = self.any_git_binary_help_output();

        // Note: Do not spawn these commands on the background thread, as this causes some git hooks to hang.
        async move {
            let git_binary = git_binary?;
            let working_directory = git_binary.working_directory.clone();
            if !help_output
                .await
                .lines()
                .any(|line| line.trim().starts_with("hook "))
            {
                let hook_abs_path = git_dir.join("hooks").join(hook.as_str());
                if hook_abs_path.is_file() && git_binary.is_trusted {
                    #[allow(clippy::disallowed_methods)]
                    let output = new_command(&hook_abs_path)
                        .envs(env.iter())
                        .current_dir(&working_directory)
                        .output()
                        .await?;

                    if !output.status.success() {
                        return Err(GitBinaryCommandError {
                            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                            status: output.status,
                        }
                        .into());
                    }
                }

                return Ok(());
            }

            if git_binary.is_trusted {
                let git_binary = git_binary.envs(HashMap::clone(&env));
                git_binary
                    .run(&["hook", "run", "--ignore-missing", hook.as_str()])
                    .await?;
            }
            Ok(())
        }
        .boxed()
    }

    pub(super) fn repository_initial_graph_data(
        &self,
        log_source: LogSource,
        log_order: LogOrder,
        request_tx: Sender<Vec<Arc<InitialGraphCommitData>>>,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary();

        async move {
            let mut git_log_command = vec!["log", GRAPH_COMMIT_FORMAT, log_order.as_arg()];
            git_log_command.extend(log_source.get_args()?);
            let mut command = git.build_command(&git_log_command);
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());

            let mut child = command.spawn()?;
            let stdout = child.stdout.take().context("failed to get stdout")?;
            let stderr = child.stderr.take().context("failed to get stderr")?;
            let mut reader = BufReader::new(stdout);

            let mut line_buffer = String::new();
            let mut lines: Vec<String> = Vec::with_capacity(GRAPH_CHUNK_SIZE);

            loop {
                line_buffer.clear();
                let bytes_read = reader.read_line(&mut line_buffer).await?;

                if bytes_read == 0 {
                    if !lines.is_empty() {
                        let commits = parse_initial_graph_output(lines.iter().map(|s| s.as_str()));
                        if request_tx.send(commits).await.is_err() {
                            log::warn!(
                                "initial_graph_data: receiver dropped while sending commits"
                            );
                        }
                    }
                    break;
                }

                let line = line_buffer.trim_end_matches('\n').to_string();
                lines.push(line);

                if lines.len() >= GRAPH_CHUNK_SIZE {
                    let commits = parse_initial_graph_output(lines.iter().map(|s| s.as_str()));
                    if request_tx.send(commits).await.is_err() {
                        log::warn!("initial_graph_data: receiver dropped while streaming commits");
                        break;
                    }
                    lines.clear();
                }
            }

            let status = child.status().await?;
            if !status.success() {
                let mut stderr_output = String::new();
                BufReader::new(stderr)
                    .read_to_string(&mut stderr_output)
                    .await
                    .log_err();

                if stderr_output.is_empty() {
                    anyhow::bail!("git log command failed with {}", status);
                } else {
                    anyhow::bail!("git log command failed with {}: {}", status, stderr_output);
                }
            }
            Ok(())
        }
        .boxed()
    }

    pub(super) fn repository_search_commits(
        &self,
        log_source: LogSource,
        search_args: SearchCommitArgs,
        request_tx: Sender<Oid>,
    ) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary();

        async move {
            let mut args = vec!["log", SEARCH_COMMIT_FORMAT];
            let hash_query = commit_hash_search_query(search_args.query.as_str())
                .map(|query| query.to_ascii_lowercase());

            if hash_query.is_none() {
                args.push("--fixed-strings");

                if !search_args.case_sensitive {
                    args.push("--regexp-ignore-case");
                }

                args.push("--grep");
                args.push(search_args.query.as_str());
            }

            args.extend(log_source.get_args()?);
            let mut command = git.build_command(&args);
            command.stdout(Stdio::piped());
            command.stderr(Stdio::null());

            let mut child = command.spawn()?;
            let stdout = child.stdout.take().context("failed to get stdout")?;
            let mut reader = BufReader::new(stdout);

            let mut line_buffer = String::new();

            loop {
                line_buffer.clear();
                let bytes_read = reader.read_line(&mut line_buffer).await?;

                if bytes_read == 0 {
                    break;
                }

                let sha = line_buffer.trim_end_matches('\n');
                if let Some(hash_query) = hash_query.as_ref()
                    && !sha.to_ascii_lowercase().starts_with(hash_query)
                {
                    continue;
                }

                if let Ok(oid) = Oid::from_str(sha)
                    && request_tx.send(oid).await.is_err()
                {
                    break;
                }
            }

            child.status().await?;
            Ok(())
        }
        .boxed()
    }

    pub(super) fn repository_file_history_changed_files(
        &self,
        paths: Vec<RepoPath>,
        commit_limit: usize,
    ) -> BoxFuture<'_, Result<Vec<FileHistoryChangedFileSets>>> {
        let git = self.git_binary();

        async move {
            if paths.is_empty() {
                return Ok(Vec::new());
            }

            if commit_limit == 0 {
                return Ok(vec![FileHistoryChangedFileSets::default(); paths.len()]);
            }

            let max_count_arg = format!("--max-count={commit_limit}");
            let mut args = [
                "log",
                max_count_arg.as_str(),
                "--full-diff",
                "--no-renames",
                "--name-only",
                "-z",
                "--format=%x1e",
                "--",
            ]
            .map(OsString::from)
            .to_vec();
            args.extend(paths.iter().map(|path| OsString::from(path.as_unix_str())));

            let output = git.build_command(&args).output().await?;
            anyhow::ensure!(
                output.status.success(),
                "git log failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );

            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(parse_file_history_changed_files_output(&stdout, &paths))
        }
        .boxed()
    }

    pub(super) fn repository_commit_data_reader(&self) -> Result<CommitDataReader> {
        let git_binary = self.git_binary();

        let (request_tx, request_rx) = async_channel::bounded::<CommitDataRequest>(64);

        let task = self.executor.spawn(async move {
            if let Err(error) = run_commit_data_reader(git_binary, request_rx).await {
                log::error!("commit data reader failed: {error:?}");
            }
        });

        Ok(CommitDataReader {
            request_tx,
            _task: task,
        })
    }

    pub(super) fn repository_set_trusted(&self, trusted: bool) {
        self.is_trusted
            .store(trusted, std::sync::atomic::Ordering::Release);
    }

    pub(super) fn repository_is_trusted(&self) -> bool {
        self.is_trusted.load(std::sync::atomic::Ordering::Acquire)
    }
}
