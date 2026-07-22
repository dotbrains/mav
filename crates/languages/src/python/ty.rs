use super::*;

pub struct TyLspAdapter {
    fs: Arc<dyn Fs>,
}

#[cfg(target_os = "macos")]
impl TyLspAdapter {
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::TarGz;
    const ARCH_SERVER_NAME: &str = "apple-darwin";
}

#[cfg(target_os = "linux")]
impl TyLspAdapter {
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::TarGz;
    const ARCH_SERVER_NAME: &str = "unknown-linux-gnu";
}

#[cfg(target_os = "freebsd")]
impl TyLspAdapter {
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::TarGz;
    const ARCH_SERVER_NAME: &str = "unknown-freebsd";
}

#[cfg(target_os = "windows")]
impl TyLspAdapter {
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::Zip;
    const ARCH_SERVER_NAME: &str = "pc-windows-msvc";
}

impl TyLspAdapter {
    const SERVER_NAME: LanguageServerName = LanguageServerName::new_static("ty");

    pub fn new(fs: Arc<dyn Fs>) -> TyLspAdapter {
        TyLspAdapter { fs }
    }

    fn build_asset_name() -> Result<(String, String)> {
        let arch = match consts::ARCH {
            "x86" => "i686",
            _ => consts::ARCH,
        };
        let os = Self::ARCH_SERVER_NAME;
        let suffix = match consts::OS {
            "windows" => "zip",
            _ => "tar.gz",
        };
        let asset_name = format!("ty-{arch}-{os}.{suffix}");
        let asset_stem = format!("ty-{arch}-{os}");
        Ok((asset_stem, asset_name))
    }
}

#[async_trait(?Send)]
impl LspAdapter for TyLspAdapter {
    fn name(&self) -> LanguageServerName {
        Self::SERVER_NAME
    }

    async fn label_for_completion(
        &self,
        item: &lsp::CompletionItem,
        language: &Arc<language::Language>,
    ) -> Option<language::CodeLabel> {
        let label = &item.label;
        let label_len = label.len();
        let grammar = language.grammar()?;
        let highlight_id = highlight_id_for_completion(item.kind?, grammar)?;

        let mut text = label.clone();
        if let Some(completion_details) = item
            .label_details
            .as_ref()
            .and_then(|details| details.detail.as_ref())
        {
            write!(&mut text, " {}", completion_details).ok();
        }

        Some(language::CodeLabel::filtered(
            text,
            label_len,
            item.filter_text.as_deref(),
            highlight_id
                .map(|id| (0..label_len, id))
                .into_iter()
                .collect(),
        ))
    }

    async fn label_for_symbol(
        &self,
        symbol: &language::Symbol,
        language: &Arc<language::Language>,
    ) -> Option<language::CodeLabel> {
        label_for_python_symbol(symbol, language)
    }

    async fn workspace_configuration(
        self: Arc<Self>,
        delegate: &Arc<dyn LspAdapterDelegate>,
        toolchain: Option<Toolchain>,
        _: Option<Uri>,
        cx: &mut AsyncApp,
    ) -> Result<Value> {
        let mut ret = cx
            .update(|cx| {
                language_server_settings(delegate.as_ref(), &self.name(), cx)
                    .and_then(|s| s.settings.clone())
            })
            .unwrap_or_else(|| json!({}));
        if let Some(toolchain) = toolchain.and_then(|toolchain| {
            serde_json::from_value::<PythonToolchainData>(toolchain.as_json).ok()
        }) {
            _ = maybe!({
                let uri =
                    url::Url::from_file_path(toolchain.environment.executable.as_ref()?).ok()?;
                let sys_prefix = toolchain.environment.prefix.clone()?;
                let environment = json!({
                    "executable": {
                        "uri": uri,
                        "sysPrefix": sys_prefix
                    }
                });
                ret.as_object_mut()?
                    .entry("pythonExtension")
                    .or_insert_with(|| json!({ "activeEnvironment": environment }));
                Some(())
            });
        }
        Ok(json!({"ty": ret}))
    }
}

impl LspInstaller for TyLspAdapter {
    type BinaryVersion = GitHubLspBinaryVersion;
    async fn fetch_latest_server_version(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        _: bool,
        _: &mut AsyncApp,
    ) -> Result<Self::BinaryVersion> {
        let release =
            latest_github_release("astral-sh/ty", true, false, delegate.http_client()).await?;
        let (_, asset_name) = Self::build_asset_name()?;
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

    async fn check_if_user_installed(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        toolchain: Option<Toolchain>,
        _: &AsyncApp,
    ) -> Option<LanguageServerBinary> {
        let ty_in_venv = if let Some(toolchain) = toolchain
            && toolchain.language_name.as_ref() == "Python"
        {
            Path::new(toolchain.path.as_str())
                .parent()
                .map(|path| path.join("ty"))
        } else {
            None
        };

        for path in ty_in_venv.into_iter().chain(["ty".into()]) {
            if let Some(ty_bin) = delegate.which(path.as_os_str()).await {
                let env = delegate.shell_env().await;
                return Some(LanguageServerBinary {
                    path: ty_bin,
                    env: Some(env),
                    arguments: vec!["server".into()],
                });
            }
        }

        None
    }

    fn fetch_server_binary(
        &self,
        latest_version: Self::BinaryVersion,
        container_dir: PathBuf,
        delegate: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Result<LanguageServerBinary>> + use<> {
        let delegate = delegate.clone();

        async move {
            let GitHubLspBinaryVersion {
                name,
                url,
                digest: expected_digest,
            } = latest_version;
            let destination_path = container_dir.join(format!("ty-{name}"));

            async_fs::create_dir_all(&destination_path).await?;

            let server_path = match Self::GITHUB_ASSET_KIND {
                AssetKind::TarGz | AssetKind::TarBz2 | AssetKind::Gz => destination_path
                    .join(Self::build_asset_name()?.0)
                    .join("ty"),
                AssetKind::Zip => destination_path.clone().join("ty.exe"),
            };

            let binary = LanguageServerBinary {
                path: server_path.clone(),
                env: None,
                arguments: vec!["server".into()],
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
                arguments: vec!["server".into()],
            })
        }
    }

    async fn cached_server_binary(
        &self,
        container_dir: PathBuf,
        _: &dyn LspAdapterDelegate,
    ) -> Option<LanguageServerBinary> {
        maybe!(async {
            let mut last = None;
            let mut entries = self.fs.read_dir(&container_dir).await?;
            while let Some(entry) = entries.next().await {
                let path = entry?;
                if path.extension().is_some_and(|ext| ext == "metadata") {
                    continue;
                }
                last = Some(path);
            }

            let path = last.context("no cached binary")?;
            let path = match TyLspAdapter::GITHUB_ASSET_KIND {
                AssetKind::TarGz | AssetKind::TarBz2 | AssetKind::Gz => {
                    path.join(Self::build_asset_name()?.0).join("ty")
                }
                AssetKind::Zip => path.join("ty.exe"),
            };

            anyhow::Ok(LanguageServerBinary {
                path,
                env: None,
                arguments: vec!["server".into()],
            })
        })
        .await
        .log_err()
    }
}
