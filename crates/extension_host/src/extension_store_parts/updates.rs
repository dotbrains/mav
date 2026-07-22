use super::super::*;

impl ExtensionStore {
    /// Updates the set of installed extensions.
    ///
    /// First, this unloads any themes, languages, or grammars that are
    /// no longer in the manifest, or whose files have changed on disk.
    /// Then it loads any themes, languages, or grammars that are newly
    /// added to the manifest, or whose files have changed on disk.
    #[ztracing::instrument(skip_all)]
    pub(super) fn extensions_updated(
        &mut self,
        mut new_index: ExtensionIndex,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let old_index = &self.extension_index;

        let suppressed_extensions_to_remove = new_index
            .extensions
            .extract_if(.., |extension_id, _| {
                SUPPRESSED_EXTENSIONS.contains(extension_id.as_ref())
            })
            .collect::<Vec<_>>();

        // Determine which extensions need to be loaded and unloaded, based
        // on the changes to the manifest and the extensions that we know have been
        // modified.
        let mut extensions_to_unload = Vec::default();
        let mut extensions_to_load = Vec::default();
        {
            let mut old_keys = old_index.extensions.iter().peekable();
            let mut new_keys = new_index.extensions.iter().peekable();
            loop {
                match (old_keys.peek(), new_keys.peek()) {
                    (None, None) => break,
                    (None, Some(_)) => {
                        extensions_to_load.push(new_keys.next().unwrap().0.clone());
                    }
                    (Some(_), None) => {
                        extensions_to_unload.push(old_keys.next().unwrap().0.clone());
                    }
                    (Some((old_key, _)), Some((new_key, _))) => match old_key.cmp(new_key) {
                        Ordering::Equal => {
                            let (old_key, old_value) = old_keys.next().unwrap();
                            let (new_key, new_value) = new_keys.next().unwrap();
                            if old_value != new_value || self.modified_extensions.contains(old_key)
                            {
                                extensions_to_unload.push(old_key.clone());
                                extensions_to_load.push(new_key.clone());
                            }
                        }
                        Ordering::Less => {
                            extensions_to_unload.push(old_keys.next().unwrap().0.clone());
                        }
                        Ordering::Greater => {
                            extensions_to_load.push(new_keys.next().unwrap().0.clone());
                        }
                    },
                }
            }
            self.modified_extensions.clear();
        }

        let trigger_suppressed_extension_removal =
            move |this: &mut ExtensionStore, cx: &mut Context<ExtensionStore>| {
                for (id, _) in suppressed_extensions_to_remove {
                    this.uninstall_extension(id, cx).detach_and_log_err(cx);
                }
            };

        if extensions_to_load.is_empty() && extensions_to_unload.is_empty() {
            self.reload_complete_senders.clear();
            trigger_suppressed_extension_removal(self, cx);
            return Task::ready(());
        }

        let reload_count = extensions_to_unload
            .iter()
            .filter(|id| extensions_to_load.contains(id))
            .count();

        log::info!(
            "extensions updated. loading {}, reloading {}, unloading {}",
            extensions_to_load.len() - reload_count,
            reload_count,
            extensions_to_unload.len() - reload_count
        );

        let extension_ids = extensions_to_load
            .iter()
            .filter_map(|id| {
                Some((
                    id.clone(),
                    new_index.extensions.get(id)?.manifest.version.clone(),
                ))
            })
            .collect::<Vec<_>>();

        telemetry::event!("Extensions Loaded", id_and_versions = extension_ids);

        let themes_to_remove = old_index
            .themes
            .iter()
            .filter_map(|(name, entry)| {
                if extensions_to_unload.contains(&entry.extension) {
                    Some(name.clone().into())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let icon_themes_to_remove = old_index
            .icon_themes
            .iter()
            .filter_map(|(name, entry)| {
                if extensions_to_unload.contains(&entry.extension) {
                    Some(name.clone().into())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let languages_to_remove = old_index
            .languages
            .iter()
            .filter_map(|(name, entry)| {
                if extensions_to_unload.contains(&entry.extension) {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let mut grammars_to_remove = Vec::new();
        let mut server_removal_tasks = Vec::with_capacity(extensions_to_unload.len());
        for extension_id in &extensions_to_unload {
            let Some(extension) = old_index.extensions.get(extension_id) else {
                continue;
            };
            grammars_to_remove.extend(extension.manifest.grammars.keys().cloned());
            for (language_server_name, config) in &extension.manifest.language_servers {
                for language in config.languages() {
                    server_removal_tasks.push(self.proxy.remove_language_server(
                        &language,
                        language_server_name,
                        cx,
                    ));
                }
            }

            for server_id in extension.manifest.context_servers.keys() {
                self.proxy.unregister_context_server(server_id.clone(), cx);
            }
            for adapter in extension.manifest.debug_adapters.keys() {
                self.proxy.unregister_debug_adapter(adapter.clone());
            }
            for locator in extension.manifest.debug_locators.keys() {
                self.proxy.unregister_debug_locator(locator.clone());
            }
        }

        self.wasm_extensions
            .retain(|(extension, _)| !extensions_to_unload.contains(&extension.id));
        self.proxy.remove_user_themes(themes_to_remove);
        self.proxy.remove_icon_themes(icon_themes_to_remove);
        self.proxy
            .remove_languages(&languages_to_remove, &grammars_to_remove);

        // Remove semantic token rules for languages being unloaded.
        if !languages_to_remove.is_empty() {
            SettingsStore::update_global(cx, |store, cx| {
                for language in &languages_to_remove {
                    store.remove_language_semantic_token_rules(language.as_ref(), cx);
                }
            });
        }

        let mut grammars_to_add = Vec::new();
        let mut themes_to_add = Vec::new();
        let mut icon_themes_to_add = Vec::new();
        let mut snippets_to_add = Vec::new();
        for extension_id in &extensions_to_load {
            let Some(extension) = new_index.extensions.get(extension_id) else {
                continue;
            };

            grammars_to_add.extend(extension.manifest.grammars.keys().map(|grammar_name| {
                let mut grammar_path = self.installed_dir.clone();
                grammar_path.extend([extension_id.as_ref(), "grammars"]);
                grammar_path.push(grammar_name.as_ref());
                grammar_path.set_extension("wasm");
                (grammar_name.clone(), grammar_path)
            }));
            themes_to_add.extend(extension.manifest.themes.iter().map(|theme_path| {
                let mut path = self.installed_dir.clone();
                path.extend([Path::new(extension_id.as_ref()), theme_path.as_std_path()]);
                path
            }));
            icon_themes_to_add.extend(extension.manifest.icon_themes.iter().map(
                |icon_theme_path| {
                    let mut path = self.installed_dir.clone();
                    path.extend([
                        Path::new(extension_id.as_ref()),
                        icon_theme_path.as_std_path(),
                    ]);

                    let mut icons_root_path = self.installed_dir.clone();
                    icons_root_path.extend([Path::new(extension_id.as_ref())]);

                    (path, icons_root_path)
                },
            ));
            snippets_to_add.extend(extension.manifest.snippets.iter().flat_map(|snippets| {
                snippets.paths().map(|snippets_path| {
                    let mut path = self.installed_dir.clone();
                    path.extend([Path::new(extension_id.as_ref()), snippets_path.as_path()]);
                    path
                })
            }));
        }

        self.proxy.register_grammars(grammars_to_add);
        let languages_to_add = new_index
            .languages
            .iter()
            .filter(|(_, entry)| extensions_to_load.contains(&entry.extension))
            .collect::<Vec<_>>();
        let mut semantic_token_rules_to_add: Vec<(LanguageName, SemanticTokenRules)> = Vec::new();
        for (language_name, language) in languages_to_add {
            let mut language_path = self.installed_dir.clone();
            language_path.extend([
                Path::new(language.extension.as_ref()),
                language.path.as_path(),
            ]);

            // Load semantic token rules if present in the language directory.
            let rules_path = language_path.join(SemanticTokenRules::FILE_NAME);
            if std::fs::exists(&rules_path).is_ok_and(|exists| exists)
                && let Some(rules) = SemanticTokenRules::load(&rules_path).log_err()
            {
                semantic_token_rules_to_add.push((language_name.clone(), rules));
            }

            self.proxy.register_language(
                language_name.clone(),
                language.grammar.clone(),
                language.matcher.clone(),
                language.hidden,
                Arc::new(move || {
                    let config =
                        LanguageConfig::load(language_path.join(LanguageConfig::FILE_NAME))?;
                    let queries = load_plugin_queries(&language_path);
                    let context_provider =
                        std::fs::read_to_string(language_path.join(TaskTemplates::FILE_NAME))
                            .ok()
                            .and_then(|contents| {
                                let definitions =
                                    serde_json_lenient::from_str(&contents).log_err()?;
                                Some(Arc::new(ContextProviderWithTasks::new(definitions)) as Arc<_>)
                            });

                    Ok(LoadedLanguage {
                        config,
                        queries,
                        context_provider,
                        toolchain_provider: None,
                        manifest_name: None,
                    })
                }),
            );
        }

        // Register semantic token rules for newly loaded extension languages.
        if !semantic_token_rules_to_add.is_empty() {
            SettingsStore::update_global(cx, |store, cx| {
                for (language_name, rules) in semantic_token_rules_to_add {
                    store.set_language_semantic_token_rules(language_name.0.clone(), rules, cx);
                }
            });
        }

        let fs = self.fs.clone();
        let wasm_host = self.wasm_host.clone();
        let root_dir = self.installed_dir.clone();
        let proxy = self.proxy.clone();
        let extension_entries = extensions_to_load
            .iter()
            .filter_map(|name| new_index.extensions.get(name).cloned())
            .collect::<Vec<_>>();
        self.extension_index = new_index;
        cx.notify();
        cx.emit(Event::ExtensionsUpdated);

        cx.spawn(async move |this, cx| {
            cx.background_spawn({
                let fs = fs.clone();
                async move {
                    let _ = join_all(server_removal_tasks).await;
                    for theme_path in themes_to_add {
                        proxy
                            .load_user_theme(theme_path, fs.clone())
                            .await
                            .log_err();
                    }

                    for (icon_theme_path, icons_root_path) in icon_themes_to_add {
                        proxy
                            .load_icon_theme(icon_theme_path, icons_root_path, fs.clone())
                            .await
                            .log_err();
                    }

                    for snippets_path in &snippets_to_add {
                        match fs
                            .load(snippets_path)
                            .await
                            .with_context(|| format!("Loading snippets from {snippets_path:?}"))
                        {
                            Ok(snippets_contents) => {
                                proxy
                                    .register_snippet(snippets_path, &snippets_contents)
                                    .log_err();
                            }
                            Err(e) => log::error!("Cannot load snippets: {e:#}"),
                        }
                    }
                }
            })
            .await;

            let mut wasm_extensions = Vec::new();
            for extension in extension_entries {
                if extension.manifest.lib.kind.is_none() {
                    continue;
                };

                let extension_path = root_dir.join(extension.manifest.id.as_ref());
                let wasm_extension = WasmExtension::load(
                    &extension_path,
                    &extension.manifest,
                    wasm_host.clone(),
                    cx,
                )
                .await
                .with_context(|| format!("Loading extension from {extension_path:?}"));

                match wasm_extension {
                    Ok(wasm_extension) => {
                        wasm_extensions.push((extension.manifest.clone(), wasm_extension))
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to load extension: {}, {:#}",
                            extension.manifest.id,
                            e
                        );
                        this.update(cx, |_, cx| {
                            cx.emit(Event::ExtensionFailedToLoad(extension.manifest.id.clone()))
                        })
                        .ok();
                    }
                }
            }

            this.update(cx, |this, cx| {
                this.reload_complete_senders.clear();

                for (manifest, wasm_extension) in &wasm_extensions {
                    let extension = Arc::new(wasm_extension.clone());

                    for (language_server_id, language_server_config) in &manifest.language_servers {
                        for language in language_server_config.languages() {
                            this.proxy.register_language_server(
                                extension.clone(),
                                language_server_id.clone(),
                                language.clone(),
                            );
                        }
                    }

                    for id in manifest.context_servers.keys() {
                        this.proxy
                            .register_context_server(extension.clone(), id.clone(), cx);
                    }

                    for (debug_adapter, meta) in &manifest.debug_adapters {
                        let mut path = root_dir.clone();
                        path.push(Path::new(manifest.id.as_ref()));
                        if let Some(schema_path) = &meta.schema_path {
                            path.push(schema_path);
                        } else {
                            path.push("debug_adapter_schemas");
                            path.push(Path::new(debug_adapter.as_ref()).with_extension("json"));
                        }

                        this.proxy.register_debug_adapter(
                            extension.clone(),
                            debug_adapter.clone(),
                            &path,
                        );
                    }

                    for debug_adapter in manifest.debug_locators.keys() {
                        this.proxy
                            .register_debug_locator(extension.clone(), debug_adapter.clone());
                    }
                }

                this.wasm_extensions.extend(wasm_extensions);
                this.proxy.set_extensions_loaded();
                this.proxy.reload_current_theme(cx);
                this.proxy.reload_current_icon_theme(cx);
                trigger_suppressed_extension_removal(this, cx);

                if let Some(events) = ExtensionEvents::try_global(cx) {
                    events.update(cx, |this, cx| {
                        this.emit(extension::Event::ExtensionsInstalledChanged, cx)
                    });
                }
            })
            .ok();
        })
    }
}

fn load_plugin_queries(root_path: &Path) -> LanguageQueries {
    let mut result = LanguageQueries::default();
    if let Some(entries) = std::fs::read_dir(root_path).log_err() {
        for entry in entries {
            let Some(entry) = entry.log_err() else {
                continue;
            };
            let path = entry.path();
            if let Some(remainder) = path.strip_prefix(root_path).ok().and_then(|p| p.to_str()) {
                if !remainder.ends_with(".scm") {
                    continue;
                }
                for (name, query) in QUERY_FILENAME_PREFIXES {
                    if remainder.starts_with(name) {
                        if let Some(contents) = std::fs::read_to_string(&path).log_err() {
                            match query(&mut result) {
                                None => *query(&mut result) = Some(contents.into()),
                                Some(r) => r.to_mut().push_str(contents.as_ref()),
                            }
                        }
                        break;
                    }
                }
            }
        }
    }
    result
}
