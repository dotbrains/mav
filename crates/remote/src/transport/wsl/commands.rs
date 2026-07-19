use super::*;

/// `wslpath` is a executable available in WSL, it's a linux binary.
/// So it doesn't support Windows style paths.
async fn sanitize_path(path: &Path) -> Result<String> {
    let path = smol::fs::canonicalize(path)
        .await
        .with_context(|| format!("Failed to canonicalize path {}", path.display()))?;
    let path_str = path.to_string_lossy();

    let sanitized = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);
    Ok(sanitized.replace('\\', "/"))
}

pub(super) fn run_wsl_command_with_output_impl(
    options: &WslConnectionOptions,
    program: &str,
    args: &[&str],
) -> impl Future<Output = Result<String>> + use<> {
    let exec_command = wsl_command_impl(options, program, args, true);
    let command = wsl_command_impl(options, program, args, false);
    async move {
        match run_wsl_command_impl(exec_command).await {
            Ok(res) => Ok(res),
            Err(exec_err) => match run_wsl_command_impl(command).await {
                Ok(res) => Ok(res),
                Err(e) => Err(e.context(exec_err)),
            },
        }
    }
}

impl WslConnectionOptions {
    pub fn abs_windows_path_to_wsl_path(
        &self,
        source: &Path,
    ) -> impl Future<Output = Result<String>> + use<> {
        let path_str = source.to_string_lossy();

        let source = path_str.strip_prefix(r"\\?\").unwrap_or(&*path_str);
        let source = source.replace('\\', "/");
        run_wsl_command_with_output_impl(self, "wslpath", &["-u", &source])
    }
}

pub(super) async fn windows_path_to_wsl_path_impl(
    options: &WslConnectionOptions,
    source: &Path,
) -> Result<String> {
    let source = sanitize_path(source).await?;
    run_wsl_command_with_output_impl(options, "wslpath", &["-u", &source]).await
}

/// Converts a WSL/POSIX path to a Windows path using `wslpath -w`.
///
/// For example, `/home/user/project` becomes `\\wsl.localhost\Ubuntu\home\user\project`
#[cfg(target_os = "windows")]
pub fn wsl_path_to_windows_path(
    options: &WslConnectionOptions,
    wsl_path: &Path,
) -> impl Future<Output = Result<PathBuf>> + use<> {
    let wsl_path_str = wsl_path.to_string_lossy().to_string();
    let command = wsl_command_impl(options, "wslpath", &["-w", &wsl_path_str], true);
    async move {
        let windows_path = run_wsl_command_impl(command).await?;
        Ok(PathBuf::from(windows_path))
    }
}

pub(super) fn run_wsl_command_impl(
    mut command: util::command::Command,
) -> impl Future<Output = Result<String>> {
    async move {
        let output = command
            .output()
            .await
            .with_context(|| format!("Failed to run command '{:?}'", command))?;

        if !output.status.success() {
            return Err(anyhow!(
                "Command '{:?}' failed: {}",
                command,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

/// Creates a new `wsl.exe` command that runs the given program with the given arguments.
///
/// If `exec` is true, the command will be executed in the WSL environment without spawning a new shell.
pub(super) fn wsl_command_impl(
    options: &WslConnectionOptions,
    program: &str,
    args: &[impl AsRef<OsStr>],
    exec: bool,
) -> util::command::Command {
    let mut command = util::command::new_command("wsl.exe");

    if let Some(user) = &options.user {
        command.arg("--user").arg(user);
    }

    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("--distribution")
        .arg(&options.distro_name)
        .arg("--cd")
        .arg("~");

    if exec {
        command.arg("--exec");
    }

    command.arg(program).args(args);

    log::debug!("wsl {:?}", command);
    command
}
