use super::*;

impl RealGitRepository {
    pub(super) fn repository_remote_urls(&self) -> BoxFuture<'_, HashMap<String, String>> {
        let git = self.git_binary();
        self.executor
            .spawn(async move {
                let mut urls = HashMap::default();
                if let Ok(stdout) = git.run(&["remote", "-v"]).await {
                    for line in stdout.lines() {
                        if let Some(line) = line.strip_suffix(" (fetch)")
                            && let Some((name, url)) = line.split_once(char::is_whitespace)
                        {
                            urls.insert(name.to_string(), url.trim_start().to_string());
                        }
                    }
                }
                urls
            })
            .boxed()
    }

    pub(super) fn repository_revparse_batch(
        &self,
        revs: Vec<String>,
    ) -> BoxFuture<'_, Result<Vec<Option<String>>>> {
        let git = self.git_binary();
        self.executor
            .spawn(async move {
                let mut process = git
                    .build_command(&["cat-file", "--batch-check=%(objectname)"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()?;

                let stdin = process
                    .stdin
                    .take()
                    .context("no stdin for git cat-file subprocess")?;
                let mut stdin = BufWriter::new(stdin);
                for rev in &revs {
                    stdin.write_all(rev.as_bytes()).await?;
                    stdin.write_all(b"\n").await?;
                }
                stdin.flush().await?;
                drop(stdin);

                let output = process.output().await?;
                let output = std::str::from_utf8(&output.stdout)?;
                let shas = output
                    .lines()
                    .map(|line| {
                        if line.ends_with("missing") {
                            None
                        } else {
                            Some(line.to_string())
                        }
                    })
                    .collect::<Vec<_>>();

                if shas.len() != revs.len() {
                    // In an octopus merge, git cat-file still only outputs the first sha from MERGE_HEAD.
                    bail!("unexpected number of shas")
                }

                Ok(shas)
            })
            .boxed()
    }

    pub(super) fn repository_merge_message(&self) -> BoxFuture<'_, Option<String>> {
        let path = self.path().join("MERGE_MSG");
        self.executor
            .spawn(async move { std::fs::read_to_string(&path).ok() })
            .boxed()
    }

    pub(super) fn repository_status(&self, path_prefixes: &[RepoPath]) -> Task<Result<GitStatus>> {
        let git = self.git_binary_in_worktree();
        let args = git_status_args(path_prefixes);
        log::debug!("Checking for git status in {path_prefixes:?}");
        self.executor.spawn(async move {
            let git = git?;
            let output = git.build_command(&args).output().await?;
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.parse()
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("git status failed: {stderr}");
            }
        })
    }

    pub(super) fn repository_check_access(&self) -> BoxFuture<'_, Result<()>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                git?.run(&["rev-parse"]).await?;
                Ok(())
            })
            .boxed()
    }

    pub(super) fn repository_diff_tree(
        &self,
        request: DiffTreeType,
    ) -> BoxFuture<'_, Result<TreeDiff>> {
        let git = self.git_binary_in_worktree();

        let mut args = vec![
            OsString::from("diff-tree"),
            OsString::from("-r"),
            OsString::from("-z"),
            OsString::from("--no-renames"),
        ];
        match request {
            DiffTreeType::MergeBase { base, head } => {
                args.push("--merge-base".into());
                args.push(OsString::from(base.as_str()));
                args.push(OsString::from(head.as_str()));
            }
            DiffTreeType::Since { base, head } => {
                args.push(OsString::from(base.as_str()));
                args.push(OsString::from(head.as_str()));
            }
        }

        self.executor
            .spawn(async move {
                let git = git?;
                let output = git.build_command(&args).output().await?;
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    stdout.parse()
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("git status failed: {stderr}");
                }
            })
            .boxed()
    }

    pub(super) fn repository_stash_entries(&self) -> BoxFuture<'static, Result<GitStash>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let git = git?;
                let output = git
                    .build_command(&["stash", "list", "--pretty=format:%gd%x00%H%x00%ct%x00%s"])
                    .output()
                    .await?;
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    stdout.parse()
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    anyhow::bail!("git status failed: {stderr}");
                }
            })
            .boxed()
    }
}
