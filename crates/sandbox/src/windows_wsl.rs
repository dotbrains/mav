//! Windows sandbox integration via WSL.
//!
//! Sandboxed Windows terminal commands are routed through WSL and then executed
//! under Bubblewrap inside Linux. Projects may be opened either from native
//! Windows paths (`C:\Users\...`) or WSL UNC paths
//! (`\\wsl.localhost\Ubuntu\home\...`). Native drive-letter paths are
//! translated into the distro's filesystem view with `wslpath` (falling back
//! to the conventional `/mnt/<drive>/...` mapping if that fails) and use the
//! user's default WSL distro unless a WSL UNC path in the request pins a
//! specific distro.
//!
//! Errors fall into two classes the agent treats differently:
//!
//! * **Environment unavailable** — WSL missing or failing to start, no
//!   usable `bwrap`, or the probe/path-resolution stdout protocol breaking
//!   down. These are returned as a [`WslSandboxUnavailable`] (whose `Display`
//!   carries
//!   [`WSL_SANDBOX_UNAVAILABLE_PREFIX`](crate::WSL_SANDBOX_UNAVAILABLE_PREFIX)),
//!   so the agent recognizes them *by type* and offers the same
//!   retry / run-unsandboxed fallback it offers on Linux, rather than matching
//!   on message text.
//! * **Bad request** — a specific path that doesn't exist or can't be mapped
//!   into WSL, or a request mixing distros. These are ordinary `anyhow` errors
//!   *without* [`WslSandboxUnavailable`], and are reported back to the model,
//!   which can fix the request and retry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use smol::process::{Command, Stdio};

use anyhow::{Context as _, Result, bail, ensure};

use crate::WSL_SANDBOX_UNAVAILABLE_PREFIX;

/// Per-command relaxations of the WSL/Bubblewrap sandbox. Windows can only
/// toggle network access wholesale (no loopback-proxy confinement yet), so this
/// is a plain bool rather than the richer cross-platform [`crate::SandboxNetPolicy`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SandboxPermissions {
    pub(crate) allow_network: bool,
    pub(crate) allow_fs_write: bool,
}

/// Exit code the environment probe script uses to signal that `bwrap` is not
/// installed, distinguishing that from WSL itself failing to start a shell.
/// Chosen to be unlikely to collide with `wsl.exe`'s own failure codes.
const BWRAP_MISSING_EXIT_CODE: i32 = 41;

/// Exit code the environment probe script uses to signal that `bwrap` is
/// installed but failed the sandbox smoke test — typically because the
/// distro restricts unprivileged user namespaces (e.g. Ubuntu 24.04's
/// default AppArmor policy), which every namespace flag we pass depends on.
const BWRAP_UNUSABLE_EXIT_CODE: i32 = 42;

/// Prefix of the probe script's single result line, so it can be picked out
/// of any stdout noise printed by the login shell's profile scripts.
const PROBE_RESULT_PREFIX: &str = "mav-wsl-probe:";

/// Marks a failure of the Windows WSL sandboxing *environment*: WSL is missing
/// or won't start, there's no usable `bwrap`, or the probe / path-resolution
/// stdout protocol broke down. Returned as the root of the `anyhow::Error` so
/// callers classify it by type ([`anyhow::Error::downcast_ref`]) instead of by
/// matching message text. Per-request failures (a missing writable path, paths
/// mixing distros) are ordinary `anyhow` errors *without* this type, so they
/// never match — the agent returns those to the model rather than offering to
/// run unsandboxed.
#[derive(Debug, Clone)]
pub struct WslSandboxUnavailable(String);

impl WslSandboxUnavailable {
    /// Build an environment-unavailable error from a human-readable reason
    /// (without the [`WSL_SANDBOX_UNAVAILABLE_PREFIX`], which `Display` adds).
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// The reason, without the leading [`WSL_SANDBOX_UNAVAILABLE_PREFIX`].
    #[cfg(test)]
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WslSandboxUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{WSL_SANDBOX_UNAVAILABLE_PREFIX}: {}", self.0)
    }
}

impl std::error::Error for WslSandboxUnavailable {}

/// Shorthand for an [`anyhow::Error`] wrapping a [`WslSandboxUnavailable`].
fn unavailable(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(WslSandboxUnavailable::new(message))
}

