use super::super::*;

impl ExtensionStore {
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalExtensionStore>()
            .map(|store| store.0.clone())
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalExtensionStore>().0.clone()
    }

    pub fn new(
        extensions_dir: PathBuf,
        build_dir: Option<PathBuf>,
        extension_host_proxy: Arc<ExtensionHostProxy>,
        fs: Arc<dyn Fs>,
        http_client: Arc<HttpClientWithUrl>,
        builder_client: Arc<dyn HttpClient>,
        telemetry: Option<Arc<Telemetry>>,
        node_runtime: NodeRuntime,
        cx: &mut Context<Self>,
    ) -> Self {
        let work_dir = extensions_dir.join("work");
        let build_dir = build_dir.unwrap_or_else(|| extensions_dir.join("build"));
        let installed_dir = extensions_dir.join("installed");
        let staging_dir = extensions_dir.join("staging");
        let index_path = extensions_dir.join("index.json");

        let (reload_tx, mut reload_rx) = unbounded();
        let (connection_registered_tx, mut connection_registered_rx) = unbounded();
        let mut this = Self {
            proxy: extension_host_proxy.clone(),
            extension_index: Default::default(),
            installed_dir,
            staging_dir,
            index_path,
            builder: Arc::new(ExtensionBuilder::new(builder_client, build_dir)),
            outstanding_operations: Default::default(),
            modified_extensions: Default::default(),
            reload_complete_senders: Vec::new(),
            wasm_host: WasmHost::new(
                fs.clone(),
                http_client.clone(),
                node_runtime,
                extension_host_proxy,
                work_dir,
                cx,
            ),
            wasm_extensions: Vec::new(),
            fs,
            http_client,
            telemetry,
            reload_tx,
            tasks: Vec::new(),

            remote_clients: Default::default(),
            ssh_registered_tx: connection_registered_tx,
        };

        // The extensions store maintains an index file, which contains a complete
        // list of the installed extensions and the resources that they provide.
        // This index is loaded synchronously on startup.
        let (index_content, index_metadata, extensions_metadata) =
            cx.foreground_executor().block_on(async {
                futures::join!(
                    this.fs.load(&this.index_path),
                    this.fs.metadata(&this.index_path),
                    this.fs.metadata(&this.installed_dir),
                )
            });

        // Normally, there is no need to rebuild the index. But if the index file
        // is invalid or is out-of-date according to the filesystem mtimes, then
        // it must be asynchronously rebuilt.
        let mut extension_index = ExtensionIndex::default();
        let mut extension_index_needs_rebuild = true;
        if let Ok(index_content) = index_content
            && let Some(index) = serde_json::from_str(&index_content).log_err()
        {
            extension_index = index;
            if let (Ok(Some(index_metadata)), Ok(Some(extensions_metadata))) =
                (index_metadata, extensions_metadata)
                && index_metadata
                    .mtime
                    .bad_is_greater_than(extensions_metadata.mtime)
            {
                extension_index_needs_rebuild = false;
            }
        }

        // Immediately load all of the extensions in the initial manifest. If the
        // index needs to be rebuild, then enqueue
        let load_initial_extensions = this.extensions_updated(extension_index, cx);
        let mut reload_future = None;
        if extension_index_needs_rebuild {
            reload_future = Some(this.reload(None, cx));
        }

        cx.spawn(async move |this, cx| {
            if let Some(future) = reload_future {
                future.await;
            }
            this.update(cx, |this, cx| this.auto_install_extensions(cx))
                .ok();
            this.update(cx, |this, cx| this.check_for_updates(cx)).ok();
        })
        .detach();

        // Perform all extension loading in a single task to ensure that we
        // never attempt to simultaneously load/unload extensions from multiple
        // parallel tasks.
        this.tasks.push(cx.spawn(async move |this, cx| {
            async move {
                load_initial_extensions.await;

                let mut index_changed = false;
                let mut debounce_timer = cx.background_spawn(futures::future::pending()).fuse();

                loop {
                    select_biased! {
                        _ = debounce_timer => {
                            if index_changed {
                                let index = this
                                    .update(cx, |this, cx| this.rebuild_extension_index(cx))?
                                    .await;
                                this.update(cx, |this, cx| this.extensions_updated(index, cx))?
                                    .await;
                                index_changed = false;
                            }

                            Self::update_remote_clients(&this, cx).await?;
                        }
                        _ = connection_registered_rx.next() => {
                            debounce_timer = cx.background_executor().timer(RELOAD_DEBOUNCE_DURATION).fuse()
                        }
                        extension_id = reload_rx.next() => {
                            let Some(extension_id) = extension_id else { break; };
                            this.update(cx, |this, _cx| {
                                this.modified_extensions.extend(extension_id);
                            })?;
                            index_changed = true;
                            debounce_timer = cx.background_executor().timer(RELOAD_DEBOUNCE_DURATION).fuse()
                        }
                    }
                }

                anyhow::Ok(())
            }
            .map(drop)
            .await;
        }));

        // Watch the installed extensions directory for changes. Whenever changes are
        // detected, rebuild the extension index, and load/unload any extensions that
        // have been added, removed, or modified.
        this.tasks.push(cx.background_spawn({
            let fs = this.fs.clone();
            let reload_tx = this.reload_tx.clone();
            let installed_dir = this.installed_dir.clone();
            async move {
                let (mut paths, _) = fs.watch(&installed_dir, FS_WATCH_LATENCY).await;
                while let Some(events) = paths.next().await {
                    for event in events {
                        let Ok(event_path) = event.path.strip_prefix(&installed_dir) else {
                            continue;
                        };

                        if let Some(path::Component::Normal(extension_dir_name)) =
                            event_path.components().next()
                            && let Some(extension_id) = extension_dir_name.to_str()
                        {
                            reload_tx.unbounded_send(Some(extension_id.into())).ok();
                        }
                    }
                }
            }
        }));

        this
    }

    pub fn reload(
        &mut self,
        modified_extension: Option<Arc<str>>,
        cx: &mut Context<Self>,
    ) -> impl Future<Output = ()> + use<> {
        let (tx, rx) = oneshot::channel();
        self.reload_complete_senders.push(tx);
        self.reload_tx
            .unbounded_send(modified_extension)
            .expect("reload task exited");
        cx.emit(Event::StartedReloading);

        async move {
            rx.await.ok();
        }
    }

    pub(super) fn extensions_dir(&self) -> PathBuf {
        self.installed_dir.clone()
    }

    pub fn outstanding_operations(&self) -> &BTreeMap<Arc<str>, ExtensionOperation> {
        &self.outstanding_operations
    }

    pub fn installed_extensions(&self) -> &BTreeMap<Arc<str>, ExtensionIndexEntry> {
        &self.extension_index.extensions
    }

    pub fn dev_extensions(&self) -> impl Iterator<Item = &Arc<ExtensionManifest>> {
        self.extension_index
            .extensions
            .values()
            .filter_map(|extension| extension.dev.then_some(&extension.manifest))
    }

    pub fn extension_manifest_for_id(&self, extension_id: &str) -> Option<&Arc<ExtensionManifest>> {
        self.extension_index
            .extensions
            .get(extension_id)
            .map(|extension| &extension.manifest)
    }

    /// Returns the names of themes provided by extensions.
    pub fn extension_themes<'a>(
        &'a self,
        extension_id: &'a str,
    ) -> impl Iterator<Item = &'a Arc<str>> {
        self.extension_index
            .themes
            .iter()
            .filter_map(|(name, theme)| theme.extension.as_ref().eq(extension_id).then_some(name))
    }

    /// Returns the path to the theme file within an extension, if there is an
    /// extension that provides the theme.
    pub fn path_to_extension_theme(&self, theme_name: &str) -> Option<PathBuf> {
        let entry = self.extension_index.themes.get(theme_name)?;

        Some(
            self.extensions_dir()
                .join(entry.extension.as_ref())
                .join(&entry.path),
        )
    }

    /// Returns the names of icon themes provided by extensions.
    pub fn extension_icon_themes<'a>(
        &'a self,
        extension_id: &'a str,
    ) -> impl Iterator<Item = &'a Arc<str>> {
        self.extension_index
            .icon_themes
            .iter()
            .filter_map(|(name, icon_theme)| {
                icon_theme
                    .extension
                    .as_ref()
                    .eq(extension_id)
                    .then_some(name)
            })
    }

    /// Returns the path to the icon theme file within an extension, if there is
    /// an extension that provides the icon theme.
    pub fn path_to_extension_icon_theme(
        &self,
        icon_theme_name: &str,
    ) -> Option<(PathBuf, PathBuf)> {
        let entry = self.extension_index.icon_themes.get(icon_theme_name)?;

        let icon_theme_path = self
            .extensions_dir()
            .join(entry.extension.as_ref())
            .join(&entry.path);
        let icons_root_path = self.extensions_dir().join(entry.extension.as_ref());

        Some((icon_theme_path, icons_root_path))
    }
}
