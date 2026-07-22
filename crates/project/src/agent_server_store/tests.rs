use super::*;
use crate::agent_registry_store::{
    AgentRegistryStore, RegistryAgent, RegistryAgentMetadata, RegistryNpxAgent,
};
use crate::worktree_store::{WorktreeIdCounter, WorktreeStore};
use gpui::TestAppContext;
use node_runtime::NodeRuntime;
use settings::Settings as _;

fn make_npx_agent(id: &str, version: &str) -> RegistryAgent {
    let id = SharedString::from(id.to_string());
    RegistryAgent::Npx(RegistryNpxAgent {
        metadata: RegistryAgentMetadata {
            id: AgentId::new(id.clone()),
            name: id.clone(),
            description: SharedString::from(""),
            version: SharedString::from(version.to_string()),
            repository: None,
            website: None,
            icon_path: None,
        },
        package: id,
        args: Vec::new(),
        env: HashMap::default(),
    })
}

fn init_test_settings(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
}

fn init_registry(
    cx: &mut TestAppContext,
    agents: Vec<RegistryAgent>,
) -> gpui::Entity<AgentRegistryStore> {
    cx.update(|cx| AgentRegistryStore::init_test_global(cx, agents))
}

fn set_registry_settings(cx: &mut TestAppContext, agent_names: &[&str]) {
    cx.update(|cx| {
        AllAgentServersSettings::override_global(
            AllAgentServersSettings(
                agent_names
                    .iter()
                    .map(|name| {
                        (
                            name.to_string(),
                            ::settings::CustomAgentServerSettings::Registry {
                                env: HashMap::default(),
                                default_mode: None,
                                default_config_options: HashMap::default(),
                                favorite_config_option_values: HashMap::default(),
                            }
                            .into(),
                        )
                    })
                    .collect(),
            ),
            cx,
        );
    });
}

fn create_agent_server_store(cx: &mut TestAppContext) -> gpui::Entity<AgentServerStore> {
    cx.update(|cx| {
        let fs: Arc<dyn Fs> = fs::FakeFs::new(cx.background_executor().clone());
        let worktree_store =
            cx.new(|cx| WorktreeStore::local(false, fs.clone(), WorktreeIdCounter::get(cx)));
        let project_environment = cx.new(|cx| {
            crate::ProjectEnvironment::new(None, worktree_store.downgrade(), None, false, cx)
        });
        let http_client = http_client::FakeHttpClient::with_404_response();

        cx.new(|cx| {
            AgentServerStore::local(
                NodeRuntime::unavailable(),
                fs,
                project_environment,
                http_client,
                cx,
            )
        })
    })
}

#[test]
fn builds_bounded_npm_package_specs() {
    assert_eq!(
        bounded_npm_package_spec("agent-package@1.2.3"),
        "agent-package@0.0.0 - 1.2.3"
    );
    assert_eq!(
        bounded_npm_package_spec("@scope/agent-package@1.2.3-beta.1"),
        "@scope/agent-package@0.0.0 - 1.2.3-beta.1"
    );
    assert_eq!(
        bounded_npm_package_spec("@scope/agent-package"),
        "@scope/agent-package"
    );
    assert_eq!(
        bounded_npm_package_spec("agent-package@latest"),
        "agent-package@latest"
    );
}

#[test]
fn detects_supported_archive_suffixes() {
    assert!(matches!(
        registry_archive_kind_for_url("https://example.com/agent.zip"),
        Ok(RegistryArchiveKind::Archive(AssetKind::Zip))
    ));
    assert!(matches!(
        registry_archive_kind_for_url("https://example.com/agent.zip?download=1"),
        Ok(RegistryArchiveKind::Archive(AssetKind::Zip))
    ));
    assert!(matches!(
        registry_archive_kind_for_url("https://example.com/agent.tar.gz"),
        Ok(RegistryArchiveKind::Archive(AssetKind::TarGz))
    ));
    assert!(matches!(
        registry_archive_kind_for_url("https://example.com/agent.tar.gz?download=1#latest"),
        Ok(RegistryArchiveKind::Archive(AssetKind::TarGz))
    ));
    assert!(matches!(
        registry_archive_kind_for_url("https://example.com/agent.tgz"),
        Ok(RegistryArchiveKind::Archive(AssetKind::TarGz))
    ));
    assert!(matches!(
        registry_archive_kind_for_url("https://example.com/agent.tgz#download"),
        Ok(RegistryArchiveKind::Archive(AssetKind::TarGz))
    ));
    assert!(matches!(
        registry_archive_kind_for_url("https://example.com/agent.tar.bz2"),
        Ok(RegistryArchiveKind::Archive(AssetKind::TarBz2))
    ));
    assert!(matches!(
        registry_archive_kind_for_url("https://example.com/agent.tar.bz2?download=1"),
        Ok(RegistryArchiveKind::Archive(AssetKind::TarBz2))
    ));
    assert!(matches!(
        registry_archive_kind_for_url("https://example.com/agent.tbz2"),
        Ok(RegistryArchiveKind::Archive(AssetKind::TarBz2))
    ));
    assert!(matches!(
        registry_archive_kind_for_url("https://example.com/agent.tbz2#download"),
        Ok(RegistryArchiveKind::Archive(AssetKind::TarBz2))
    ));
    assert!(matches!(
        registry_archive_kind_for_url("https://example.com/agent.ZIP"),
        Ok(RegistryArchiveKind::Archive(AssetKind::Zip))
    ));
}

