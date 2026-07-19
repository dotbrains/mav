use std::path::PathBuf;

use gpui::{App, ReadGlobal as _};
use http_proxy::HostPattern;
use settings::SettingsStore;

pub(super) fn raw_sandbox_lists(cx: &App) -> (Vec<String>, Vec<PathBuf>) {
    let store = SettingsStore::global(cx);
    let permissions = store
        .raw_user_settings()
        .and_then(|user| user.content.agent.as_ref())
        .and_then(|agent| agent.sandbox_permissions.as_ref());

    let network_hosts = permissions
        .and_then(|permissions| permissions.network_hosts.as_ref())
        .map(|hosts| hosts.0.clone())
        .unwrap_or_default();
    let write_paths = permissions
        .and_then(|permissions| permissions.write_paths.as_ref())
        .map(|paths| paths.0.clone())
        .unwrap_or_default();

    (network_hosts, write_paths)
}

pub(super) fn canonicalize_host(host: &str) -> Result<String, String> {
    use http_proxy::HostPatternError;

    HostPattern::parse(host)
        .map(|pattern| pattern.to_string())
        .map_err(|error| match error {
            HostPatternError::Empty => "Domain cannot be empty.".to_string(),
            HostPatternError::IpLiteral(_) => {
                "IP addresses and local domains aren't allowed; enter a domain like github.com."
                    .to_string()
            }
            HostPatternError::InvalidWildcard(_) => {
                "Wildcards are only allowed as a leading label, e.g. *.github.com.".to_string()
            }
            HostPatternError::Invalid { .. } => {
                "Not a valid domain. Use a domain like github.com or *.npmjs.org.".to_string()
            }
        })
}

fn update_sandbox_permissions(
    cx: &mut App,
    update: impl 'static + Send + FnOnce(&mut settings::SandboxPermissionsContent),
) {
    SettingsStore::global(cx).update_settings_file(<dyn fs::Fs>::global(cx), move |settings, _| {
        update(
            settings
                .agent
                .get_or_insert_default()
                .sandbox_permissions
                .get_or_insert_default(),
        );
    });
}

pub(super) fn set_sandbox_enabled(value: bool, cx: &mut App) {
    // The UI presents an "enabled" switch, but the stored setting is the
    // inverse (`allow_unsandboxed`).
    update_sandbox_permissions(cx, move |permissions| {
        permissions.allow_unsandboxed = Some(!value);
    });
}

pub(super) fn set_allow_all_hosts(value: bool, cx: &mut App) {
    update_sandbox_permissions(cx, move |permissions| {
        permissions.allow_all_hosts = Some(value);
    });
}

pub(super) fn set_allow_git_access(value: bool, cx: &mut App) {
    update_sandbox_permissions(cx, move |permissions| {
        permissions.allow_git_access = Some(value);
    });
}

pub(super) fn set_allow_fs_write_all(value: bool, cx: &mut App) {
    update_sandbox_permissions(cx, move |permissions| {
        permissions.allow_fs_write_all = Some(value);
    });
}

pub(super) fn add_network_host(host: String, cx: &mut App) {
    update_sandbox_permissions(cx, move |permissions| {
        let hosts = &mut permissions.network_hosts.get_or_insert_default().0;
        if !hosts.contains(&host) {
            hosts.push(host);
        }
    });
}

pub(super) fn update_network_host(old_host: String, new_host: String, cx: &mut App) {
    update_sandbox_permissions(cx, move |permissions| {
        let hosts = &mut permissions.network_hosts.get_or_insert_default().0;
        if hosts.contains(&new_host) {
            return;
        }
        if let Some(entry) = hosts.iter_mut().find(|host| **host == old_host) {
            *entry = new_host;
        }
    });
}

pub(super) fn remove_network_host(host: String, cx: &mut App) {
    update_sandbox_permissions(cx, move |permissions| {
        if let Some(hosts) = permissions.network_hosts.as_mut() {
            hosts.0.retain(|entry| *entry != host);
        }
    });
}

pub(super) fn add_write_path(path: PathBuf, cx: &mut App) {
    // Normalize away `.`/`..` so the stored entry matches the form the runtime
    // uses for coverage checks (see `compile_sandbox_permissions`) and the form
    // persisted by the in-thread "Allow always" grant.
    let Ok(path) = util::paths::normalize_lexically(&path) else {
        return;
    };
    update_sandbox_permissions(cx, move |permissions| {
        let paths = &mut permissions.write_paths.get_or_insert_default().0;
        // Store minimal subtrees so a parent path subsumes its descendants.
        util::paths::insert_subtree(paths, path);
    });
}

pub(super) fn update_write_path(old_path: PathBuf, new_path: PathBuf, cx: &mut App) {
    let Ok(new_path) = util::paths::normalize_lexically(&new_path) else {
        return;
    };
    update_sandbox_permissions(cx, move |permissions| {
        if let Some(paths) = permissions.write_paths.as_mut() {
            paths.0.retain(|entry| *entry != old_path);
            util::paths::insert_subtree(&mut paths.0, new_path);
        }
    });
}

pub(super) fn remove_write_path(path: PathBuf, cx: &mut App) {
    update_sandbox_permissions(cx, move |permissions| {
        if let Some(paths) = permissions.write_paths.as_mut() {
            paths.0.retain(|entry| *entry != path);
        }
    });
}
