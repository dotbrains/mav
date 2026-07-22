use super::*;

impl ExtensionsPage {
    pub(super) fn on_extension_installed(
        &mut self,
        workspace: WeakEntity<Workspace>,
        extension_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let extension_store = ExtensionStore::global(cx).read(cx);
        let themes = extension_store
            .extension_themes(extension_id)
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        if !themes.is_empty() {
            workspace
                .update(cx, |_workspace, cx| {
                    window.dispatch_action(
                        mav_actions::theme_selector::Toggle {
                            themes_filter: Some(themes),
                        }
                        .boxed_clone(),
                        cx,
                    );
                })
                .ok();
            return;
        }

        let icon_themes = extension_store
            .extension_icon_themes(extension_id)
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        if !icon_themes.is_empty() {
            workspace
                .update(cx, |_workspace, cx| {
                    window.dispatch_action(
                        mav_actions::icon_theme_selector::Toggle {
                            themes_filter: Some(icon_themes),
                        }
                        .boxed_clone(),
                        cx,
                    );
                })
                .ok();
        }
    }

    /// Returns whether a dev extension currently exists for the extension with the given ID.
    pub(super) fn dev_extension_exists(extension_id: &str, cx: &mut Context<Self>) -> bool {
        let extension_store = ExtensionStore::global(cx).read(cx);

        extension_store
            .dev_extensions()
            .any(|dev_extension| dev_extension.id.as_ref() == extension_id)
    }

    pub(super) fn extension_status(extension_id: &str, cx: &mut Context<Self>) -> ExtensionStatus {
        let extension_store = ExtensionStore::global(cx).read(cx);

        match extension_store.outstanding_operations().get(extension_id) {
            Some(ExtensionOperation::Install) => ExtensionStatus::Installing,
            Some(ExtensionOperation::Remove) => ExtensionStatus::Removing,
            Some(ExtensionOperation::Upgrade) => ExtensionStatus::Upgrading,
            None => match extension_store.installed_extensions().get(extension_id) {
                Some(extension) => ExtensionStatus::Installed(extension.manifest.version.clone()),
                None => ExtensionStatus::NotInstalled,
            },
        }
    }

    pub(super) fn filter_extension_entries(&mut self, cx: &mut Context<Self>) {
        self.filtered_remote_extension_indices.clear();
        self.filtered_remote_extension_indices.extend(
            self.remote_extension_entries
                .iter()
                .enumerate()
                .filter(|(_, extension)| match self.filter {
                    ExtensionFilter::All => true,
                    ExtensionFilter::Installed => {
                        let status = Self::extension_status(&extension.id, cx);
                        matches!(status, ExtensionStatus::Installed(_))
                    }
                    ExtensionFilter::NotInstalled => {
                        let status = Self::extension_status(&extension.id, cx);

                        matches!(status, ExtensionStatus::NotInstalled)
                    }
                })
                .filter(|(_, extension)| match self.provides_filter {
                    Some(provides) => extension.manifest.provides.contains(&provides),
                    None => true,
                })
                .map(|(ix, _)| ix),
        );

        self.filtered_dev_extension_indices.clear();
        self.filtered_dev_extension_indices.extend(
            self.dev_extension_entries
                .iter()
                .enumerate()
                .filter(|(_, manifest)| match self.provides_filter {
                    Some(provides) => manifest.provides().contains(&provides),
                    None => true,
                })
                .map(|(ix, _)| ix),
        );

        cx.notify();
    }

    pub(super) fn scroll_to_top(&mut self, cx: &mut Context<Self>) {
        self.list.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    pub(super) fn fetch_extensions(
        &mut self,
        search: Option<String>,
        provides_filter: Option<BTreeSet<ExtensionProvides>>,
        on_complete: Option<Box<dyn FnOnce(&mut Self, &mut Context<Self>) + Send>>,
        cx: &mut Context<Self>,
    ) {
        self.is_fetching_extensions = true;
        self.fetch_failed = false;
        cx.notify();

        let extension_store = ExtensionStore::global(cx);

        let dev_extensions = extension_store
            .read(cx)
            .dev_extensions()
            .cloned()
            .collect::<Vec<_>>();

        let remote_extensions =
            if let Some(id) = search.as_ref().and_then(|s| s.strip_prefix("id:")) {
                let versions =
                    extension_store.update(cx, |store, cx| store.fetch_extension_versions(id, cx));
                cx.foreground_executor().spawn(async move {
                    let versions = versions.await?;
                    let latest = versions
                        .into_iter()
                        .max_by_key(|v| v.published_at)
                        .context("no extension found")?;
                    Ok(vec![latest])
                })
            } else {
                extension_store.update(cx, |store, cx| {
                    store.fetch_extensions(search.as_deref(), provides_filter.as_ref(), cx)
                })
            };

        cx.spawn(async move |this, cx| {
            let dev_extensions = if let Some(search) = search {
                let match_candidates = dev_extensions
                    .iter()
                    .enumerate()
                    .map(|(ix, manifest)| StringMatchCandidate::new(ix, &manifest.name))
                    .collect::<Vec<_>>();

                let matches = match_strings(
                    &match_candidates,
                    &search,
                    false,
                    true,
                    match_candidates.len(),
                    &Default::default(),
                    cx.background_executor().clone(),
                )
                .await;
                matches
                    .into_iter()
                    .map(|mat| dev_extensions[mat.candidate_id].clone())
                    .collect()
            } else {
                dev_extensions
            };

            let fetch_result = remote_extensions.await;

            let result = this.update(cx, |this, cx| {
                cx.notify();
                this.dev_extension_entries = dev_extensions;
                this.is_fetching_extensions = false;

                match fetch_result {
                    Ok(extensions) => {
                        this.fetch_failed = false;
                        this.remote_extension_entries = extensions;
                        this.filter_extension_entries(cx);
                        if let Some(callback) = on_complete {
                            callback(this, cx);
                        }
                        Ok(())
                    }
                    Err(err) => {
                        this.fetch_failed = true;
                        this.filter_extension_entries(cx);
                        Err(err)
                    }
                }
            });

            result?
        })
        .detach_and_log_err(cx);
    }
}
