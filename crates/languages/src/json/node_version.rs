use anyhow::{Context as _, Result, bail};
use async_compression::futures::bufread::GzipDecoder;
use async_tar::Archive;
use async_trait::async_trait;
use futures::StreamExt;
use gpui::AsyncApp;
use http_client::github::{GitHubLspBinaryVersion, latest_github_release};
use language::{LspAdapter, LspAdapterDelegate, LspInstaller, Toolchain};
use lsp::{LanguageServerBinary, LanguageServerName};
use smol::{
    fs::{self},
    io::BufReader,
};
use std::{env::consts, future::Future, path::PathBuf, sync::Arc};
use util::{ResultExt, archive::extract_zip, fs::remove_matching, maybe};

pub struct NodeVersionAdapter;

impl NodeVersionAdapter {
    const SERVER_NAME: LanguageServerName =
        LanguageServerName::new_static("package-version-server");
}

impl LspInstaller for NodeVersionAdapter {
    type BinaryVersion = GitHubLspBinaryVersion;

    async fn fetch_latest_server_version(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        _: bool,
        _: &mut AsyncApp,
    ) -> Result<GitHubLspBinaryVersion> {
        let release = latest_github_release(
            "mav-industries/package-version-server",
            true,
            false,
            delegate.http_client(),
        )
        .await?;
        let os = match consts::OS {
            "macos" => "apple-darwin",
            "linux" => "unknown-linux-gnu",
            "windows" => "pc-windows-msvc",
            other => bail!("Running on unsupported os: {other}"),
        };
        let suffix = if consts::OS == "windows" {
            ".zip"
        } else {
            ".tar.gz"
        };
        let asset_name = format!("{}-{}-{os}{suffix}", Self::SERVER_NAME, consts::ARCH);
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .with_context(|| format!("no asset found matching `{asset_name:?}`"))?;
        Ok(GitHubLspBinaryVersion {
            name: release.tag_name,
            url: asset.browser_download_url.clone(),
            digest: asset.digest.clone(),
        })
    }

    async fn check_if_user_installed(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        _: Option<Toolchain>,
        _: &AsyncApp,
    ) -> Option<LanguageServerBinary> {
        let path = delegate.which(Self::SERVER_NAME.as_ref()).await?;
        Some(LanguageServerBinary {
            path,
            env: None,
            arguments: Default::default(),
        })
    }

    fn fetch_server_binary(
        &self,
        latest_version: GitHubLspBinaryVersion,
        container_dir: PathBuf,
        delegate: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Result<LanguageServerBinary>> + use<> {
        let delegate = delegate.clone();

        async move {
            let version = &latest_version;
            let destination_path = container_dir.join(format!(
                "{}-{}{}",
                Self::SERVER_NAME,
                version.name,
                std::env::consts::EXE_SUFFIX
            ));
            let destination_container_path =
                container_dir.join(format!("{}-{}-tmp", Self::SERVER_NAME, version.name));
            if fs::metadata(&destination_path).await.is_err() {
                let mut response = delegate
                    .http_client()
                    .get(&version.url, Default::default(), true)
                    .await
                    .context("downloading release")?;
                if version.url.ends_with(".zip") {
                    extract_zip(&destination_container_path, response.body_mut()).await?;
                } else if version.url.ends_with(".tar.gz") {
                    let decompressed_bytes = GzipDecoder::new(BufReader::new(response.body_mut()));
                    let archive = Archive::new(decompressed_bytes);
                    archive.unpack(&destination_container_path).await?;
                }

                fs::copy(
                    destination_container_path.join(format!(
                        "{}{}",
                        Self::SERVER_NAME,
                        std::env::consts::EXE_SUFFIX
                    )),
                    &destination_path,
                )
                .await?;
                remove_matching(&container_dir, |entry| entry != destination_path).await;
            }
            Ok(LanguageServerBinary {
                path: destination_path,
                env: None,
                arguments: Default::default(),
            })
        }
    }

    async fn cached_server_binary(
        &self,
        container_dir: PathBuf,
        _delegate: &dyn LspAdapterDelegate,
    ) -> Option<LanguageServerBinary> {
        get_cached_version_server_binary(container_dir).await
    }
}

#[async_trait(?Send)]
impl LspAdapter for NodeVersionAdapter {
    fn name(&self) -> LanguageServerName {
        Self::SERVER_NAME
    }
}

async fn get_cached_version_server_binary(container_dir: PathBuf) -> Option<LanguageServerBinary> {
    maybe!(async {
        let mut last = None;
        let mut entries = fs::read_dir(&container_dir).await?;
        while let Some(entry) = entries.next().await {
            last = Some(entry?.path());
        }

        anyhow::Ok(LanguageServerBinary {
            path: last.context("no cached binary")?,
            env: None,
            arguments: Default::default(),
        })
    })
    .await
    .log_err()
}