/// Wrap a Linux process invocation so it runs under Bubblewrap inside WSL.
///
/// `program` and `args` must name a Linux executable and Linux argv, not a
/// Windows executable. The caller is expected to convert the model's command
/// into a Linux shell invocation (typically `/bin/sh -c ...`) before calling
/// this function.
///
/// All writable paths, Git paths, and the cwd must be paths that can be mapped
/// into WSL. The cwd and ordinary writable paths must exist; Git metadata paths
/// may be missing because Bubblewrap cannot bind a missing source. WSL UNC paths
/// may specify a distro; native drive-letter paths are translated with `wslpath`
/// inside either that distro or the default distro (falling back to
/// `/mnt/<drive>/...` if translation fails).
///
/// `env` is forwarded into the sandboxed command via `bwrap --setenv` rather
/// than being set on the `wsl.exe` process. Windows environment variables
/// don't cross the WSL boundary unless they're listed in `WSLENV`, so without
/// this the command would lose `PAGER` (used to stop `git` from paging into
/// the PTY) and the rest of the project environment. Variables whose Windows
/// values are meaningless or harmful inside Linux are dropped (see
/// [`is_forwardable_env_var`]).
///
/// This function performs up to two `wsl.exe` round-trips (environment probe
/// and path resolution, each cached) plus filesystem stats of WSL UNC paths,
/// any of which can take seconds when the WSL VM is cold (and the stats can
/// stall on a slow `\\wsl.localhost` filesystem). Run it on a background
/// executor, never on the UI thread, and bound it with a timeout — a wedged
/// `wsl.exe` (a real failure mode when the WSL service is unhealthy)
/// otherwise stalls the returned future forever. This crate deliberately has
/// no timer of its own (timers come from the caller's executor so tests stay
/// deterministic); instead it guarantees that dropping the future kills any
/// in-flight `wsl.exe` child, so a caller-side timeout that drops the future
/// also reaps the process. Parameters are owned so the returned future is
/// `Send + 'static`.
pub async fn wrap_invocation<S: std::hash::BuildHasher>(
    program: String,
    args: Vec<String>,
    writable_paths: Vec<PathBuf>,
    writable_git_paths: Vec<PathBuf>,
    protected_git_paths: Vec<PathBuf>,
    permissions: SandboxPermissions,
    cwd: Option<PathBuf>,
    env: HashMap<String, String, S>,
) -> Result<(String, Vec<String>)> {
    // Mapping failures are bad requests (a path that doesn't exist or has a
    // shape WSL can't address), not environment problems, so no
    // `WSL_SANDBOX_UNAVAILABLE_PREFIX` here.
    let cwd_mapping =
        match &cwd {
            Some(cwd) => Some(directory_to_wsl(cwd).with_context(|| {
                format!("failed to map terminal cwd `{}` into WSL", cwd.display())
            })?),
            None => None,
        };

    let writable_mappings = writable_paths
        .iter()
        .map(|path| {
            path_to_wsl(path).with_context(|| {
                format!("failed to map writable path `{}` into WSL", path.display())
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let writable_git_mappings = writable_git_paths
        .iter()
        .map(|path| {
            path_to_wsl_allowing_missing(path).with_context(|| {
                format!(
                    "failed to map writable Git metadata path `{}` into WSL",
                    path.display()
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let protected_git_mappings = protected_git_paths
        .iter()
        .map(|path| {
            path_to_wsl_allowing_missing(path).with_context(|| {
                format!(
                    "failed to map protected Git metadata path `{}` into WSL",
                    path.display()
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let distro = select_distro(
        cwd_mapping.as_ref(),
        writable_mappings
            .iter()
            .chain(writable_git_mappings.iter())
            .chain(protected_git_mappings.iter()),
    )?;
    let wsl_exe = wsl_exe_path();
    if !wsl_exe.is_file() {
        return Err(unavailable(format!(
            "WSL (`wsl.exe`) was not found at `{}`",
            wsl_exe.display()
        )));
    }
    let environment = probe_environment(&wsl_exe, distro.as_deref()).await?;

    // Resolve all paths (translating native drive-letter paths with `wslpath`
    // now that the distro is known) in a single WSL round-trip. The cwd and
    // ordinary writable paths must exist; Git paths may be missing because, as
    // with Linux bwrap, a not-yet-created `.git` placeholder can't be overlaid.
    let has_cwd = cwd_mapping.is_some();
    let writable_path_count = writable_mappings.len();
    let writable_git_path_count = writable_git_mappings.len();
    let mut mappings = Vec::with_capacity(
        writable_mappings.len() + writable_git_mappings.len() + protected_git_mappings.len() + 1,
    );
    if let Some(mapping) = cwd_mapping {
        mappings.push((mapping, "terminal cwd", true));
    }
    mappings.extend(
        writable_mappings
            .into_iter()
            .map(|mapping| (mapping, "writable path", true)),
    );
    mappings.extend(
        writable_git_mappings
            .into_iter()
            .map(|mapping| (mapping, "writable Git metadata path", false)),
    );
    mappings.extend(
        protected_git_mappings
            .into_iter()
            .map(|mapping| (mapping, "protected Git metadata path", false)),
    );
    let resolved = resolve_paths(&wsl_exe, distro.as_deref(), &mappings).await?;
    let (cwd, writable_paths, protected_git_paths) = split_resolved_paths(
        has_cwd,
        writable_path_count,
        writable_git_path_count,
        resolved,
    )?;

    let mut wsl_args = Vec::new();
    if let Some(distro) = distro.as_deref() {
        wsl_args.extend(["-d".to_string(), distro.to_string()]);
    }
    if let Some(cwd) = &cwd {
        wsl_args.extend(["--cd".to_string(), cwd.clone()]);
    }
    // Use the absolute path the probe validated: `wsl --exec` searches only
    // the default WSL PATH, which may not include a profile-managed location
    // where the probe's login shell found `bwrap`.
    wsl_args.extend(["--exec".to_string(), environment.bwrap_path.clone()]);
    wsl_args.extend(build_bwrap_args(
        &writable_paths,
        &protected_git_paths,
        permissions,
        cwd.as_deref(),
        environment.mask_interop_dir,
        &env,
    ));
    wsl_args.push("--".to_string());
    wsl_args.push(program);
    wsl_args.extend(args);

    Ok((wsl_exe.to_string_lossy().into_owned(), wsl_args))
}

mod bwrap;
mod command;
mod distro;
mod path_mapping;
mod path_resolution;
mod probe;

use bwrap::*;
use command::*;
use distro::*;
use path_mapping::*;
use path_resolution::*;
use probe::*;

#[cfg(test)]
mod bwrap_tests;
#[cfg(test)]
mod distro_tests;
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod path_tests;
#[cfg(test)]
mod probe_tests;
#[cfg(test)]
mod resolution_tests;
#[cfg(test)]
mod send;
