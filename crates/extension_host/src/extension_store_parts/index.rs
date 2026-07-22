use super::super::*;

impl ExtensionStore {
    pub(super) fn rebuild_extension_index(&self, cx: &mut Context<Self>) -> Task<ExtensionIndex> {
        let fs = self.fs.clone();
        let work_dir = self.wasm_host.work_dir.clone();
        let extensions_dir = self.installed_dir.clone();
        let index_path = self.index_path.clone();
        let proxy = self.proxy.clone();
        cx.background_spawn(async move {
            let start_time = Instant::now();
            let mut index = ExtensionIndex::default();

            fs.create_dir(&work_dir).await.log_err();
            fs.create_dir(&extensions_dir).await.log_err();

            let extension_paths = fs.read_dir(&extensions_dir).await;
            if let Ok(mut extension_paths) = extension_paths {
                while let Some(extension_dir) = extension_paths.next().await {
                    let Ok(extension_dir) = extension_dir else {
                        continue;
                    };

                    if extension_dir
                        .file_name()
                        .is_some_and(|file_name| file_name == ".DS_Store")
                    {
                        continue;
                    }

                    Self::add_extension_to_index(
                        fs.clone(),
                        extension_dir,
                        &mut index,
                        proxy.clone(),
                    )
                    .await
                    .log_err();
                }
            }

            if let Ok(index_json) = serde_json::to_string_pretty(&index) {
                fs.save(&index_path, &index_json.as_str().into(), Default::default())
                    .await
                    .context("failed to save extension index")
                    .log_err();
            }

            log::info!("rebuilt extension index in {:?}", start_time.elapsed());
            index
        })
    }

    async fn add_extension_to_index(
        fs: Arc<dyn Fs>,
        extension_dir: PathBuf,
        index: &mut ExtensionIndex,
        proxy: Arc<ExtensionHostProxy>,
    ) -> Result<()> {
        let mut extension_manifest = ExtensionManifest::load(fs.clone(), &extension_dir).await?;
        let extension_id = extension_manifest.id.clone();

        // TODO: distinguish dev extensions more explicitly, by the absence
        // of a checksum file that we'll create when downloading normal extensions.
        let is_dev = fs
            .metadata(&extension_dir)
            .await?
            .with_context(|| format!("missing extension directory {extension_dir:?}"))?
            .is_symlink;

        let language_dir = extension_dir.join("languages");
        if let Ok(mut language_paths) = fs.read_dir(&language_dir).await {
            while let Some(language_path) = language_paths.next().await {
                let language_path = language_path
                    .with_context(|| format!("reading entries in language dir {language_dir:?}"))?;
                let Ok(relative_path) = language_path.strip_prefix(&extension_dir) else {
                    continue;
                };
                let Ok(Some(fs_metadata)) = fs.metadata(&language_path).await else {
                    continue;
                };
                if !fs_metadata.is_dir {
                    continue;
                }
                let language_config_path = language_path.join(LanguageConfig::FILE_NAME);
                let config = fs.load(&language_config_path).await.with_context(|| {
                    format!("loading language config from {language_config_path:?}")
                })?;
                let config = ::toml::from_str::<LanguageConfig>(&config)?;

                let relative_path = relative_path.to_rel_path_buf()?;
                if !extension_manifest.languages.contains(&relative_path) {
                    extension_manifest.languages.push(relative_path.clone());
                }

                index.languages.insert(
                    config.name.clone(),
                    ExtensionIndexLanguageEntry {
                        extension: extension_id.clone(),
                        path: relative_path.as_std_path().to_path_buf(),
                        matcher: config.matcher,
                        hidden: config.hidden,
                        grammar: config.grammar,
                    },
                );
            }
        }

        if let Ok(mut theme_paths) = fs.read_dir(&extension_dir.join("themes")).await {
            while let Some(theme_path) = theme_paths.next().await {
                let theme_path = theme_path?;
                let Ok(relative_path) = theme_path.strip_prefix(&extension_dir) else {
                    continue;
                };

                let Some(theme_families) = proxy
                    .list_theme_names(theme_path.clone(), fs.clone())
                    .await
                    .log_err()
                else {
                    continue;
                };

                let relative_path = relative_path.to_rel_path_buf()?;
                if !extension_manifest.themes.contains(&relative_path) {
                    extension_manifest.themes.push(relative_path.clone());
                }

                for theme_name in theme_families {
                    index.themes.insert(
                        theme_name.into(),
                        ExtensionIndexThemeEntry {
                            extension: extension_id.clone(),
                            path: relative_path.as_std_path().to_path_buf(),
                        },
                    );
                }
            }
        }

        if let Ok(mut icon_theme_paths) = fs.read_dir(&extension_dir.join("icon_themes")).await {
            while let Some(icon_theme_path) = icon_theme_paths.next().await {
                let icon_theme_path = icon_theme_path?;
                let Ok(relative_path) = icon_theme_path.strip_prefix(&extension_dir) else {
                    continue;
                };

                let Some(icon_theme_families) = proxy
                    .list_icon_theme_names(icon_theme_path.clone(), fs.clone())
                    .await
                    .log_err()
                else {
                    continue;
                };

                let relative_path = relative_path.to_rel_path_buf()?;
                if !extension_manifest.icon_themes.contains(&relative_path) {
                    extension_manifest.icon_themes.push(relative_path.clone());
                }

                for icon_theme_name in icon_theme_families {
                    index.icon_themes.insert(
                        icon_theme_name.into(),
                        ExtensionIndexIconThemeEntry {
                            extension: extension_id.clone(),
                            path: relative_path.as_std_path().to_path_buf(),
                        },
                    );
                }
            }
        }

        let extension_wasm_path = extension_dir.join("extension.wasm");
        if fs.is_file(&extension_wasm_path).await {
            extension_manifest
                .lib
                .kind
                .get_or_insert(ExtensionLibraryKind::Rust);
        }

        index.extensions.insert(
            extension_id.clone(),
            ExtensionIndexEntry {
                dev: is_dev,
                manifest: Arc::new(extension_manifest),
            },
        );

        Ok(())
    }
}
