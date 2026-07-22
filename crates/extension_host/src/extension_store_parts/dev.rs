use super::super::*;

impl ExtensionStore {
    pub fn install_dev_extension(
        &mut self,
        extension_source_path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let extensions_dir = self.extensions_dir();
        let fs = self.fs.clone();
        let builder = self.builder.clone();

        cx.spawn(async move |this, cx| {
            let mut extension_manifest =
                ExtensionManifest::load(fs.clone(), &extension_source_path).await?;
            let extension_id = extension_manifest.id.clone();

            if let Some(uninstall_task) = this
                .update(cx, |this, cx| {
                    this.extension_index
                        .extensions
                        .get(extension_id.as_ref())
                        .is_some_and(|index_entry| !index_entry.dev)
                        .then(|| this.uninstall_extension(extension_id.clone(), cx))
                })
                .ok()
                .flatten()
            {
                uninstall_task.await.log_err();
            }

            if !this.update(cx, |this, cx| {
                match this.outstanding_operations.entry(extension_id.clone()) {
                    btree_map::Entry::Occupied(_) => return false,
                    btree_map::Entry::Vacant(e) => e.insert(ExtensionOperation::Install),
                };
                cx.notify();
                true
            })? {
                return Ok(());
            }

            let _finish = cx.on_drop(&this, {
                let extension_id = extension_id.clone();
                move |this, cx| {
                    this.outstanding_operations.remove(extension_id.as_ref());
                    cx.notify();
                }
            });

            cx.background_spawn({
                let extension_source_path = extension_source_path.clone();
                let fs = fs.clone();
                async move {
                    builder
                        .compile_extension(
                            &extension_source_path,
                            &mut extension_manifest,
                            CompileExtensionOptions::dev(),
                            fs,
                        )
                        .await
                }
            })
            .await
            .inspect_err(|error| {
                util::log_err(error);
            })?;

            let output_path = &extensions_dir.join(extension_id.as_ref());
            if let Some(metadata) = fs.metadata(output_path).await? {
                if metadata.is_symlink {
                    fs.remove_file(
                        output_path,
                        RemoveOptions {
                            recursive: false,
                            ignore_if_not_exists: true,
                        },
                    )
                    .await?;
                } else {
                    bail!("extension {extension_id} is still installed");
                }
            }

            fs.create_symlink(output_path, extension_source_path)
                .await?;

            this.update(cx, |this, cx| this.reload(None, cx))?.await;
            this.update(cx, |this, cx| {
                cx.emit(Event::ExtensionInstalled(extension_id.clone()));
                if let Some(events) = ExtensionEvents::try_global(cx)
                    && let Some(manifest) = this.extension_manifest_for_id(&extension_id)
                {
                    events.update(cx, |this, cx| {
                        this.emit(extension::Event::ExtensionInstalled(manifest.clone()), cx)
                    });
                }
            })?;

            Ok(())
        })
    }

    pub fn rebuild_dev_extension(&mut self, extension_id: Arc<str>, cx: &mut Context<Self>) {
        let path = self.installed_dir.join(extension_id.as_ref());
        let builder = self.builder.clone();
        let fs = self.fs.clone();

        match self.outstanding_operations.entry(extension_id.clone()) {
            btree_map::Entry::Occupied(_) => return,
            btree_map::Entry::Vacant(e) => e.insert(ExtensionOperation::Upgrade),
        };

        cx.notify();
        let compile = cx.background_spawn(async move {
            let mut manifest = ExtensionManifest::load(fs.clone(), &path).await?;
            builder
                .compile_extension(&path, &mut manifest, CompileExtensionOptions::dev(), fs)
                .await
        });

        cx.spawn(async move |this, cx| {
            let result = compile.await;

            this.update(cx, |this, cx| {
                this.outstanding_operations.remove(&extension_id);
                cx.notify();
            })?;

            if result.is_ok() {
                this.update(cx, |this, cx| this.reload(Some(extension_id), cx))?
                    .await;
            }

            result
        })
        .detach_and_log_err(cx)
    }
}
