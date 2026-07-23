use super::*;

fn build_bwrap_args<S: std::hash::BuildHasher>(
    writable_paths: &[String],
    protected_git_paths: &[String],
    permissions: SandboxPermissions,
    cwd: Option<&str>,
    mask_interop_dir: bool,
    env: &HashMap<String, String, S>,
) -> Vec<String> {
    let mut args = Vec::new();

    if permissions.allow_fs_write {
        push_bind(&mut args, "--bind", "/", "/");
    } else {
        push_bind(&mut args, "--ro-bind", "/", "/");
        args.extend(["--tmpfs".to_string(), "/tmp".to_string()]);
        for path in writable_paths {
            push_bind(&mut args, "--bind", path, path);
        }
        // Protect Git metadata by re-binding it read-only over the writable
        // worktree binds above (order matters: later binds win). When Git access
        // is granted these paths are included in `writable_paths` instead.
        for path in protected_git_paths {
            push_bind(&mut args, "--ro-bind", path, path);
        }
    }

    // Block WSL's Windows interop, regardless of the requested permissions.
    // Without this, a sandboxed process can exec a Windows binary (e.g.
    // /mnt/c/Windows/System32/cmd.exe), which the kernel's binfmt handler
    // (`/init`) hands off to the Windows host over an AF_UNIX socket — running
    // fully outside bwrap and defeating both the filesystem and the network
    // restrictions. `/init` locates that socket via the $WSL_INTEROP
    // environment variable, so we drop it; and we mask the socket directory
    // (when it exists) so the value can't be rediscovered by listing
    // /run/WSL and re-exporting it. Both steps are required: unsetting the
    // variable alone is bypassable, and masking alone leaves the inherited
    // variable usable.
    args.extend(["--unsetenv".to_string(), "WSL_INTEROP".to_string()]);
    args.extend(["--unsetenv".to_string(), "WSLENV".to_string()]);
    if mask_interop_dir {
        args.extend(["--tmpfs".to_string(), "/run/WSL".to_string()]);
    }

    args.extend([
        "--dev".to_string(),
        "/dev".to_string(),
        "--proc".to_string(),
        "/proc".to_string(),
    ]);

    if !permissions.allow_network {
        args.push("--unshare-net".to_string());
    }

    args.extend([
        "--unshare-user".to_string(),
        "--unshare-ipc".to_string(),
        "--unshare-uts".to_string(),
        "--unshare-pid".to_string(),
        "--unshare-cgroup-try".to_string(),
        "--die-with-parent".to_string(),
    ]);

    // Forward the caller-provided environment into the command. Windows env
    // set on the `wsl.exe` process doesn't reach the Linux command, so we
    // re-apply it here on the sandbox's child instead.
    for (name, value) in env {
        if is_forwardable_env_var(name) {
            args.extend(["--setenv".to_string(), name.clone(), value.clone()]);
        }
    }

    if let Some(cwd) = cwd {
        args.extend(["--chdir".to_string(), cwd.to_string()]);
    }

    args
}

/// Whether an environment variable should be forwarded into the Linux sandbox.
///
/// `bwrap --setenv` calls `setenv(3)`, which rejects names that are empty or
/// contain `=`. Windows process environments include such entries — most
/// notably the per-drive current-directory pseudo-variables (`=C:`, `=D:`,
/// ...) Windows keeps in the environment block — so they must be skipped or
/// bwrap aborts with "setenv failed".
///
/// Beyond that, many variables hold Windows-specific values that would be
/// meaningless or actively break the command inside WSL, so they are dropped
/// rather than forwarded: `PATH`/`PATHEXT` would shadow WSL's own `PATH` and
/// stop the shell from finding Linux executables, the temp-dir variables point
/// at Windows paths that don't exist in WSL (bwrap provides a fresh tmpfs
/// `/tmp` instead), the WSL interop variables would undermine the explicit
/// interop block above, a Windows-set `HOME` would clobber the distro's own
/// `$HOME` and break the shell, and the rest are Windows system locations,
/// shell settings, host/session identity, and CPU descriptors that are either
/// wrong or misleading inside Linux. This is a blocklist rather than an allowlist so
/// genuinely portable variables (e.g. `LANG`, `CARGO_TERM_COLOR`, `PAGER`)
/// still reach the command; it only needs to cover the variables Windows
/// populates by default. Matched case-insensitively because Windows
/// environment variable names are.
fn is_forwardable_env_var(name: &str) -> bool {
    if name.is_empty() || name.contains('=') {
        return false;
    }
    const BLOCKED: &[&str] = &[
        // Shadow or break the Linux process environment.
        "PATH",
        "PATHEXT",
        "TMPDIR",
        "TMP",
        "TEMP",
        // Would undermine the explicit WSL interop block above.
        "WSL_INTEROP",
        "WSLENV",
        // Windows system locations and shell, meaningless inside Linux.
        "OS",
        "COMSPEC",
        "WINDIR",
        "SYSTEMROOT",
        "SYSTEMDRIVE",
        // When Windows has `HOME` set (e.g. for git), it's a Windows path that
        // would clobber the distro's correct Linux `$HOME` and break the shell.
        "HOME",
        "HOMEDRIVE",
        "HOMEPATH",
        "HOMESHARE",
        "USERPROFILE",
        "PUBLIC",
        "ALLUSERSPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "PROGRAMDATA",
        "PROGRAMFILES",
        "PROGRAMFILES(X86)",
        "PROGRAMW6432",
        "COMMONPROGRAMFILES",
        "COMMONPROGRAMFILES(X86)",
        "COMMONPROGRAMW6432",
        "PSMODULEPATH",
        "DRIVERDATA",
        "ONEDRIVE",
        // Windows host/session identity, misleading inside Linux.
        "COMPUTERNAME",
        "USERNAME",
        "USERDOMAIN",
        "USERDOMAIN_ROAMINGPROFILE",
        "LOGONSERVER",
        "SESSIONNAME",
        // Windows CPU descriptors.
        "NUMBER_OF_PROCESSORS",
        "PROCESSOR_ARCHITECTURE",
        "PROCESSOR_ARCHITEW6432",
        "PROCESSOR_IDENTIFIER",
        "PROCESSOR_LEVEL",
        "PROCESSOR_REVISION",
    ];
    !BLOCKED
        .iter()
        .any(|blocked| name.eq_ignore_ascii_case(blocked))
}

