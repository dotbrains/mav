use super::*;

/// `CREATE_NO_WINDOW` process creation flag. `wsl.exe` is a console-subsystem
/// binary, so spawning it from a GUI process without this flag flashes a
/// console window. Defined locally because this crate doesn't depend on
/// `util` (whose command helpers normally take care of this).
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Invoke `wsl.exe` with the given args and return its raw output.
///
/// Only spawn failures become errors here; callers interpret the exit status
/// themselves. stdout, when used, is decoded as UTF-8 (lossily) — that's
/// only valid for `--exec`'d programs whose output we control, not for
/// `wsl.exe`'s own diagnostics (which are UTF-16LE).
///
/// `output()` spawns the child eagerly and the returned future owns it, so
/// with `kill_on_drop` the child can't outlive this future: a caller-side
/// timeout or cancellation that drops us also terminates a wedged `wsl.exe`
/// instead of leaking it.
async fn run_wsl_command(
    wsl_exe: &Path,
    distro: Option<&str>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    description: &str,
) -> Result<std::process::Output> {
    use smol::process::windows::CommandExt as _;

    let mut command = Command::new(wsl_exe);
    if let Some(distro) = distro {
        command.args(["-d", distro]);
    }
    command
        .args(args)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .creation_flags(CREATE_NO_WINDOW);

    command.output().await.map_err(|error| {
        unavailable(format!(
            "failed to invoke WSL while trying to {description}: {error:#}"
        ))
    })
}

fn command_failure_details(exit_code: Option<i32>, stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stderr = stderr.trim();
    let exit_status = match exit_code {
        Some(code) => format!("exit code {code}"),
        None => "terminated by signal".to_string(),
    };
    if stderr.is_empty() {
        format!(" ({exit_status})")
    } else {
        format!(" ({exit_status}; stderr: {stderr})")
    }
}

fn wsl_distro_label(distro: Option<&str>) -> String {
    match distro {
        Some(distro) => format!("WSL distro `{distro}`"),
        None => "the default WSL distro".to_string(),
    }
}

fn wsl_exe_path() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join("wsl.exe")
}
