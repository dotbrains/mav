use super::super::*;

impl ExtensionStore {
    /// Installs any extensions that should be included with Mav by default.
    ///
    /// This can be used to make certain functionality provided by extensions
    /// available out-of-the-box.
    pub fn auto_install_extensions(&mut self, cx: &mut Context<Self>) {
        if cfg!(test) {
            return;
        }

        let extension_settings = ExtensionSettings::get_global(cx);

        let extensions_to_install = extension_settings
            .auto_install_extensions
            .keys()
            .filter(|extension_id| extension_settings.should_auto_install(extension_id))
            .filter(|extension_id| {
                let is_already_installed = self
                    .extension_index
                    .extensions
                    .contains_key(extension_id.as_ref());
                !is_already_installed && !SUPPRESSED_EXTENSIONS.contains(extension_id.as_ref())
            })
            .cloned()
            .collect::<Vec<_>>();

        cx.spawn(async move |this, cx| {
            for extension_id in extensions_to_install {
                this.update(cx, |this, cx| {
                    this.install_latest_extension(extension_id.clone(), cx);
                })
                .ok();
            }
        })
        .detach();
    }

    pub fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        let task = self.fetch_extensions_with_update_available(cx);
        cx.spawn(async move |this, cx| Self::upgrade_extensions(this, task.await?, cx).await)
            .detach();
    }

    async fn upgrade_extensions(
        this: WeakEntity<Self>,
        extensions: Vec<ExtensionMetadata>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        for extension in extensions {
            let task = this.update(cx, |this, cx| {
                if let Some(installed_extension) =
                    this.extension_index.extensions.get(&extension.id)
                {
                    let installed_version =
                        Version::from_str(&installed_extension.manifest.version).ok()?;
                    let latest_version = Version::from_str(&extension.manifest.version).ok()?;

                    if installed_version >= latest_version {
                        return None;
                    }
                }

                Some(this.upgrade_extension(extension.id, extension.manifest.version, cx))
            })?;

            if let Some(task) = task {
                task.await.log_err();
            }
        }
        anyhow::Ok(())
    }

    pub(super) fn fetch_extensions_from_api(
        &self,
        path: &str,
        query: &[(&str, &str)],
        cx: &mut Context<ExtensionStore>,
    ) -> Task<Result<Vec<ExtensionMetadata>>> {
        let url = self.http_client.build_mav_api_url(path, query);
        let http_client = self.http_client.clone();
        cx.spawn(async move |_, _| {
            let mut response = http_client
                .get(url?.as_ref(), AsyncBody::empty(), true)
                .await?;

            let mut body = Vec::new();
            response
                .body_mut()
                .read_to_end(&mut body)
                .await
                .context("error reading extensions")?;

            if response.status().is_client_error() {
                let text = String::from_utf8_lossy(body.as_slice());
                bail!(
                    "status error {}, response: {text:?}",
                    response.status().as_u16()
                );
            }

            let mut response: GetExtensionsResponse = serde_json::from_slice(&body)?;

            response
                .data
                .retain(|extension| !SUPPRESSED_EXTENSIONS.contains(extension.id.as_ref()));

            Ok(response.data)
        })
    }

    pub fn install_extension(
        &mut self,
        extension_id: Arc<str>,
        version: Arc<str>,
        cx: &mut Context<Self>,
    ) {
        self.install_or_upgrade_extension(extension_id, version, ExtensionOperation::Install, cx)
            .detach_and_log_err(cx);
    }

    fn install_or_upgrade_extension_at_endpoint(
        &mut self,
        extension_id: Arc<str>,
        url: Url,
        operation: ExtensionOperation,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let extension_dir = self.installed_dir.join(extension_id.as_ref());
        let staging_dir = self.staging_dir.clone();
        let http_client = self.http_client.clone();
        let fs = self.fs.clone();

        match self.outstanding_operations.entry(extension_id.clone()) {
            btree_map::Entry::Occupied(_) => return Task::ready(Ok(())),
            btree_map::Entry::Vacant(e) => e.insert(operation),
        };
        cx.notify();

        cx.spawn(async move |this, cx| {
            let _finish = cx.on_drop(&this, {
                let extension_id = extension_id.clone();
                move |this, cx| {
                    this.outstanding_operations.remove(extension_id.as_ref());
                    cx.notify();
                }
            });

            cx.background_spawn(async move {
                let mut response = http_client
                    .get(url.as_ref(), Default::default(), true)
                    .await
                    .context("downloading extension")?;

                let content_length = response
                    .headers()
                    .get(http_client::http::header::CONTENT_LENGTH)
                    .and_then(|value| value.to_str().ok()?.parse::<usize>().ok());

                let mut body = BufReader::new(response.body_mut());
                let mut tar_gz_bytes = Vec::new();
                body.read_to_end(&mut tar_gz_bytes).await?;

                if let Some(content_length) = content_length {
                    let actual_len = tar_gz_bytes.len();
                    if content_length != actual_len {
                        bail!(
                            "downloaded extension size {actual_len} \
                        does not match content length {content_length}"
                        );
                    }
                }

                let decompressed_bytes = GzipDecoder::new(BufReader::new(tar_gz_bytes.as_slice()));
                let archive = Archive::new(decompressed_bytes);

                let remove_dir = || {
                    fs.remove_dir(
                        &extension_dir,
                        RemoveOptions {
                            recursive: true,
                            ignore_if_not_exists: true,
                        },
                    )
                };

                let temp_dir = fs
                    .create_dir(&staging_dir)
                    .await
                    .and_then(|()| tempfile::tempdir_in(&staging_dir).map_err(Into::into));

                match temp_dir {
                    Ok(temp_dir) => {
                        archive.unpack(temp_dir.path()).await?;
                        remove_dir().await?;
                        fs.rename(
                            temp_dir.path(),
                            &extension_dir,
                            RenameOptions {
                                overwrite: true,
                                ignore_if_exists: true,
                                create_parents: true,
                            },
                        )
                        .await
                    }
                    Err(_) => {
                        remove_dir().await?;
                        archive.unpack(extension_dir).await.map_err(Into::into)
                    }
                }
            })
            .await?;

            this.update(cx, |this, cx| this.reload(Some(extension_id.clone()), cx))?
                .await;

            if let ExtensionOperation::Install = operation {
                this.update(cx, |this, cx| {
                    cx.emit(Event::ExtensionInstalled(extension_id.clone()));
                    if let Some(events) = ExtensionEvents::try_global(cx)
                        && let Some(manifest) = this.extension_manifest_for_id(&extension_id)
                    {
                        events.update(cx, |this, cx| {
                            this.emit(extension::Event::ExtensionInstalled(manifest.clone()), cx)
                        });
                    }
                })
                .ok();
            }

            anyhow::Ok(())
        })
    }

    pub fn install_latest_extension(&mut self, extension_id: Arc<str>, cx: &mut Context<Self>) {
        log::info!("installing extension {extension_id} latest version");

        let schema_versions = schema_version_range();
        let wasm_api_versions = wasm_api_version_range(ReleaseChannel::global(cx));

        let Some(url) = self
            .http_client
            .build_mav_api_url(
                &format!("/extensions/{extension_id}/download"),
                &[
                    ("min_schema_version", &schema_versions.start().to_string()),
                    ("max_schema_version", &schema_versions.end().to_string()),
                    (
                        "min_wasm_api_version",
                        &wasm_api_versions.start().to_string(),
                    ),
                    ("max_wasm_api_version", &wasm_api_versions.end().to_string()),
                ],
            )
            .log_err()
        else {
            return;
        };

        self.install_or_upgrade_extension_at_endpoint(
            extension_id,
            url,
            ExtensionOperation::Install,
            cx,
        )
        .detach_and_log_err(cx);
    }

    pub fn upgrade_extension(
        &mut self,
        extension_id: Arc<str>,
        version: Arc<str>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.install_or_upgrade_extension(extension_id, version, ExtensionOperation::Upgrade, cx)
    }

    fn install_or_upgrade_extension(
        &mut self,
        extension_id: Arc<str>,
        version: Arc<str>,
        operation: ExtensionOperation,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        log::info!("installing extension {extension_id} {version}");
        let Some(url) = self
            .http_client
            .build_mav_api_url(
                &format!("/extensions/{extension_id}/{version}/download"),
                &[],
            )
            .log_err()
        else {
            return Task::ready(Ok(()));
        };

        self.install_or_upgrade_extension_at_endpoint(extension_id, url, operation, cx)
    }

    pub fn uninstall_extension(
        &mut self,
        extension_id: Arc<str>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let extension_dir = self.installed_dir.join(extension_id.as_ref());
        let work_dir = self.wasm_host.work_dir.join(extension_id.as_ref());
        let fs = self.fs.clone();

        let extension_manifest = self.extension_manifest_for_id(&extension_id).cloned();

        match self.outstanding_operations.entry(extension_id.clone()) {
            btree_map::Entry::Occupied(_) => return Task::ready(Ok(())),
            btree_map::Entry::Vacant(e) => e.insert(ExtensionOperation::Remove),
        };

        cx.spawn(async move |extension_store, cx| {
            let _finish = cx.on_drop(&extension_store, {
                let extension_id = extension_id.clone();
                move |this, cx| {
                    this.outstanding_operations.remove(extension_id.as_ref());
                    cx.notify();
                }
            });

            fs.remove_dir(
                &extension_dir,
                RemoveOptions {
                    recursive: true,
                    ignore_if_not_exists: true,
                },
            )
            .await
            .with_context(|| format!("Removing extension dir {extension_dir:?}"))?;

            extension_store
                .update(cx, |extension_store, cx| extension_store.reload(None, cx))?
                .await;

            // There's a race between wasm extension fully stopping and the directory removal.
            // On Windows, it's impossible to remove a directory that has a process running in it.
            for i in 0..3 {
                cx.background_executor()
                    .timer(Duration::from_millis(i * 100))
                    .await;
                let removal_result = fs
                    .remove_dir(
                        &work_dir,
                        RemoveOptions {
                            recursive: true,
                            ignore_if_not_exists: true,
                        },
                    )
                    .await;
                match removal_result {
                    Ok(()) => break,
                    Err(e) => {
                        if i == 2 {
                            log::error!("Failed to remove extension work dir {work_dir:?} : {e}");
                        }
                    }
                }
            }

            extension_store.update(cx, |_, cx| {
                cx.emit(Event::ExtensionUninstalled(extension_id.clone()));
                if let Some(events) = ExtensionEvents::try_global(cx)
                    && let Some(manifest) = extension_manifest
                {
                    events.update(cx, |this, cx| {
                        this.emit(extension::Event::ExtensionUninstalled(manifest.clone()), cx)
                    });
                }
            })?;

            anyhow::Ok(())
        })
    }
}
