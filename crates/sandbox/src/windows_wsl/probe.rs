use super::*;

/// What [`probe_environment`] learned about a WSL distro.
#[derive(Clone, Debug, Eq, PartialEq)]
struct EnvironmentProbe {
    /// Whether the WSL interop socket directory (`/run/WSL`) exists and so
    /// must (and can) be masked — see [`build_bwrap_args`].
    mask_interop_dir: bool,
    /// Absolute path of the `bwrap` binary the smoke test validated. The real
    /// invocation must exec this exact path: `wsl --exec` searches only the
    /// default WSL PATH, so a bare `bwrap` could miss (or differ from) the
    /// binary the probe's login shell found.
    bwrap_path: String,
}

/// Shell script run by [`probe_environment`]. Resolves `bwrap` to an absolute
/// path (exit [`BWRAP_MISSING_EXIT_CODE`] if absent), rejects setuid-root
/// binaries, then smoke-tests a real minimal sandbox (exit
/// [`BWRAP_UNUSABLE_EXIT_CODE`] on failure) using the same mount and namespace
/// flags as [`build_bwrap_args`] — presence isn't
/// enough, because unprivileged user namespaces can be disabled by the
/// distro's kernel, sysctl, or AppArmor policy (notably Ubuntu 24.04, the
/// current default WSL distro), in which case `bwrap` exists but every
/// sandboxed command would fail. The interop mask is included in the smoke
/// test when `/run/WSL` exists so the exact mount we later perform is
/// exercised too. On success, one [`PROBE_RESULT_PREFIX`]-marked result line
/// reports the interop state and the resolved `bwrap` path.
fn probe_script() -> String {
    format!(
        "bwrap_path=$(command -v bwrap) || exit {BWRAP_MISSING_EXIT_CODE}; \
         if [ -u \"$bwrap_path\" ] && [ \"$(stat -c %u \"$bwrap_path\" 2>/dev/null)\" = 0 ]; then \
         echo 'setuid-root bwrap is not supported' >&2; \
         exit {BWRAP_UNUSABLE_EXIT_CODE}; fi; \
         if [ -d /run/WSL ]; then interop=interop; mask='--tmpfs /run/WSL'; \
         else interop=no-interop; mask=''; fi; \
         \"$bwrap_path\" --ro-bind / / --tmpfs /tmp $mask --dev /dev --proc /proc \
         --unshare-net --unshare-user --unshare-ipc --unshare-uts --unshare-pid \
         --unshare-cgroup-try --die-with-parent -- true >/dev/null \
         || exit {BWRAP_UNUSABLE_EXIT_CODE}; \
         printf '{PROBE_RESULT_PREFIX} %s %s\\n' \"$interop\" \"$bwrap_path\""
    )
}

/// Probe a distro's sandbox environment in one `wsl.exe` round-trip: confirm
/// a shell starts, confirm `bwrap` is installed *and can actually set up an
/// unprivileged sandbox* (see [`probe_script`]), and report whether the
/// interop socket directory exists.
///
/// Successful results are cached per distro for the life of the process —
/// like `linux_bubblewrap::is_available`, the answers can't realistically
/// change while Mav runs. Failures are deliberately *not* cached so a user
/// who installs `bwrap` (or lifts a user-namespace restriction) after seeing
/// the error can retry the command without restarting Mav.
async fn probe_environment(wsl_exe: &Path, distro: Option<&str>) -> Result<EnvironmentProbe> {
    static CACHE: OnceLock<Mutex<HashMap<Option<String>, EnvironmentProbe>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let key = distro.map(str::to_string);
    if let Some(probe) = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&key)
    {
        return Ok(probe.clone());
    }

    // A login shell (`-lc`) is used so a `bwrap` reachable only through a
    // profile-managed PATH is still found; the resolved absolute path is
    // reported back so the real invocation execs the same binary.
    let script = probe_script();
    let output = run_wsl_command(
        wsl_exe,
        distro,
        ["--exec", "sh", "-lc", &script],
        "probe the sandbox environment",
    )
    .await?;
    if output.status.code() == Some(BWRAP_MISSING_EXIT_CODE) {
        return Err(unavailable(format!(
            "Bubblewrap (`bwrap`) is not installed in {}",
            wsl_distro_label(distro)
        )));
    }
    if output.status.code() == Some(BWRAP_UNUSABLE_EXIT_CODE) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        return Err(unavailable(format!(
            "Bubblewrap (`bwrap`) is installed in {} but could not set up a sandbox — the \
             distro may restrict unprivileged user namespaces (as Ubuntu 24.04's default \
             AppArmor policy does){}",
            wsl_distro_label(distro),
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        )));
    }
    if !output.status.success() {
        return Err(unavailable(format!(
            "failed to start a shell in {}{}",
            wsl_distro_label(distro),
            command_failure_details(output.status.code(), &output.stderr)
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let probe = parse_probe_output(&stdout).map_err(|error| {
        unavailable(format!(
            "unexpected sandbox probe output from {}: {error:#}",
            wsl_distro_label(distro)
        ))
    })?;
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key, probe.clone());
    Ok(probe)
}

/// Parse [`probe_script`] output: the last [`PROBE_RESULT_PREFIX`]-marked
/// line wins, so stdout noise from login-shell profile scripts (which runs
/// before the script body) is ignored.
fn parse_probe_output(stdout: &str) -> Result<EnvironmentProbe> {
    let line = stdout
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(PROBE_RESULT_PREFIX))
        .with_context(|| format!("no probe result line in: {stdout:?}"))?;
    let (interop, bwrap_path) = line
        .trim_start()
        .split_once(' ')
        .with_context(|| format!("malformed probe result line: {line:?}"))?;
    let mask_interop_dir = match interop {
        "interop" => true,
        "no-interop" => false,
        _ => bail!("malformed probe result line: {line:?}"),
    };
    ensure!(
        bwrap_path.starts_with('/'),
        "`bwrap` resolved to {bwrap_path:?} rather than an absolute path; a shell \
         alias or function named `bwrap` cannot be run with `wsl --exec`"
    );
    Ok(EnvironmentProbe {
        mask_interop_dir,
        bwrap_path: bwrap_path.to_string(),
    })
}
