use super::super::*;

impl ExtensionStore {
    pub(super) fn prepare_remote_extension(
        &mut self,
        extension_id: Arc<str>,
        is_dev: bool,
        tmp_dir: PathBuf,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let src_dir = self.extensions_dir().join(extension_id.as_ref());
        let Some(loaded_extension) = self.extension_index.extensions.get(&extension_id).cloned()
        else {
            return Task::ready(Err(anyhow!("extension no longer installed")));
        };
        let fs = self.fs.clone();
        cx.background_spawn(async move {
            const EXTENSION_TOML: &str = "extension.toml";
            const EXTENSION_WASM: &str = "extension.wasm";
            const CONFIG_TOML: &str = LanguageConfig::FILE_NAME;

            if is_dev {
                let manifest_toml = toml::to_string(&loaded_extension.manifest)?;
                fs.save(
                    &tmp_dir.join(EXTENSION_TOML),
                    &Rope::from(manifest_toml),
                    language::LineEnding::Unix,
                )
                .await?;
            } else {
                fs.copy_file(
                    &src_dir.join(EXTENSION_TOML),
                    &tmp_dir.join(EXTENSION_TOML),
                    fs::CopyOptions::default(),
                )
                .await?
            }

            if fs.is_file(&src_dir.join(EXTENSION_WASM)).await {
                fs.copy_file(
                    &src_dir.join(EXTENSION_WASM),
                    &tmp_dir.join(EXTENSION_WASM),
                    fs::CopyOptions::default(),
                )
                .await?
            }

            for language_path in loaded_extension.manifest.languages.iter() {
                if fs
                    .is_file(&src_dir.join(language_path).join(CONFIG_TOML))
                    .await
                {
                    fs.create_dir(&tmp_dir.join(language_path)).await?;
                    fs.copy_file(
                        &src_dir.join(language_path).join(CONFIG_TOML),
                        &tmp_dir.join(language_path).join(CONFIG_TOML),
                        fs::CopyOptions::default(),
                    )
                    .await?
                }
            }

            for (adapter_name, meta) in loaded_extension.manifest.debug_adapters.iter() {
                let schema_path = extension::build_debug_adapter_schema_path(adapter_name, meta)?;

                if fs.is_file(&src_dir.join(&schema_path)).await {
                    if let Some(parent) = schema_path.parent() {
                        fs.create_dir(&tmp_dir.join(parent)).await?
                    }
                    fs.copy_file(
                        &src_dir.join(&schema_path),
                        &tmp_dir.join(&schema_path),
                        fs::CopyOptions::default(),
                    )
                    .await?
                }
            }

            Ok(())
        })
    }

    async fn sync_extensions_to_remotes(
        this: &WeakEntity<Self>,
        client: WeakEntity<RemoteClient>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let extensions = this.update(cx, |this, _cx| {
            this.extension_index
                .extensions_to_sync_to_remote()
                .into_entries()
                .map(|(id, entry)| proto::Extension {
                    id: id.to_string(),
                    version: entry.manifest.version.to_string(),
                    dev: entry.dev,
                })
                .collect()
        })?;

        let response = client
            .update(cx, |client, _cx| {
                client
                    .proto_client()
                    .request(proto::SyncExtensions { extensions })
            })?
            .await?;
        let path_style = client.read_with(cx, |client, _| client.path_style())?;

        for missing_extension in response.missing_extensions.into_iter() {
            let tmp_dir = tempfile::tempdir()?;
            this.update(cx, |this, cx| {
                this.prepare_remote_extension(
                    missing_extension.id.clone().into(),
                    missing_extension.dev,
                    tmp_dir.path().to_owned(),
                    cx,
                )
            })?
            .await?;
            let dest_dir = RemotePathBuf::new(
                path_style
                    .join(&response.tmp_dir, &missing_extension.id)
                    .with_context(|| {
                        format!(
                            "failed to construct destination path: {:?}, {:?}",
                            response.tmp_dir, missing_extension.id,
                        )
                    })?,
                path_style,
            );
            log::info!(
                "Uploading extension {} to {:?}",
                missing_extension.clone().id,
                dest_dir
            );

            client
                .update(cx, |client, cx| {
                    client.upload_directory(tmp_dir.path().to_owned(), dest_dir.clone(), cx)
                })?
                .await?;

            log::info!(
                "Finished uploading extension {}",
                missing_extension.clone().id
            );

            let result = client
                .update(cx, |client, _cx| {
                    client.proto_client().request(proto::InstallExtension {
                        tmp_dir: dest_dir.to_proto(),
                        extension: Some(missing_extension.clone()),
                    })
                })?
                .await;

            if let Err(e) = result {
                log::error!(
                    "Failed to install extension {}: {}",
                    missing_extension.id,
                    e
                );
            }
        }

        anyhow::Ok(())
    }

    pub async fn update_remote_clients(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> Result<()> {
        let clients = this.update(cx, |this, _cx| {
            this.remote_clients.retain(|v| v.upgrade().is_some());
            this.remote_clients.clone()
        })?;

        for client in clients {
            Self::sync_extensions_to_remotes(this, client, cx)
                .await
                .log_err();
        }

        anyhow::Ok(())
    }

    pub fn register_remote_client(
        &mut self,
        client: Entity<RemoteClient>,
        _cx: &mut Context<Self>,
    ) {
        self.remote_clients.push(client.downgrade());
        self.ssh_registered_tx.unbounded_send(()).ok();
    }
}
