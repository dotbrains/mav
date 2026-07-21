use super::input::TerminalSandboxInput;
use crate::sandboxing::NetworkRequest;
use anyhow::Result;
use gpui::{App, Entity};
use project::Project;
use std::path::{Path, PathBuf};

pub(super) fn resolve_write_paths(
    raw_paths: &[String],
    working_dir: Option<&Path>,
    project: &Entity<Project>,
    cx: &App,
) -> Vec<PathBuf> {
    if raw_paths.is_empty() {
        return Vec::new();
    }
    let project = project.read(cx);
    let windows_paths = project.path_style(cx).is_windows();
    let base = working_dir.map(Path::to_path_buf).or_else(|| {
        project
            .worktrees(cx)
            .next()
            .map(|worktree| worktree.read(cx).abs_path().to_path_buf())
    });
    join_write_paths(raw_paths, base.as_deref(), windows_paths)
}

/// Pure path-joining step of [`resolve_write_paths`], split out so it can be
/// unit-tested without a `Project`/`App`.
///
/// Each path is lexically normalized (resolving `.`/`..`) so that later
/// subtree-containment checks and the user-facing approval prompt operate on
/// the same path the sandbox will ultimately enforce. Relative paths with no
/// base, and paths that traverse above the filesystem root, are dropped.
///
/// On Windows, raw paths the model expressed in WSL terms (a `/mnt/<drive>/...`
/// automount path, or a WSL-absolute `/home/...` path) are mapped back to the
/// form the sandbox machinery expects before normalization.
pub(super) fn join_write_paths(
    raw_paths: &[String],
    base: Option<&Path>,
    windows_paths: bool,
) -> Vec<PathBuf> {
    raw_paths
        .iter()
        .filter_map(|raw| {
            if windows_paths {
                if let Some(path) = wsl_drive_mount_path_to_windows_path(raw) {
                    return Some(path);
                }
                if let Some(path) = wsl_absolute_path(raw) {
                    return Some(path);
                }
            }

            let path = Path::new(raw);
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                base?.join(path)
            };
            util::paths::normalize_lexically(&absolute).ok()
        })
        .collect()
}

fn wsl_drive_mount_path_to_windows_path(raw: &str) -> Option<PathBuf> {
    let raw = raw.replace('\\', "/");
    let remainder = raw.strip_prefix("/mnt/")?;
    let (drive, rest) = remainder
        .split_once('/')
        .map_or((remainder, ""), |(drive, rest)| (drive, rest));
    let mut drive_chars = drive.chars();
    let drive = drive_chars.next()?.to_ascii_uppercase();
    if !drive.is_ascii_alphabetic() || drive_chars.next().is_some() {
        return None;
    }

    let mut windows_path = format!("{drive}:\\");
    if !rest.is_empty() {
        windows_path.push_str(&rest.replace('/', "\\"));
    }
    Some(PathBuf::from(windows_path))
}

fn wsl_absolute_path(raw: &str) -> Option<PathBuf> {
    let raw = raw.replace('\\', "/");
    if raw.starts_with('/') && !raw.starts_with("//") {
        Some(PathBuf::from(raw))
    } else {
        None
    }
}

/// Convert a (validated) network request into the access mode enforced by the
/// terminal sandbox.
pub(super) fn network_request_to_sandbox_network_access(
    network: &NetworkRequest,
) -> acp_thread::SandboxNetworkAccess {
    match network {
        NetworkRequest::None => acp_thread::SandboxNetworkAccess::None,
        NetworkRequest::AnyHost => acp_thread::SandboxNetworkAccess::All,
        NetworkRequest::Hosts(hosts) => {
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            {
                acp_thread::SandboxNetworkAccess::Restricted(http_proxy::Allowlist::from_patterns(
                    hosts.iter().cloned(),
                ))
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            {
                let _ = hosts;
                acp_thread::SandboxNetworkAccess::None
            }
        }
    }
}

/// Parse and validate the model's network escalation request. `allow_all_hosts`
/// subsumes any specific `allow_hosts` list. Returns an error string suitable
/// for showing back to the model when a host pattern is malformed.
pub(super) fn build_network_request(
    sandbox: &TerminalSandboxInput,
) -> Result<NetworkRequest, String> {
    if sandbox.allow_all_hosts == Some(true) {
        return Ok(NetworkRequest::AnyHost);
    }
    if sandbox.allow_hosts.is_empty() {
        return Ok(NetworkRequest::None);
    }
    let mut patterns = Vec::with_capacity(sandbox.allow_hosts.len());
    for raw in &sandbox.allow_hosts {
        match http_proxy::HostPattern::parse(raw) {
            Ok(pattern) => patterns.push(pattern),
            Err(error) => {
                return Err(format!(
                    "`allow_hosts` contains an invalid pattern '{raw}': {error}. \
                     Hostnames only — no IP literals; leading-`*.` wildcards \
                     are supported (e.g. `*.example.com`)."
                ));
            }
        }
    }
    Ok(NetworkRequest::Hosts(patterns))
}