#[test]
fn detects_raw_binary_archive_urls() {
    assert_eq!(
        registry_archive_kind_for_url("https://x.ai/cli/grok-0.2.20-macos-aarch64").unwrap(),
        RegistryArchiveKind::RawBinary {
            file_name: "grok-0.2.20-macos-aarch64".to_string()
        },
    );
    assert_eq!(
        registry_archive_kind_for_url("https://x.ai/cli/grok-0.2.20-windows-x86_64.exe").unwrap(),
        RegistryArchiveKind::RawBinary {
            file_name: "grok-0.2.20-windows-x86_64.exe".to_string()
        },
    );
    assert_eq!(
        registry_archive_kind_for_url("https://example.com/agent-binary?download=1#latest")
            .unwrap(),
        RegistryArchiveKind::RawBinary {
            file_name: "agent-binary".to_string()
        },
    );
    assert_eq!(
        registry_archive_kind_for_url("https://example.com/agent%20binary").unwrap(),
        RegistryArchiveKind::RawBinary {
            file_name: "agent binary".to_string()
        },
    );
    // No file name to install the binary as.
    assert!(registry_archive_kind_for_url("https://example.com/").is_err());
    // Percent-decoding must not allow path traversal in the file name.
    assert!(registry_archive_kind_for_url("https://example.com/a%2F..%2Fevil").is_err());
    assert!(registry_archive_kind_for_url("https://example.com/%2E%2E").is_err());
}

#[test]
fn parses_github_release_archive_urls() {
    let github_archive = github_release_archive_from_url(
        "https://github.com/owner/repo/releases/download/release%2F2.3.5/agent.tar.bz2?download=1",
    )
    .unwrap();

    assert_eq!(github_archive.repo_name_with_owner, "owner/repo");
    assert_eq!(github_archive.tag, "release/2.3.5");
    assert_eq!(github_archive.asset_name, "agent.tar.bz2");
}

#[test]
fn rejects_unsupported_archive_suffixes() {
    let error = registry_archive_kind_for_url("https://example.com/agent.tar.xz")
        .err()
        .map(|error| error.to_string());

    assert_eq!(
        error,
        Some(
            "unsupported archive type .tar.xz in URL: https://example.com/agent.tar.xz".to_string()
        ),
    );

    for installer_url in [
        "https://example.com/agent.dmg",
        "https://example.com/agent.pkg",
        "https://example.com/agent.deb",
        "https://example.com/agent.rpm",
        "https://example.com/agent.msi",
        "https://example.com/agent.AppImage",
    ] {
        assert!(
            registry_archive_kind_for_url(installer_url).is_err(),
            "expected {installer_url} to be rejected"
        );
    }
}

#[test]
fn versioned_archive_cache_dir_includes_version_before_url_hash() {
    let slash_version_dir = versioned_archive_cache_dir(
        Path::new("/tmp/agents"),
        Some("release/2.3.5"),
        "https://example.com/agent.zip",
    );
    let colon_version_dir = versioned_archive_cache_dir(
        Path::new("/tmp/agents"),
        Some("release:2.3.5"),
        "https://example.com/agent.zip",
    );
    let file_name = slash_version_dir
        .file_name()
        .and_then(|name| name.to_str())
        .expect("cache directory should have a file name");

    assert!(file_name.starts_with("v_release-2.3.5_"));
    assert_ne!(slash_version_dir, colon_version_dir);
}

