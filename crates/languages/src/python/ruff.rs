use super::*;

pub(crate) struct RuffLspAdapter {
    fs: Arc<dyn Fs>,
}

impl RuffLspAdapter {
    fn convert_ruff_schema(raw_schema: &serde_json::Value) -> serde_json::Value {
        let Some(schema_object) = raw_schema.as_object() else {
            return raw_schema.clone();
        };

        let mut root_properties = serde_json::Map::new();

        for (key, value) in schema_object {
            let parts: Vec<&str> = key.split('.').collect();

            if parts.is_empty() {
                continue;
            }

            let mut current = &mut root_properties;

            for (i, part) in parts.iter().enumerate() {
                let is_last = i == parts.len() - 1;

                if is_last {
                    let mut schema_entry = serde_json::Map::new();

                    if let Some(doc) = value.get("doc").and_then(|d| d.as_str()) {
                        schema_entry.insert(
                            "markdownDescription".to_string(),
                            serde_json::Value::String(doc.to_string()),
                        );
                    }

                    if let Some(default_val) = value.get("default") {
                        schema_entry.insert("default".to_string(), default_val.clone());
                    }

                    if let Some(value_type) = value.get("value_type").and_then(|v| v.as_str()) {
                        if value_type.contains('|') {
                            let enum_values: Vec<serde_json::Value> = value_type
                                .split('|')
                                .map(|s| s.trim().trim_matches('"'))
                                .filter(|s| !s.is_empty())
                                .map(|s| serde_json::Value::String(s.to_string()))
                                .collect();

                            if !enum_values.is_empty() {
                                schema_entry
                                    .insert("type".to_string(), serde_json::json!("string"));
                                schema_entry.insert(
                                    "enum".to_string(),
                                    serde_json::Value::Array(enum_values),
                                );
                            }
                        } else if value_type.starts_with("list[") {
                            schema_entry.insert("type".to_string(), serde_json::json!("array"));
                            if let Some(item_type) = value_type
                                .strip_prefix("list[")
                                .and_then(|s| s.strip_suffix(']'))
                            {
                                let json_type = match item_type {
                                    "str" => "string",
                                    "int" => "integer",
                                    "bool" => "boolean",
                                    _ => "string",
                                };
                                schema_entry.insert(
                                    "items".to_string(),
                                    serde_json::json!({"type": json_type}),
                                );
                            }
                        } else if value_type.starts_with("dict[") {
                            schema_entry.insert("type".to_string(), serde_json::json!("object"));
                        } else {
                            let json_type = match value_type {
                                "bool" => "boolean",
                                "int" | "usize" => "integer",
                                "str" => "string",
                                _ => "string",
                            };
                            schema_entry.insert(
                                "type".to_string(),
                                serde_json::Value::String(json_type.to_string()),
                            );
                        }
                    }

                    current.insert(part.to_string(), serde_json::Value::Object(schema_entry));
                } else {
                    let next_current = current
                        .entry(part.to_string())
                        .or_insert_with(|| {
                            serde_json::json!({
                                "type": "object",
                                "properties": {}
                            })
                        })
                        .as_object_mut()
                        .expect("should be an object")
                        .entry("properties")
                        .or_insert_with(|| serde_json::json!({}))
                        .as_object_mut()
                        .expect("properties should be an object");

                    current = next_current;
                }
            }
        }

        serde_json::json!({
            "type": "object",
            "properties": root_properties
        })
    }
}

#[cfg(target_os = "macos")]
impl RuffLspAdapter {
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::TarGz;
    const ARCH_SERVER_NAME: &str = "apple-darwin";
}

#[cfg(target_os = "linux")]
impl RuffLspAdapter {
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::TarGz;
    const ARCH_SERVER_NAME: &str = "unknown-linux-gnu";
}

#[cfg(target_os = "freebsd")]
impl RuffLspAdapter {
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::TarGz;
    const ARCH_SERVER_NAME: &str = "unknown-freebsd";
}