fn push_bind(args: &mut Vec<String>, flag: &str, source: &str, destination: &str) {
    args.extend([
        flag.to_string(),
        source.to_string(),
        destination.to_string(),
    ]);
}

fn directory_to_wsl(path: &Path) -> Result<PathMapping> {
    ensure!(
        path.is_dir(),
        "Windows sandboxing via WSL can only use an existing directory as cwd: {}",
        path.display()
    );
    map_path_to_wsl(path)
}

fn path_to_wsl(path: &Path) -> Result<PathMapping> {
    let path_string = path.to_string_lossy();
    if let Ok(path) = parse_wsl_absolute_path(&path_string) {
        return Ok(PathMapping::Wsl(path));
    }

    ensure!(
        path.is_dir() || path.is_file(),
        "Windows sandboxing via WSL can only grant existing files or directories: {}",
        path.display()
    );
    map_path_to_wsl(path)
}

fn path_to_wsl_allowing_missing(path: &Path) -> Result<PathMapping> {
    let path_string = path.to_string_lossy();
    if let Ok(path) = parse_wsl_absolute_path(&path_string) {
        return Ok(PathMapping::Wsl(path));
    }
    map_path_to_wsl(path)
}

fn map_path_to_wsl(path: &Path) -> Result<PathMapping> {
    let path_string = path.to_string_lossy();
    if let Ok(path) = parse_wsl_unc_path(&path_string) {
        return Ok(PathMapping::Wsl(path));
    }
    let fallback = parse_native_drive_path(&path_string)?;
    let windows_path = path_string
        .strip_prefix(r"\\?\")
        .unwrap_or(&path_string)
        .replace('\\', "/");
    Ok(PathMapping::NativeDrive {
        windows_path,
        fallback,
    })
}

fn parse_wsl_absolute_path(path: &str) -> Result<WslPath> {
    let path = path.replace('\\', "/");
    ensure!(
        path.starts_with('/') && !path.starts_with("//"),
        "path is not a WSL absolute path: {path}"
    );
    Ok(WslPath { distro: None, path })
}

fn parse_wsl_unc_path(path: &str) -> Result<WslPath> {
    let path = path.replace('/', "\\");
    let remainder = path
        .strip_prefix("\\\\wsl.localhost\\")
        .or_else(|| path.strip_prefix("\\\\wsl$\\"))
        .or_else(|| path.strip_prefix("\\\\?\\UNC\\wsl.localhost\\"))
        .or_else(|| path.strip_prefix("\\\\?\\UNC\\wsl$\\"))
        .with_context(|| format!("path is not a WSL UNC path: {path}"))?;

    let (distro, rest) = remainder
        .split_once('\\')
        .map(|(distro, rest)| (distro, Some(rest)))
        .unwrap_or((remainder, None));
    ensure!(
        !distro.is_empty(),
        "WSL UNC path is missing a distro name: {path}"
    );

    let linux_path = match rest {
        Some(rest) if !rest.is_empty() => format!("/{}", rest.replace('\\', "/")),
        _ => "/".to_string(),
    };

    Ok(WslPath {
        distro: Some(distro.to_string()),
        path: linux_path,
    })
}

fn parse_native_drive_path(path: &str) -> Result<WslPath> {
    let path = path
        .strip_prefix("\\\\?\\")
        .unwrap_or(path)
        .replace('\\', "/");
    let mut chars = path.chars();
    let Some(drive) = chars.next().filter(|drive| drive.is_ascii_alphabetic()) else {
        bail!("path is not a drive-letter Windows path: {path}");
    };
    ensure!(chars.next() == Some(':'), "path is not absolute: {path}");
    let rest = chars.as_str().trim_start_matches('/');
    let drive = drive.to_ascii_lowercase();
    let linux_path = if rest.is_empty() {
        format!("/mnt/{drive}")
    } else {
        format!("/mnt/{drive}/{rest}")
    };
    Ok(WslPath {
        distro: None,
        path: linux_path,
    })
}
