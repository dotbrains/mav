use super::*;

impl RealGitRepository {
    pub(super) fn repository_load_commit(
        &self,
        commit: String,
        cx: AsyncApp,
    ) -> BoxFuture<'_, Result<CommitDiff>> {
        let git = self.git_binary();
        cx.background_spawn(async move {
            let show_output = git
                .build_command(&[
                    "show",
                    "--format=",
                    "-z",
                    "--no-renames",
                    "--name-status",
                    "--first-parent",
                ])
                .arg(&commit)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
                .context("starting git show process")?;

            let show_stdout = String::from_utf8_lossy(&show_output.stdout);
            let changes = parse_git_diff_name_status(&show_stdout);
            let parent_sha = format!("{}^", commit);

            let mut cat_file_process = git
                .build_command(&["cat-file", "--batch=%(objectsize)"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .context("starting git cat-file process")?;

            let mut files = Vec::<CommitFile>::new();
            let mut stdin = BufWriter::with_capacity(512, cat_file_process.stdin.take().unwrap());
            let mut stdout = BufReader::new(cat_file_process.stdout.take().unwrap());
            let mut info_line = String::new();
            let mut newline = [b'\0'];
            for (path, status_code) in changes {
                // git-show outputs `/`-delimited paths even on Windows.
                let Some(rel_path) = RelPath::unix(path).log_err() else {
                    continue;
                };

                match status_code {
                    StatusCode::Modified => {
                        stdin.write_all(commit.as_bytes()).await?;
                        stdin.write_all(b":").await?;
                        stdin.write_all(path.as_bytes()).await?;
                        stdin.write_all(b"\n").await?;
                        stdin.write_all(parent_sha.as_bytes()).await?;
                        stdin.write_all(b":").await?;
                        stdin.write_all(path.as_bytes()).await?;
                        stdin.write_all(b"\n").await?;
                    }
                    StatusCode::Added => {
                        stdin.write_all(commit.as_bytes()).await?;
                        stdin.write_all(b":").await?;
                        stdin.write_all(path.as_bytes()).await?;
                        stdin.write_all(b"\n").await?;
                    }
                    StatusCode::Deleted => {
                        stdin.write_all(parent_sha.as_bytes()).await?;
                        stdin.write_all(b":").await?;
                        stdin.write_all(path.as_bytes()).await?;
                        stdin.write_all(b"\n").await?;
                    }
                    _ => continue,
                }
                stdin.flush().await?;

                info_line.clear();
                stdout.read_line(&mut info_line).await?;

                let len = info_line.trim_end().parse().with_context(|| {
                    format!("invalid object size output from cat-file {info_line}")
                })?;
                let mut text_bytes = vec![0; len];
                stdout.read_exact(&mut text_bytes).await?;
                stdout.read_exact(&mut newline).await?;

                let mut old_text = None;
                let mut new_text = None;
                let mut is_binary = is_binary_content(&text_bytes);
                let text = if is_binary {
                    String::new()
                } else {
                    String::from_utf8_lossy(&text_bytes).to_string()
                };

                match status_code {
                    StatusCode::Modified => {
                        info_line.clear();
                        stdout.read_line(&mut info_line).await?;
                        let len = info_line.trim_end().parse().with_context(|| {
                            format!("invalid object size output from cat-file {}", info_line)
                        })?;
                        let mut parent_bytes = vec![0; len];
                        stdout.read_exact(&mut parent_bytes).await?;
                        stdout.read_exact(&mut newline).await?;
                        is_binary = is_binary || is_binary_content(&parent_bytes);
                        if is_binary {
                            old_text = Some(String::new());
                            new_text = Some(String::new());
                        } else {
                            old_text = Some(String::from_utf8_lossy(&parent_bytes).to_string());
                            new_text = Some(text);
                        }
                    }
                    StatusCode::Added => new_text = Some(text),
                    StatusCode::Deleted => old_text = Some(text),
                    _ => continue,
                }

                files.push(CommitFile {
                    path: RepoPath(Arc::from(rel_path)),
                    old_text,
                    new_text,
                    is_binary,
                })
            }

            Ok(CommitDiff { files })
        })
        .boxed()
    }

    pub(super) fn repository_load_commit_template(
        &self,
    ) -> BoxFuture<'_, Result<Option<GitCommitTemplate>>> {
        let working_directory = self.working_directory();
        let git_binary = self.git_binary_in_worktree();

        self.executor
            .spawn(async move {
                let working_directory = working_directory?;
                let git_binary = git_binary?;
                let output = git_binary
                    .build_command(&["config", "--get", "commit.template"])
                    .output()
                    .await
                    .context("failed to run git config --get commit.template")?;

                let raw_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !output.status.success() || raw_path.is_empty() {
                    return Ok(None);
                }

                let path = PathBuf::from(&raw_path);
                let path = if let Some(path) = raw_path.strip_prefix("~/") {
                    paths::home_dir().join(path)
                } else if path.is_relative() {
                    working_directory.join(path)
                } else {
                    path
                };

                let template = match std::fs::read_to_string(&path) {
                    Ok(s) if !s.trim().is_empty() => Some(s),
                    Err(err) => {
                        log::warn!("failed to read commit template {}: {}", path.display(), err);
                        None
                    }
                    _ => None,
                };

                Ok(template.map(|template| GitCommitTemplate { template }))
            })
            .boxed()
    }

    pub(super) fn repository_set_index_text(
        &self,
        path: RepoPath,
        content: Option<String>,
        env: Arc<HashMap<String, String>>,
        is_executable: bool,
    ) -> BoxFuture<'_, anyhow::Result<()>> {
        let git = self.git_binary();
        self.executor
            .spawn(async move {
                let mode = if is_executable { "100755" } else { "100644" };

                if let Some(content) = content {
                    let mut child = git
                        .build_command(&["hash-object", "-w", "--stdin"])
                        .envs(env.iter())
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .spawn()?;
                    let mut stdin = child.stdin.take().unwrap();
                    stdin.write_all(content.as_bytes()).await?;
                    stdin.flush().await?;
                    drop(stdin);
                    let output = child.output().await?.stdout;
                    let sha = str::from_utf8(&output)?.trim();

                    log::debug!("indexing SHA: {sha}, path {path:?}");

                    let output = git
                        .build_command(&["update-index", "--add", "--cacheinfo", mode, sha])
                        .envs(env.iter())
                        .arg(path.as_unix_str())
                        .output()
                        .await?;

                    anyhow::ensure!(
                        output.status.success(),
                        "Failed to stage:\n{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                } else {
                    log::debug!("removing path {path:?} from the index");
                    let output = git
                        .build_command(&["update-index", "--force-remove", "--"])
                        .envs(env.iter())
                        .arg(path.as_unix_str())
                        .output()
                        .await?;
                    anyhow::ensure!(
                        output.status.success(),
                        "Failed to unstage:\n{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }

                Ok(())
            })
            .boxed()
    }
}
