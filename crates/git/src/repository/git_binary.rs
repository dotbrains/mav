use super::*;

pub(crate) struct GitBinary {
    pub(super) git_binary_path: PathBuf,
    pub(super) working_directory: PathBuf,
    pub(super) git_directory: PathBuf,
    pub(super) executor: BackgroundExecutor,
    pub(super) index_file_path: Option<PathBuf>,
    pub(super) envs: HashMap<String, String>,
    pub(super) is_trusted: bool,
}

impl GitBinary {
    pub(crate) fn new(
        git_binary_path: PathBuf,
        working_directory: PathBuf,
        git_directory: PathBuf,
        executor: BackgroundExecutor,
        is_trusted: bool,
    ) -> Self {
        Self {
            git_binary_path,
            working_directory,
            git_directory,
            executor,
            index_file_path: None,
            envs: HashMap::default(),
            is_trusted,
        }
    }

    pub(super) async fn list_untracked_files(&self) -> Result<Vec<PathBuf>> {
        let status_output = self
            .run(&["status", "--porcelain=v1", "--untracked-files=all", "-z"])
            .await?;

        let paths = status_output
            .split('\0')
            .filter(|entry| entry.len() >= 3 && entry.starts_with("?? "))
            .map(|entry| PathBuf::from(&entry[3..]))
            .collect::<Vec<_>>();
        Ok(paths)
    }

    pub(super) fn envs(mut self, envs: HashMap<String, String>) -> Self {
        self.envs = envs;
        self
    }

    pub async fn with_temp_index<R>(
        &mut self,
        f: impl AsyncFnOnce(&Self) -> Result<R>,
    ) -> Result<R> {
        let index_file_path = self.path_for_index_id(Uuid::new_v4());

        let delete_temp_index = util::defer({
            let index_file_path = index_file_path.clone();
            let executor = self.executor.clone();
            move || {
                executor
                    .spawn(async move {
                        smol::fs::remove_file(index_file_path).await.log_err();
                    })
                    .detach();
            }
        });

        // Copy the default index file so that Git doesn't have to rebuild the
        // whole index from scratch. This might fail if this is an empty repository.
        smol::fs::copy(self.git_directory.join("index"), &index_file_path)
            .await
            .ok();

        self.index_file_path = Some(index_file_path.clone());
        let result = f(self).await;
        self.index_file_path = None;
        let result = result?;

        smol::fs::remove_file(index_file_path).await.ok();
        delete_temp_index.abort();

        Ok(result)
    }

    pub async fn with_exclude_overrides(&self) -> Result<GitExcludeOverride> {
        let path = self.git_directory.join("info").join("exclude");

        GitExcludeOverride::new(path).await
    }

    fn path_for_index_id(&self, id: Uuid) -> PathBuf {
        self.git_directory.join(format!("index-{}.tmp", id))
    }

    pub async fn run<S>(&self, args: &[S]) -> Result<String>
    where
        S: AsRef<OsStr>,
    {
        let mut stdout = self.run_raw(args).await?;
        if stdout.chars().last() == Some('\n') {
            stdout.pop();
        }
        Ok(stdout)
    }

    /// Returns the result of the command without trimming the trailing newline.
    pub async fn run_raw<S>(&self, args: &[S]) -> Result<String>
    where
        S: AsRef<OsStr>,
    {
        let mut command = self.build_command(args);
        let output = command.output().await?;
        anyhow::ensure!(
            output.status.success(),
            GitBinaryCommandError {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                status: output.status,
            }
        );
        Ok(String::from_utf8(output.stdout)?)
    }

    #[allow(clippy::disallowed_methods)]
    pub(crate) fn build_command<S>(&self, args: &[S]) -> util::command::Command
    where
        S: AsRef<OsStr>,
    {
        let mut command = new_command(&self.git_binary_path);
        command.current_dir(&self.working_directory);
        // Disabled to stop malicious actors from running arbitrary commands via fsmonitor hooks
        command.args(["-c", "core.fsmonitor=false"]);
        // Prepended signature lines would corrupt our --format parsers.
        command.args(["-c", "log.showSignature=false"]);
        command.arg("--no-optional-locks");
        // Internal commands must be non-interactive so background tasks never block on user input.
        command.arg("--no-pager");

        if !self.is_trusted {
            command.args(["-c", "core.hooksPath=/dev/null"]);
            command.args(["-c", "core.sshCommand=ssh"]);
            command.args(["-c", "credential.helper="]);
            command.args(["-c", "protocol.ext.allow=never"]);
            command.args(["-c", "diff.external="]);
        }
        command.args(args);

        // If the `diff` command is being used, we'll want to add the
        // `--no-ext-diff` flag when working on an untrusted repository,
        // preventing any external diff programs from being invoked.
        if !self.is_trusted && args.iter().any(|arg| arg.as_ref() == "diff") {
            command.arg("--no-ext-diff");
        }

        if let Some(index_file_path) = self.index_file_path.as_ref() {
            command.env("GIT_INDEX_FILE", index_file_path);
        }
        command.envs(&self.envs);
        command
    }
}

#[derive(Error, Debug)]
#[error("Git command failed:\n{stdout}{stderr}\n")]
pub(super) struct GitBinaryCommandError {
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) status: ExitStatus,
}

pub(super) async fn run_git_command(
    env: Arc<HashMap<String, String>>,
    ask_pass: AskPassDelegate,
    mut command: util::command::Command,
    executor: BackgroundExecutor,
) -> Result<RemoteCommandOutput> {
    if env.contains_key("GIT_ASKPASS") {
        let git_process = command.spawn()?;
        let output = git_process.output().await?;
        anyhow::ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(RemoteCommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    } else {
        let ask_pass = AskPassSession::new(executor, ask_pass).await?;
        command
            .env("GIT_ASKPASS", ask_pass.script_path())
            .env("SSH_ASKPASS", ask_pass.script_path())
            .env("SSH_ASKPASS_REQUIRE", "force");
        #[cfg(target_os = "windows")]
        command.env("MAV_ASKPASS_SOCKET", ask_pass.socket_path());
        let git_process = command.spawn()?;

        run_askpass_command(ask_pass, git_process).await
    }
}

async fn run_askpass_command(
    mut ask_pass: AskPassSession,
    git_process: util::command::Child,
) -> anyhow::Result<RemoteCommandOutput> {
    select_biased! {
        result = ask_pass.run().fuse() => {
            match result {
                AskPassResult::CancelledByUser => {
                    Err(anyhow!(REMOTE_CANCELLED_BY_USER))?
                }
                AskPassResult::Timedout => {
                    Err(anyhow!("Connecting to host timed out"))?
                }
            }
        }
        output = git_process.output().fuse() => {
            let output = output?;
            anyhow::ensure!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(RemoteCommandOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            })
        }
    }
}
