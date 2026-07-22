mod capability_granter;
pub mod extension_settings;
mod extension_store_parts;
pub mod headless_host;
pub mod wasm_host;

#[cfg(test)]
mod extension_store_test;

use anyhow::{Context as _, Result, anyhow, bail};
use async_compression::futures::bufread::GzipDecoder;
use async_tar::Archive;
use client::{Client, proto, telemetry::Telemetry};
use cloud_api_types::{ExtensionMetadata, ExtensionProvides, GetExtensionsResponse};
use collections::{BTreeMap, BTreeSet, FxHashSet, HashMap, HashSet, btree_map};
pub use extension::ExtensionManifest;
use extension::extension_builder::{CompileExtensionOptions, ExtensionBuilder};
use extension::{
    ExtensionContextServerProxy, ExtensionDebugAdapterProviderProxy, ExtensionEvents,
    ExtensionGrammarProxy, ExtensionHostProxy, ExtensionLanguageProxy,
    ExtensionLanguageServerProxy, ExtensionSnippetProxy, ExtensionThemeProxy,
};
use fs::{Fs, RemoveOptions, RenameOptions};
use futures::future::join_all;
use futures::{
    AsyncReadExt as _, Future, FutureExt as _, StreamExt as _,
    channel::{
        mpsc::{UnboundedSender, unbounded},
        oneshot,
    },
    io::BufReader,
    select_biased,
};
use gpui::{
    App, AppContext as _, AsyncApp, Context, Entity, EventEmitter, Global, Task, TaskExt,
    UpdateGlobal as _, WeakEntity, actions,
};
use http_client::{AsyncBody, HttpClient, HttpClientWithUrl};
use language::{
    LanguageConfig, LanguageMatcher, LanguageName, LanguageQueries, LoadedLanguage,
    QUERY_FILENAME_PREFIXES, Rope,
};
use node_runtime::NodeRuntime;
use project::ContextProviderWithTasks;
use release_channel::ReleaseChannel;
use remote::RemoteClient;
use semver::Version;
use serde::{Deserialize, Serialize};
use settings::{SemanticTokenRules, Settings, SettingsStore};
use std::ops::RangeInclusive;
use std::str::FromStr;
use std::sync::LazyLock;
use std::{
    cmp::Ordering,
    path::{self, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use task::TaskTemplates;
use url::Url;
use util::{ResultExt, paths::RemotePathBuf, rel_path::PathExt};
use wasm_host::{
    WasmExtension, WasmHost,
    wit::{is_supported_wasm_api_version, wasm_api_version_range},
};

pub use extension::{
    ExtensionLibraryKind, GrammarManifestEntry, OldExtensionManifest, SchemaVersion,
};
pub use extension_settings::ExtensionSettings;

pub const RELOAD_DEBOUNCE_DURATION: Duration = Duration::from_millis(200);
const FS_WATCH_LATENCY: Duration = Duration::from_millis(100);

/// The current extension [`SchemaVersion`] supported by Mav.
const CURRENT_SCHEMA_VERSION: SchemaVersion = SchemaVersion(1);

/// Extensions that should no longer be loaded or downloaded.
///
/// These snippets should no longer be downloaded or loaded, because their
/// functionality has been integrated into the core editor.
static SUPPRESSED_EXTENSIONS: LazyLock<FxHashSet<&str>> = LazyLock::new(|| {
    FxHashSet::from_iter([
        "snippets",
        "ruff",
        "ty",
        "basedpyright",
        "basher",
        // ACP
        "opencode",
        "mistral-vibe",
        "auggie",
        "stakpak",
        "codebuddy",
        "autohand-acp",
        "corust-agent",
        "factory-droid",
        "qqcode",
    ])
});

/// Returns the [`SchemaVersion`] range that is compatible with this version of Mav.
pub fn schema_version_range() -> RangeInclusive<SchemaVersion> {
    SchemaVersion::ZERO..=CURRENT_SCHEMA_VERSION
}

/// Returns whether the given extension version is compatible with this version of Mav.
pub fn is_version_compatible(
    release_channel: ReleaseChannel,
    extension_version: &ExtensionMetadata,
) -> bool {
    let schema_version = extension_version.manifest.schema_version.unwrap_or(0);
    if CURRENT_SCHEMA_VERSION.0 < schema_version {
        return false;
    }

    if let Some(wasm_api_version) = extension_version
        .manifest
        .wasm_api_version
        .as_ref()
        .and_then(|wasm_api_version| Version::from_str(wasm_api_version).ok())
        && !is_supported_wasm_api_version(release_channel, wasm_api_version)
    {
        return false;
    }

    true
}

pub struct ExtensionStore {
    pub proxy: Arc<ExtensionHostProxy>,
    pub builder: Arc<ExtensionBuilder>,
    pub extension_index: ExtensionIndex,
    pub fs: Arc<dyn Fs>,
    pub http_client: Arc<HttpClientWithUrl>,
    pub telemetry: Option<Arc<Telemetry>>,
    pub reload_tx: UnboundedSender<Option<Arc<str>>>,
    pub reload_complete_senders: Vec<oneshot::Sender<()>>,
    pub installed_dir: PathBuf,
    pub staging_dir: PathBuf,
    pub outstanding_operations: BTreeMap<Arc<str>, ExtensionOperation>,
    pub index_path: PathBuf,
    pub modified_extensions: HashSet<Arc<str>>,
    pub wasm_host: Arc<WasmHost>,
    pub wasm_extensions: Vec<(Arc<ExtensionManifest>, WasmExtension)>,
    pub tasks: Vec<Task<()>>,
    pub remote_clients: Vec<WeakEntity<RemoteClient>>,
    pub ssh_registered_tx: UnboundedSender<()>,
}

#[derive(Clone, Copy)]
pub enum ExtensionOperation {
    Upgrade,
    Install,
    Remove,
}

#[derive(Clone)]
pub enum Event {
    ExtensionsUpdated,
    StartedReloading,
    ExtensionInstalled(Arc<str>),
    ExtensionUninstalled(Arc<str>),
    ExtensionFailedToLoad(Arc<str>),
}

impl EventEmitter<Event> for ExtensionStore {}

struct GlobalExtensionStore(Entity<ExtensionStore>);

impl Global for GlobalExtensionStore {}

#[derive(Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct ExtensionIndex {
    pub extensions: BTreeMap<Arc<str>, ExtensionIndexEntry>,
    pub themes: BTreeMap<Arc<str>, ExtensionIndexThemeEntry>,
    #[serde(default)]
    pub icon_themes: BTreeMap<Arc<str>, ExtensionIndexIconThemeEntry>,
    pub languages: BTreeMap<LanguageName, ExtensionIndexLanguageEntry>,
}

impl ExtensionIndex {
    fn extensions_to_sync_to_remote(&self) -> RemoteSyncExtensions {
        let mut extensions = RemoteSyncExtensions::default();

        for (id, entry) in &self.extensions {
            if entry.manifest.remote_load().is_some() {
                extensions.insert_extension_and_language_dependencies(self, id);
            }
        }

        extensions
    }
}

#[derive(Default)]
struct RemoteSyncExtensions(HashMap<Arc<str>, ExtensionIndexEntry>);

impl RemoteSyncExtensions {
    fn insert_extension_and_language_dependencies(
        &mut self,
        index: &ExtensionIndex,
        id: &Arc<str>,
    ) {
        if self.0.contains_key(id) {
            return;
        }

        let Some(entry) = index.extensions.get(id) else {
            return;
        };

        self.0.insert(id.clone(), entry.clone());

        let Some(remote_load) = entry.manifest.remote_load() else {
            return;
        };

        for language in remote_load.language_dependencies() {
            if let Some(language_entry) = index.languages.get(&language) {
                self.insert_extension_and_language_dependencies(index, &language_entry.extension);
            }
        }
    }

    fn into_entries(self) -> impl Iterator<Item = (Arc<str>, ExtensionIndexEntry)> {
        self.0.into_iter()
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
pub struct ExtensionIndexEntry {
    pub manifest: Arc<ExtensionManifest>,
    pub dev: bool,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Deserialize, Serialize)]
pub struct ExtensionIndexThemeEntry {
    pub extension: Arc<str>,
    pub path: PathBuf,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Deserialize, Serialize)]
pub struct ExtensionIndexIconThemeEntry {
    pub extension: Arc<str>,
    pub path: PathBuf,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Deserialize, Serialize)]
pub struct ExtensionIndexLanguageEntry {
    pub extension: Arc<str>,
    pub path: PathBuf,
    pub matcher: LanguageMatcher,
    pub hidden: bool,
    pub grammar: Option<Arc<str>>,
}

actions!(
    mav,
    [
        /// Reloads all installed extensions.
        ReloadExtensions
    ]
);

pub fn init(
    extension_host_proxy: Arc<ExtensionHostProxy>,
    fs: Arc<dyn Fs>,
    client: Arc<Client>,
    node_runtime: NodeRuntime,
    cx: &mut App,
) {
    let store = cx.new(move |cx| {
        ExtensionStore::new(
            paths::extensions_dir().clone(),
            None,
            extension_host_proxy,
            fs,
            client.http_client(),
            client.http_client(),
            Some(client.telemetry().clone()),
            node_runtime,
            cx,
        )
    });

    cx.on_action(|_: &ReloadExtensions, cx| {
        let store = cx.global::<GlobalExtensionStore>().0.clone();
        store.update(cx, |store, cx| drop(store.reload(None, cx)));
    });

    cx.set_global(GlobalExtensionStore(store));
}