#[gpui::test]
async fn test_remove_stale_versioned_archive_cache_dirs(cx: &mut TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let base_dir = Path::new("/cache");

    // FakeFs increments mtime on every create, so creation order is
    // ascending mtime: v_old_1 < v_old_2 < other < v_not_a_dir < v_current < v_newer.
    fs.insert_tree(
        base_dir,
        serde_json::json!({
            "v_old_1": {},
            "v_old_2": {},
            "other": {},
        }),
    )
    .await;
    fs.insert_file(base_dir.join("v_not_a_dir"), b"keep me".to_vec())
        .await;
    let current_version_dir = base_dir.join("v_current");
    fs.create_dir(&current_version_dir).await.unwrap();
    // Sibling that "finished extracting" after the current dir was cached.
    fs.create_dir(&base_dir.join("v_newer")).await.unwrap();

    remove_stale_versioned_archive_cache_dirs(
        fs.clone() as Arc<dyn Fs>,
        base_dir,
        &current_version_dir,
    )
    .await
    .unwrap();

    let mut remaining = fs
        .read_dir(base_dir)
        .await
        .unwrap()
        .filter_map(|entry| async move { entry.ok() })
        .map(|path| {
            path.file_name()
                .expect("entry has a name")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>()
        .await;
    remaining.sort();

    assert_eq!(
        remaining,
        vec![
            "other".to_string(),
            "v_current".to_string(),
            "v_newer".to_string(),
            "v_not_a_dir".to_string(),
        ]
    );
}

#[gpui::test]
fn test_version_change_sends_notification(cx: &mut TestAppContext) {
    init_test_settings(cx);
    let registry = init_registry(cx, vec![make_npx_agent("test-agent", "1.0.0")]);
    set_registry_settings(cx, &["test-agent"]);
    let store = create_agent_server_store(cx);

    // Verify the agent was registered with version 1.0.0.
    store.read_with(cx, |store, _| {
        let entry = store
            .external_agents
            .get(&AgentId::new("test-agent"))
            .expect("agent should be registered");
        assert_eq!(
            entry.server.version().map(|v| v.to_string()),
            Some("1.0.0".to_string())
        );
    });

    // Set up a watch channel and store the tx on the agent.
    let (tx, mut rx) = watch::channel::<Option<String>>(None);
    store.update(cx, |store, _| {
        let entry = store
            .external_agents
            .get_mut(&AgentId::new("test-agent"))
            .expect("agent should be registered");
        entry.server.set_new_version_available_tx(tx);
    });

    // Update the registry to version 2.0.0.
    registry.update(cx, |store, cx| {
        store.set_agents(vec![make_npx_agent("test-agent", "2.0.0")], cx);
    });
    cx.run_until_parked();

    // The watch channel should have received the new version.
    assert_eq!(rx.borrow().as_deref(), Some("2.0.0"));
}

#[gpui::test]
fn test_same_version_preserves_tx(cx: &mut TestAppContext) {
    init_test_settings(cx);
    let registry = init_registry(cx, vec![make_npx_agent("test-agent", "1.0.0")]);
    set_registry_settings(cx, &["test-agent"]);
    let store = create_agent_server_store(cx);

    let (tx, mut rx) = watch::channel::<Option<String>>(None);
    store.update(cx, |store, _| {
        let entry = store
            .external_agents
            .get_mut(&AgentId::new("test-agent"))
            .expect("agent should be registered");
        entry.server.set_new_version_available_tx(tx);
    });

    // "Refresh" the registry with the same version.
    registry.update(cx, |store, cx| {
        store.set_agents(vec![make_npx_agent("test-agent", "1.0.0")], cx);
    });
    cx.run_until_parked();

    // No notification should have been sent.
    assert_eq!(rx.borrow().as_deref(), None);

    // The tx should have been transferred to the rebuilt agent entry.
    store.update(cx, |store, _| {
        let entry = store
            .external_agents
            .get_mut(&AgentId::new("test-agent"))
            .expect("agent should be registered");
        assert!(
            entry.server.take_new_version_available_tx().is_some(),
            "tx should have been transferred to the rebuilt agent"
        );
    });
}

#[gpui::test]
fn test_no_tx_stored_does_not_panic_on_version_change(cx: &mut TestAppContext) {
    init_test_settings(cx);
    let registry = init_registry(cx, vec![make_npx_agent("test-agent", "1.0.0")]);
    set_registry_settings(cx, &["test-agent"]);
    let _store = create_agent_server_store(cx);

    // Update the registry without having stored any tx — should not panic.
    registry.update(cx, |store, cx| {
        store.set_agents(vec![make_npx_agent("test-agent", "2.0.0")], cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn test_multiple_agents_independent_notifications(cx: &mut TestAppContext) {
    init_test_settings(cx);
    let registry = init_registry(
        cx,
        vec![
            make_npx_agent("agent-a", "1.0.0"),
            make_npx_agent("agent-b", "3.0.0"),
        ],
    );
    set_registry_settings(cx, &["agent-a", "agent-b"]);
    let store = create_agent_server_store(cx);

    let (tx_a, mut rx_a) = watch::channel::<Option<String>>(None);
    let (tx_b, mut rx_b) = watch::channel::<Option<String>>(None);
    store.update(cx, |store, _| {
        store
            .external_agents
            .get_mut(&AgentId::new("agent-a"))
            .expect("agent-a should be registered")
            .server
            .set_new_version_available_tx(tx_a);
        store
            .external_agents
            .get_mut(&AgentId::new("agent-b"))
            .expect("agent-b should be registered")
            .server
            .set_new_version_available_tx(tx_b);
    });

    // Update only agent-a to a new version; agent-b stays the same.
    registry.update(cx, |store, cx| {
        store.set_agents(
            vec![
                make_npx_agent("agent-a", "2.0.0"),
                make_npx_agent("agent-b", "3.0.0"),
            ],
            cx,
        );
    });
    cx.run_until_parked();

    // agent-a should have received a notification.
    assert_eq!(rx_a.borrow().as_deref(), Some("2.0.0"));

    // agent-b should NOT have received a notification.
    assert_eq!(rx_b.borrow().as_deref(), None);

    // agent-b's tx should have been transferred.
    store.update(cx, |store, _| {
        assert!(
            store
                .external_agents
                .get_mut(&AgentId::new("agent-b"))
                .expect("agent-b should be registered")
                .server
                .take_new_version_available_tx()
                .is_some(),
            "agent-b tx should have been transferred"
        );
    });
}
