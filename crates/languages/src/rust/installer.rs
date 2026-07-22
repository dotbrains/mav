use super::*;

impl LspInstaller for RustLspAdapter {
    type BinaryVersion = GitHubLspBinaryVersion;
    async fn check_if_user_installed(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        _: Option<Toolchain>,
        cx: &AsyncApp,
    ) -> Option<LanguageServerBinary> {
        let delegate = delegate.clone();
        cx.background_spawn(async move {
            let env = delegate.shell_env().await;
            if let Some(path) = Self::rustup_rust_analyzer_for_worktree(delegate.as_ref()).await {
                let result = delegate
                    .try_exec(LanguageServerBinary {
                        path: path.clone(),
                        arguments: vec!["--help".into()],
                        env: Some(env.clone()),
                    })
                    .await;
                if result.is_ok() {
                    log::debug!("found rust-analyzer in rustup toolchain override");
                    return Some(LanguageServerBinary {
                        path,
                        env: Some(env),
                        arguments: vec![],
                    });
                }
            }

            let path = delegate.which("rust-analyzer".as_ref()).await?;

            // It is surprisingly common for ~/.cargo/bin/rust-analyzer to be a symlink to
            // /usr/bin/rust-analyzer that fails when you run it; so we need to test it.
            log::debug!("found rust-analyzer in PATH. trying to run `rust-analyzer --help`");
            let result = delegate
                .try_exec(LanguageServerBinary {
                    path: path.clone(),
                    arguments: vec!["--help".into()],
                    env: Some(env.clone()),
                })
                .await;
            if let Err(err) = result {
                log::debug!(
                    "failed to run rust-analyzer after detecting it in PATH: binary: {:?}: {}",
                    path,
                    err
                );
                return None;
            }

            Some(LanguageServerBinary {
                path,
                env: Some(env),
                arguments: vec![],
            })
        })
        .await
    }

    async fn fetch_latest_server_version(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        pre_release: bool,
        _: &mut AsyncApp,
    ) -> Result<GitHubLspBinaryVersion> {
        let release = latest_github_release(
            "rust-lang/rust-analyzer",
            true,
            pre_release,
            delegate.http_client(),
        )
        .await?;
        let asset_name = Self::build_asset_name().await;
        let asset = release
            .assets
            .into_iter()
            .find(|asset| asset.name == asset_name)
            .with_context(|| format!("no asset found matching `{asset_name:?}`"))?;
        Ok(GitHubLspBinaryVersion {
            name: release.tag_name,
            url: asset.browser_download_url,
            digest: asset.digest,
        })
    }

    fn fetch_server_binary(
        &self,
        version: GitHubLspBinaryVersion,
        container_dir: PathBuf,
        delegate: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Result<LanguageServerBinary>> + use<> {
        let delegate = delegate.clone();

        async move {
            let GitHubLspBinaryVersion {
                name,
                url,
                digest: expected_digest,
            } = version;
            let destination_path = container_dir.join(format!("rust-analyzer-{name}"));
            let server_path = match Self::GITHUB_ASSET_KIND {
                AssetKind::TarGz | AssetKind::TarBz2 | AssetKind::Gz => destination_path.clone(), // Tar and gzip extract in place.
                AssetKind::Zip => destination_path.clone().join("rust-analyzer.exe"), // zip contains a .exe
            };

            let binary = LanguageServerBinary {
                path: server_path.clone(),
                env: None,
                arguments: Default::default(),
            };

            let metadata_path = destination_path.with_extension("metadata");
            let metadata = GithubBinaryMetadata::read_from_file(&metadata_path)
                .await
                .ok();
            if let Some(metadata) = metadata {
                let validity_check = async || {
                    delegate
                        .try_exec(LanguageServerBinary {
                            path: server_path.clone(),
                            arguments: vec!["--version".into()],
                            env: None,
                        })
                        .await
                        .inspect_err(|err| {
                            log::warn!(
                                "Unable to run {server_path:?} asset, redownloading: {err:#}",
                            )
                        })
                };
                if let (Some(actual_digest), Some(expected_digest)) =
                    (&metadata.digest, &expected_digest)
                {
                    if actual_digest == expected_digest {
                        if validity_check().await.is_ok() {
                            return Ok(binary);
                        }
                    } else {
                        log::info!(
                            "SHA-256 mismatch for {destination_path:?} asset, downloading new asset. Expected: {expected_digest}, Got: {actual_digest}"
                        );
                    }
                } else if validity_check().await.is_ok() {
                    return Ok(binary);
                }
            }

            download_server_binary(
                &*delegate.http_client(),
                &url,
                expected_digest.as_deref(),
                &destination_path,
                Self::GITHUB_ASSET_KIND,
            )
            .await?;
            make_file_executable(&server_path).await?;
            remove_matching(&container_dir, |path| path != destination_path).await;
            GithubBinaryMetadata::write_to_file(
                &GithubBinaryMetadata {
                    metadata_version: 1,
                    digest: expected_digest,
                },
                &metadata_path,
            )
            .await?;

            Ok(LanguageServerBinary {
                path: server_path,
                env: None,
                arguments: Default::default(),
            })
        }
    }

    async fn cached_server_binary(
        &self,
        container_dir: PathBuf,
        _: &dyn LspAdapterDelegate,
    ) -> Option<LanguageServerBinary> {
        get_cached_server_binary(container_dir).await
    }
}
