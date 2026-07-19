use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use collections::HashMap;
use fs::Fs;
use futures::{AsyncReadExt, future::join_all};
use gpui::{BackgroundExecutor, FutureExt as _, SharedString};
use http_client::{AsyncBody, HttpClient, StatusCode};
use serde::Deserialize;
use util::ResultExt;

use crate::AgentId;

use super::{
    REGISTRY_FETCH_TIMEOUT, REGISTRY_ICON_FETCH_TIMEOUT, REGISTRY_URL, RegistryAgent,
    RegistryAgentMetadata, RegistryBinaryAgent, RegistryNpxAgent, RegistryTargetConfig,
};

pub(super) struct RegistryFetchResult {
    pub(super) index: RegistryIndex,
    pub(super) raw_body: Vec<u8>,
}

pub(super) async fn fetch_registry_index(
    http_client: Arc<dyn HttpClient>,
    executor: &BackgroundExecutor,
) -> Result<RegistryFetchResult> {
    let (status, body) =
        fetch_url_body(http_client, REGISTRY_URL, REGISTRY_FETCH_TIMEOUT, executor)
            .await
            .context("fetching ACP registry")?;

    if status.is_client_error() {
        let text = String::from_utf8_lossy(body.as_slice());
        bail!(
            "registry status error {}, response: {text:?}",
            status.as_u16()
        );
    }

    let index: RegistryIndex = serde_json::from_slice(&body).context("parsing ACP registry")?;
    Ok(RegistryFetchResult {
        index,
        raw_body: body,
    })
}

pub(super) async fn build_registry_agents(
    fs: Arc<dyn Fs>,
    http_client: Arc<dyn HttpClient>,
    index: RegistryIndex,
    raw_body: Vec<u8>,
    update_cache: bool,
    executor: &BackgroundExecutor,
) -> Result<Vec<RegistryAgent>> {
    let cache_dir = registry_cache_dir();
    fs.create_dir(&cache_dir).await?;

    let cache_path = cache_dir.join("registry.json");
    if update_cache {
        fs.write(&cache_path, &raw_body).await?;
    }

    let icons_dir = cache_dir.join("icons");
    if update_cache {
        fs.create_dir(&icons_dir).await?;
    }

    let current_platform = current_platform_key();
    let icon_paths = resolve_icon_paths(
        &index.agents,
        &icons_dir,
        update_cache,
        fs.clone(),
        http_client.clone(),
        executor,
    )
    .await;

    let mut agents = Vec::new();
    for (entry, icon_path) in index.agents.into_iter().zip(icon_paths) {
        let metadata = RegistryAgentMetadata {
            id: AgentId::new(entry.id),
            name: entry.name.into(),
            description: entry.description.into(),
            version: entry.version.into(),
            repository: entry.repository.map(Into::into),
            website: entry.website.map(Into::into),
            icon_path,
        };

        let binary_agent = entry.distribution.binary.as_ref().and_then(|binary| {
            if binary.is_empty() {
                return None;
            }

            let mut targets = HashMap::default();
            for (platform, target) in binary.iter() {
                targets.insert(
                    platform.clone(),
                    RegistryTargetConfig {
                        archive: target.archive.clone(),
                        cmd: target.cmd.clone(),
                        args: target.args.clone(),
                        sha256: None,
                        env: target.env.clone(),
                    },
                );
            }

            let supports_current_platform = current_platform
                .as_ref()
                .is_some_and(|platform| targets.contains_key(*platform));

            Some(RegistryBinaryAgent {
                metadata: metadata.clone(),
                targets,
                supports_current_platform,
            })
        });

        let npx_agent = entry.distribution.npx.as_ref().map(|npx| RegistryNpxAgent {
            metadata: metadata.clone(),
            package: npx.package.clone().into(),
            args: npx.args.clone(),
            env: npx.env.clone(),
        });

        let agent = match (binary_agent, npx_agent) {
            (Some(binary_agent), Some(npx_agent)) => {
                if binary_agent.supports_current_platform {
                    RegistryAgent::Binary(binary_agent)
                } else {
                    RegistryAgent::Npx(npx_agent)
                }
            }
            (Some(binary_agent), None) => RegistryAgent::Binary(binary_agent),
            (None, Some(npx_agent)) => RegistryAgent::Npx(npx_agent),
            (None, None) => continue,
        };

        agents.push(agent);
    }

    Ok(agents)
}

async fn resolve_icon_paths(
    entries: &[RegistryEntry],
    icons_dir: &Path,
    update_cache: bool,
    fs: Arc<dyn Fs>,
    http_client: Arc<dyn HttpClient>,
    executor: &BackgroundExecutor,
) -> Vec<Option<SharedString>> {
    join_all(entries.iter().map(|entry| {
        let fs = fs.clone();
        let http_client = http_client.clone();
        async move {
            resolve_icon_path(entry, icons_dir, update_cache, fs, http_client, executor)
                .await
                .log_err()
                .flatten()
        }
    }))
    .await
}

