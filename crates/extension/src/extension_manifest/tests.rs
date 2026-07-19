use pretty_assertions::assert_eq;
use util::rel_path::rel_path_buf;

use crate::ProcessExecCapability;

use super::*;

fn extension_manifest() -> ExtensionManifest {
    ExtensionManifest {
        id: "test".into(),
        name: "Test".to_string(),
        version: "1.0.0".into(),
        schema_version: SchemaVersion::ZERO,
        description: None,
        repository: None,
        authors: vec![],
        lib: Default::default(),
        themes: vec![],
        icon_themes: vec![],
        languages: vec![],
        grammars: BTreeMap::default(),
        language_servers: BTreeMap::default(),
        context_servers: BTreeMap::default(),
        slash_commands: BTreeMap::default(),
        snippets: None,
        capabilities: vec![],
        debug_adapters: Default::default(),
        debug_locators: Default::default(),
        language_model_providers: BTreeMap::default(),
    }
}

#[test]
fn test_build_adapter_schema_path_with_schema_path() {
    let adapter_name = Arc::from("my_adapter");
    let entry = DebugAdapterManifestEntry {
        schema_path: Some(rel_path_buf("foo/bar")),
    };

    let path = build_debug_adapter_schema_path(&adapter_name, &entry).unwrap();
    assert_eq!(path, rel_path_buf("foo/bar"));
}

#[test]
fn test_build_adapter_schema_path_without_schema_path() {
    let adapter_name = Arc::from("my_adapter");
    let entry = DebugAdapterManifestEntry::default();

    let path = build_debug_adapter_schema_path(&adapter_name, &entry).unwrap();
    assert_eq!(path, rel_path_buf("debug_adapter_schemas/my_adapter.json"));
}

#[test]
fn test_allow_exec_exact_match() {
    let manifest = ExtensionManifest {
        capabilities: vec![ExtensionCapability::ProcessExec(ProcessExecCapability {
            command: "ls".to_string(),
            args: vec!["-la".to_string()],
        })],
        ..extension_manifest()
    };

    assert!(manifest.allow_exec("ls", &["-la"]).is_ok());
    assert!(manifest.allow_exec("ls", &["-l"]).is_err());
    assert!(manifest.allow_exec("pwd", &[] as &[&str]).is_err());
}

#[test]
fn test_allow_exec_wildcard_arg() {
    let manifest = ExtensionManifest {
        capabilities: vec![ExtensionCapability::ProcessExec(ProcessExecCapability {
            command: "git".to_string(),
            args: vec!["*".to_string()],
        })],
        ..extension_manifest()
    };

    assert!(manifest.allow_exec("git", &["status"]).is_ok());
    assert!(manifest.allow_exec("git", &["commit"]).is_ok());
    assert!(manifest.allow_exec("git", &["status", "-s"]).is_err());
    assert!(manifest.allow_exec("npm", &["install"]).is_err());
}

#[test]
fn test_allow_exec_double_wildcard() {
    let manifest = ExtensionManifest {
        capabilities: vec![ExtensionCapability::ProcessExec(ProcessExecCapability {
            command: "cargo".to_string(),
            args: vec!["test".to_string(), "**".to_string()],
        })],
        ..extension_manifest()
    };

    assert!(manifest.allow_exec("cargo", &["test"]).is_ok());
    assert!(manifest.allow_exec("cargo", &["test", "--all"]).is_ok());
    assert!(
        manifest
            .allow_exec("cargo", &["test", "--all", "--no-fail-fast"])
            .is_ok()
    );
    assert!(manifest.allow_exec("cargo", &["build"]).is_err());
}

#[test]
fn test_allow_exec_mixed_wildcards() {
    let manifest = ExtensionManifest {
        capabilities: vec![ExtensionCapability::ProcessExec(ProcessExecCapability {
            command: "docker".to_string(),
            args: vec!["run".to_string(), "*".to_string(), "**".to_string()],
        })],
        ..extension_manifest()
    };

    assert!(manifest.allow_exec("docker", &["run", "nginx"]).is_ok());
    assert!(manifest.allow_exec("docker", &["run"]).is_err());
    assert!(
        manifest
            .allow_exec("docker", &["run", "ubuntu", "bash"])
            .is_ok()
    );
    assert!(
        manifest
            .allow_exec("docker", &["run", "alpine", "sh", "-c", "echo hello"])
            .is_ok()
    );
    assert!(manifest.allow_exec("docker", &["ps"]).is_err());
}

#[test]
#[cfg(target_os = "windows")]
fn test_deserialize_manifest_with_windows_separators() {
    use indoc::indoc;

    let content = indoc! {r#"
        id = "test-manifest"
        name = "Test Manifest"
        version = "0.0.1"
        schema_version = 0
        languages = ["foo\\bar"]
    "#};
    let manifest: ExtensionManifest = toml::from_str(&content).expect("manifest should parse");
    assert_eq!(manifest.languages, vec![rel_path_buf("foo/bar")]);
}
