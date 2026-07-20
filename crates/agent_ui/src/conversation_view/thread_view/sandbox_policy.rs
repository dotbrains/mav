use super::*;

/// Fold the always-granted baseline writable paths (the project's worktree
/// roots, derived from the same source the terminal tool uses) and, on Linux,
/// the host-isolated `/tmp` overlay into a settings policy for display. These
/// are part of what the sandbox grants whenever it's active but aren't
/// persistent-settings entries, so they're shown in the "from your settings"
/// section rather than stored. A no-op when the fs is unrestricted (rendered as
/// "All paths"), since there's nothing to scope.
pub(super) fn augment_settings_sandbox_policy(
    mut policy: SandboxPolicy,
    baseline: Vec<PathBuf>,
) -> SandboxPolicy {
    if let SandboxFsPolicy::Restricted { writable_paths } = &mut policy.fs {
        let mut merged = baseline;
        for path in writable_paths.drain(..) {
            if !merged.contains(&path) {
                merged.push(path);
            }
        }
        // The ephemeral, host-isolated tmpfs at /tmp is Linux-specific (the
        // bwrap `--tmpfs /tmp` overlay). It's a display-only label, not a real
        // host path, so it can't come from the path source above.
        #[cfg(target_os = "linux")]
        merged.push(PathBuf::from("/tmp (isolated)"));
        *writable_paths = merged;
    }
    policy
}

pub(super) fn sandbox_section(
    title: &str,
    policy: &SandboxPolicy,
    show_empty: bool,
) -> SandboxSection {
    let write_empty = fs_grants_nothing(&policy.fs);
    let network_empty = network_grants_nothing(&policy.network);
    let git_empty = git_grants_nothing(&policy.git);

    let mut section = SandboxSection::new(title.to_string());

    if show_empty || !write_empty {
        section =
            section.group(SandboxGroup::new("Write Access").rows(sandbox_fs_rows(&policy.fs)));
    }

    if show_empty || !network_empty {
        section = section
            .group(SandboxGroup::new("Network Access").rows(sandbox_network_rows(&policy.network)));
    }

    if !git_empty {
        section = section
            .group(SandboxGroup::new("Git Metadata Access").rows(sandbox_git_rows(&policy.git)));
    }

    section
}

/// Whether a policy grants nothing worth surfacing, used to decide whether to
/// show the per-thread overrides section at all.
pub(super) fn sandbox_policy_grants_nothing(policy: &SandboxPolicy) -> bool {
    fs_grants_nothing(&policy.fs)
        && network_grants_nothing(&policy.network)
        && git_grants_nothing(&policy.git)
}

/// Git access grants nothing to surface unless `.git` writes are allowed *and*
/// at least one `.git` directory is known.
fn git_grants_nothing(git: &GitSandboxPolicy) -> bool {
    !git.allows_writes() || git.git_dirs().is_empty()
}

/// Rows for the Git-access group: one row per writable `.git` directory (these
/// may live outside the project for a linked worktree).
fn sandbox_git_rows(git: &GitSandboxPolicy) -> Vec<SandboxRow> {
    match git {
        GitSandboxPolicy::Allowed { git_dirs } if !git_dirs.is_empty() => git_dirs
            .iter()
            .map(|path| SandboxRow::git(path.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

fn fs_grants_nothing(fs: &SandboxFsPolicy) -> bool {
    matches!(fs, SandboxFsPolicy::Restricted { writable_paths } if writable_paths.is_empty())
}

fn network_grants_nothing(network: &SandboxNetPolicy) -> bool {
    match network {
        SandboxNetPolicy::Blocked => true,
        SandboxNetPolicy::Restricted { allowed_domains } => allowed_domains.is_empty(),
        SandboxNetPolicy::Unrestricted => false,
    }
}

/// Rows for the write-access group: a message for the "all"/"none" cases, or one
/// row per granted path.
fn sandbox_fs_rows(fs: &SandboxFsPolicy) -> Vec<SandboxRow> {
    match fs {
        SandboxFsPolicy::Unrestricted => vec![SandboxRow::message("All paths (unrestricted)")],
        SandboxFsPolicy::Restricted { writable_paths } if writable_paths.is_empty() => {
            vec![SandboxRow::message("None")]
        }
        SandboxFsPolicy::Restricted { writable_paths } => writable_paths
            .iter()
            .map(|path| SandboxRow::path(path.clone()))
            .collect(),
    }
}

/// Rows for the network-access group: a message for the "all"/"none" cases, or
/// one row per allowed domain.
fn sandbox_network_rows(network: &SandboxNetPolicy) -> Vec<SandboxRow> {
    match network {
        SandboxNetPolicy::Unrestricted => vec![SandboxRow::message("All domains (unrestricted)")],
        SandboxNetPolicy::Blocked => vec![SandboxRow::message("None")],
        SandboxNetPolicy::Restricted { allowed_domains } if allowed_domains.is_empty() => {
            vec![SandboxRow::message("None")]
        }
        SandboxNetPolicy::Restricted { allowed_domains } => allowed_domains
            .iter()
            .map(|domain| SandboxRow::domain(domain.clone()))
            .collect(),
    }
}