async fn resolve_icon_path(
    entry: &RegistryEntry,
    icons_dir: &Path,
    update_cache: bool,
    fs: Arc<dyn Fs>,
    http_client: Arc<dyn HttpClient>,
    executor: &BackgroundExecutor,
) -> Result<Option<SharedString>> {
    let icon_url = resolve_icon_url(entry);
    let Some(icon_url) = icon_url else {
        return Ok(None);
    };

    let icon_path = icons_dir.join(format!("{}.svg", entry.id));
    if update_cache && !fs.is_file(&icon_path).await {
        if let Err(error) = download_icon(fs.clone(), http_client, &icon_url, entry, executor).await
        {
            log::warn!(
                "Failed to download ACP registry icon for {}: {error:#}",
                entry.id
            );
        }
    }

    if fs.is_file(&icon_path).await {
        Ok(Some(SharedString::from(
            icon_path.to_string_lossy().into_owned(),
        )))
    } else {
        Ok(None)
    }
}

async fn download_icon(
    fs: Arc<dyn Fs>,
    http_client: Arc<dyn HttpClient>,
    icon_url: &str,
    entry: &RegistryEntry,
    executor: &BackgroundExecutor,
) -> Result<()> {
    let (status, body) =
        fetch_url_body(http_client, icon_url, REGISTRY_ICON_FETCH_TIMEOUT, executor)
            .await
            .with_context(|| format!("fetching icon for {}", entry.id))?;

    if status.is_client_error() {
        let text = String::from_utf8_lossy(body.as_slice());
        bail!("icon status error {}, response: {text:?}", status.as_u16());
    }

    let icon_path = registry_cache_dir()
        .join("icons")
        .join(format!("{}.svg", entry.id));
    fs.write(&icon_path, &body).await?;
    Ok(())
}

async fn fetch_url_body(
    http_client: Arc<dyn HttpClient>,
    url: &str,
    timeout: Duration,
    executor: &BackgroundExecutor,
) -> Result<(StatusCode, Vec<u8>)> {
    async {
        let mut response = http_client
            .get(url, AsyncBody::default(), true)
            .await
            .with_context(|| format!("requesting {url}"))?;

        let status = response.status();
        let mut body = Vec::new();
        response
            .body_mut()
            .read_to_end(&mut body)
            .await
            .with_context(|| format!("reading response from {url}"))?;

        Ok((status, body))
    }
    .with_timeout(timeout, executor)
    .await
    .map_err(|_| {
        anyhow!(
            "timed out after {}s while fetching {url}",
            timeout.as_secs()
        )
    })?
}

fn resolve_icon_url(entry: &RegistryEntry) -> Option<String> {
    let icon = entry.icon.as_ref()?;
    if icon.starts_with("https://") || icon.starts_with("http://") {
        return Some(icon.to_string());
    }

    let relative_icon = icon.trim_start_matches("./");
    Some(format!(
        "https://raw.githubusercontent.com/agentclientprotocol/registry/main/{}/{relative_icon}",
        entry.id
    ))
}

fn current_platform_key() -> Option<&'static str> {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        return None;
    };

    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        return None;
    };

    Some(match os {
        "darwin" => match arch {
            "aarch64" => "darwin-aarch64",
            "x86_64" => "darwin-x86_64",
            _ => return None,
        },
        "linux" => match arch {
            "aarch64" => "linux-aarch64",
            "x86_64" => "linux-x86_64",
            _ => return None,
        },
        "windows" => match arch {
            "aarch64" => "windows-aarch64",
            "x86_64" => "windows-x86_64",
            _ => return None,
        },
        _ => return None,
    })
}

fn registry_cache_dir() -> PathBuf {
    paths::external_agents_dir().join("registry")
}

pub(super) fn registry_cache_path() -> PathBuf {
    registry_cache_dir().join("registry.json")
}

#[derive(Deserialize)]
pub(super) struct RegistryIndex {
    #[serde(rename = "version")]
    _version: String,
    agents: Vec<RegistryEntry>,
}

#[derive(Deserialize)]
struct RegistryEntry {
    id: String,
    name: String,
    version: String,
    description: String,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    website: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    distribution: RegistryDistribution,
}

#[derive(Deserialize)]
struct RegistryDistribution {
    #[serde(default)]
    binary: Option<HashMap<String, RegistryBinaryTarget>>,
    #[serde(default)]
    npx: Option<RegistryNpxDistribution>,
}

#[derive(Deserialize)]
struct RegistryBinaryTarget {
    archive: String,
    cmd: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RegistryNpxDistribution {
    package: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}
