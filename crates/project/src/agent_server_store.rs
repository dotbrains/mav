use std::{
    any::Any,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::Duration,
};

use ::settings::{AgentConfigOptionValue, RegisterSetting, SettingsStore, update_settings_file};
use anyhow::{Context as _, Result, bail};
use collections::HashMap;
use fs::{Fs, RemoveOptions};
use futures::StreamExt;
use gpui::{
    AppContext as _, AsyncApp, Context, Entity, EventEmitter, SharedString, Subscription, Task,
    TaskExt,
};
use http_client::{HttpClient, github::AssetKind};
use node_runtime::NodeRuntime;
use percent_encoding::percent_decode_str;
use remote::RemoteClient;
use rpc::{AnyProtoClient, TypedEnvelope, proto};
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;
use util::{ResultExt as _, debug_panic};

use crate::ProjectEnvironment;
use crate::agent_registry_store::{AgentRegistryStore, RegistryAgent, RegistryTargetConfig};

use crate::worktree_store::WorktreeStore;

mod archive_helpers;
mod config;
mod local_agents;
mod remote_agent;
mod remote_handlers;
mod store_registration;

#[cfg(test)]
mod tests;

use archive_helpers::{
    RegistryArchiveKind, github_release_archive_from_url, registry_archive_kind_for_url,
    remove_stale_versioned_archive_cache_dirs, sanitize_path_component,
    versioned_archive_cache_dir,
};
pub use config::{AllAgentServersSettings, CustomAgentServerSettings};
#[cfg(test)]
use local_agents::bounded_npm_package_spec;
use local_agents::{LocalCustomAgent, LocalRegistryArchiveAgent, LocalRegistryNpxAgent};
use remote_agent::RemoteExternalAgentServer;

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, JsonSchema)]
pub struct AgentServerCommand {
    #[serde(rename = "command")]
    pub path: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    pub env: Option<HashMap<String, String>>,
}

impl std::fmt::Debug for AgentServerCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let filtered_env = self.env.as_ref().map(|env| {
            env.iter()
                .map(|(k, v)| {
                    (
                        k,
                        if util::redact::should_redact(k) {
                            "[REDACTED]"
                        } else {
                            v
                        },
                    )
                })
                .collect::<Vec<_>>()
        });

        f.debug_struct("AgentServerCommand")
            .field("path", &self.path)
            .field("args", &self.args)
            .field("env", &filtered_env)
            .finish()
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct AgentId(pub SharedString);

impl AgentId {
    pub fn new(id: impl Into<SharedString>) -> Self {
        AgentId(id.into())
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&'static str> for AgentId {
    fn from(value: &'static str) -> Self {
        AgentId(value.into())
    }
}

impl From<AgentId> for SharedString {
    fn from(value: AgentId) -> Self {
        value.0
    }
}

impl AsRef<str> for AgentId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for AgentId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExternalAgentSource {
    #[default]
    Custom,
    Registry,
}

pub trait ExternalAgentServer {
    fn get_command(
        &mut self,
        extra_args: Vec<String>,
        extra_env: HashMap<String, String>,
        cx: &mut AsyncApp,
    ) -> Task<Result<AgentServerCommand>>;

    fn version(&self) -> Option<&SharedString> {
        None
    }

    fn take_new_version_available_tx(&mut self) -> Option<watch::Sender<Option<String>>> {
        None
    }

    fn set_new_version_available_tx(&mut self, _tx: watch::Sender<Option<String>>) {}

    fn take_loading_status_tx(&mut self) -> Option<watch::Sender<Option<String>>> {
        None
    }

    fn set_loading_status_tx(&mut self, _tx: watch::Sender<Option<String>>) {}

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

enum AgentServerStoreState {
    Local {
        node_runtime: NodeRuntime,
        fs: Arc<dyn Fs>,
        project_environment: Entity<ProjectEnvironment>,
        downstream_client: Option<(u64, AnyProtoClient)>,
        settings: Option<AllAgentServersSettings>,
        http_client: Arc<dyn HttpClient>,
        _subscriptions: Vec<Subscription>,
    },
    Remote {
        project_id: u64,
        upstream_client: Entity<RemoteClient>,
        worktree_store: Entity<WorktreeStore>,
    },
    Collab,
}

pub struct ExternalAgentEntry {
    server: Box<dyn ExternalAgentServer>,
    icon: Option<SharedString>,
    display_name: Option<SharedString>,
    pub source: ExternalAgentSource,
}

impl ExternalAgentEntry {
    pub fn new(
        server: Box<dyn ExternalAgentServer>,
        source: ExternalAgentSource,
        icon: Option<SharedString>,
        display_name: Option<SharedString>,
    ) -> Self {
        Self {
            server,
            icon,
            display_name,
            source,
        }
    }
}

pub struct AgentServerStore {
    state: AgentServerStoreState,
    pub external_agents: HashMap<AgentId, ExternalAgentEntry>,
}

pub struct AgentServersUpdated;

impl EventEmitter<AgentServersUpdated> for AgentServerStore {}

static EXTENSION_TO_REGISTRY_IDS: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| {
        HashMap::from_iter([
            ("opencode", "opencode"),
            ("mistral-vibe", "mistral-vibe"),
            ("auggie", "auggie"),
            ("stakpak", "stakpak"),
            ("codebuddy", "codebuddy-code"),
            ("autohand-acp", "autohand"),
            ("corust-agent", "corust-agent"),
            ("factory-droid", "factory-droid"),
            // Unmaintained
            // ("qqcode", ""),
        ])
    });

impl AgentServerStore {
    pub fn migrate_agent_server_from_extensions(
        &mut self,
        id: Arc<str>,
        fs: Arc<dyn Fs>,
        cx: &mut Context<Self>,
    ) {
        let Some(registry_id) = EXTENSION_TO_REGISTRY_IDS.get(id.as_ref()) else {
            return;
        };

        update_settings_file(fs, cx, move |settings, _| {
            let agent_servers = settings.agent_servers.get_or_insert_default();
            // Take the old settings
            let settings = agent_servers.remove(id.as_ref());
            // If they had both installed, just remove the extension settings, leave theirregistry settings alone
            if agent_servers.contains_key(*registry_id) {
                return;
            }
            // Insert the old settings, or write new ones so it is "installed" via the registry
            agent_servers.insert(
                registry_id.to_string(),
                settings.unwrap_or_else(|| ::settings::CustomAgentServerSettings::Registry {
                    default_mode: None,
                    env: Default::default(),
                    default_config_options: HashMap::default(),
                    favorite_config_option_values: HashMap::default(),
                }),
            );
        });
    }

    pub fn agent_icon(&self, id: &AgentId) -> Option<SharedString> {
        self.external_agents
            .get(id)
            .and_then(|entry| entry.icon.clone())
    }

    pub fn agent_source(&self, name: &AgentId) -> Option<ExternalAgentSource> {
        self.external_agents.get(name).map(|entry| entry.source)
    }
}
