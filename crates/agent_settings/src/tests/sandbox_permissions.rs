use std::path::PathBuf;

use serde_json::json;

use crate::SandboxPermissions;
use crate::settings_impl::compile_sandbox_permissions;

#[test]
fn test_sandbox_permissions_empty() {
    let permissions = compile_sandbox_permissions(None);
    assert_eq!(permissions, SandboxPermissions::default());
}

#[test]
fn test_sandbox_permissions_parsing_and_pruning() {
    let json = json!({
        "allow_all_hosts": true,
        "network_hosts": ["github.com", "*.npmjs.org"],
        "allow_git_access": true,
        "allow_unsandboxed": true,
        "write_paths": [
            "/tmp/build/cache",
            "/tmp/build",
            "/var/log"
        ]
    });

    let content: settings::SandboxPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_sandbox_permissions(Some(content));

    assert!(permissions.allow_all_hosts);
    assert_eq!(
        permissions.network_hosts,
        vec!["github.com".to_string(), "*.npmjs.org".to_string()]
    );
    assert!(permissions.allow_git_access);
    assert!(!permissions.allow_fs_write_all);
    assert!(permissions.allow_unsandboxed);
    assert_eq!(
        permissions.write_paths,
        vec![PathBuf::from("/tmp/build"), PathBuf::from("/var/log")]
    );
}

#[test]
fn test_sandbox_permissions_normalizes_and_prunes_parent_traversal() {
    let json = json!({
        "write_paths": [
            "/tmp/build/../build/cache",
            "/tmp/build",
        ]
    });

    let content: settings::SandboxPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_sandbox_permissions(Some(content));

    // `/tmp/build/../build/cache` normalizes to `/tmp/build/cache`, which is
    // then pruned as a redundant child of `/tmp/build`.
    assert_eq!(permissions.write_paths, vec![PathBuf::from("/tmp/build")]);
}