#[cfg(target_os = "windows")]
impl RuffLspAdapter {
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::Zip;
    const ARCH_SERVER_NAME: &str = "pc-windows-msvc";
}

impl RuffLspAdapter {
    const SERVER_NAME: LanguageServerName = LanguageServerName::new_static("ruff");

    pub fn new(fs: Arc<dyn Fs>) -> RuffLspAdapter {
        RuffLspAdapter { fs }
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
        let asset_name = format!("ruff-{arch}-{os}.{suffix}");
        let asset_stem = format!("ruff-{arch}-{os}");
        Ok((asset_stem, asset_name))
    }
}

#[async_trait(?Send)]
impl LspAdapter for RuffLspAdapter {
    fn name(&self) -> LanguageServerName {
        Self::SERVER_NAME
    }

    async fn initialization_options_schema(
        self: Arc<Self>,
        delegate: &Arc<dyn LspAdapterDelegate>,
        cached_binary: OwnedMutexGuard<Option<(bool, LanguageServerBinary)>>,
        cx: &mut AsyncApp,
    ) -> Option<serde_json::Value> {
        let binary = self
            .get_language_server_command(
                delegate.clone(),
                None,
                LanguageServerBinaryOptions {
                    allow_path_lookup: true,
                    allow_binary_download: false,
                    pre_release: false,
                },
                cached_binary,
                cx.clone(),
            )
            .await
            .0
            .ok()?;

        let mut command = util::command::new_command(&binary.path);
        command
            .args(&["config", "--output-format", "json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let cmd = command
            .spawn()
            .map_err(|e| log::debug!("failed to spawn command {command:?}: {e}"))
            .ok()?;
        let output = cmd
            .output()
            .await
            .map_err(|e| log::debug!("failed to execute command {command:?}: {e}"))
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let raw_schema: serde_json::Value = serde_json::from_slice(output.stdout.as_slice())
            .map_err(|e| log::debug!("failed to parse ruff's JSON schema output: {e}"))
            .ok()?;

        let converted_schema = Self::convert_ruff_schema(&raw_schema);
        Some(converted_schema)
    }
}

impl LspInstaller for RuffLspAdapter {
    type BinaryVersion = GitHubLspBinaryVersion;
    async fn check_if_user_installed(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        toolchain: Option<Toolchain>,
        _: &AsyncApp,
    ) -> Option<LanguageServerBinary> {
        let ruff_in_venv = if let Some(toolchain) = toolchain
            && toolchain.language_name.as_ref() == "Python"
        {
            Path::new(toolchain.path.as_str())
                .parent()
                .map(|path| path.join("ruff"))
        } else {
            None
        };

        for path in ruff_in_venv.into_iter().chain(["ruff".into()]) {
            if let Some(ruff_bin) = delegate.which(path.as_os_str()).await {
                let env = delegate.shell_env().await;
                return Some(LanguageServerBinary {
                    path: ruff_bin,
                    env: Some(env),
                    arguments: vec!["server".into()],
                });
            }
        }

        None
    }

    async fn fetch_latest_server_version(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        _: bool,
        _: &mut AsyncApp,
    ) -> Result<GitHubLspBinaryVersion> {
        let release =
            latest_github_release("astral-sh/ruff", true, false, delegate.http_client()).await?;
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

    fn fetch_server_binary(
        &self,
        latest_version: GitHubLspBinaryVersion,
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
            let destination_path = container_dir.join(format!("ruff-{name}"));
            let server_path = match Self::GITHUB_ASSET_KIND {
                AssetKind::TarGz | AssetKind::TarBz2 | AssetKind::Gz => destination_path
                    .join(Self::build_asset_name()?.0)
                    .join("ruff"),
                AssetKind::Zip => destination_path.clone().join("ruff.exe"),
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
            let path = match Self::GITHUB_ASSET_KIND {
                AssetKind::TarGz | AssetKind::TarBz2 | AssetKind::Gz => {
                    path.join(Self::build_asset_name()?.0).join("ruff")
                }
                AssetKind::Zip => path.join("ruff.exe"),
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
