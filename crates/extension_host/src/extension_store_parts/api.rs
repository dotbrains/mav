use super::super::*;

impl ExtensionStore {
    pub fn fetch_extensions(
        &self,
        search: Option<&str>,
        provides_filter: Option<&BTreeSet<ExtensionProvides>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<ExtensionMetadata>>> {
        let version = CURRENT_SCHEMA_VERSION.to_string();
        let mut query = vec![("max_schema_version", version.as_str())];
        if let Some(search) = search {
            query.push(("filter", search));
        }

        let provides_filter = provides_filter.map(|provides_filter| {
            provides_filter
                .iter()
                .map(|provides| provides.to_string())
                .collect::<Vec<_>>()
                .join(",")
        });
        if let Some(provides_filter) = provides_filter.as_deref() {
            query.push(("provides", provides_filter));
        }

        self.fetch_extensions_from_api("/extensions", &query, cx)
    }

    pub fn fetch_extensions_with_update_available(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<ExtensionMetadata>>> {
        let schema_versions = schema_version_range();
        let wasm_api_versions = wasm_api_version_range(ReleaseChannel::global(cx));
        let extension_settings = ExtensionSettings::get_global(cx);
        let extension_ids = self
            .extension_index
            .extensions
            .iter()
            .filter(|(id, entry)| !entry.dev && extension_settings.should_auto_update(id))
            .map(|(id, _)| id.as_ref())
            .collect::<Vec<_>>()
            .join(",");
        let task = self.fetch_extensions_from_api(
            "/extensions/updates",
            &[
                ("min_schema_version", &schema_versions.start().to_string()),
                ("max_schema_version", &schema_versions.end().to_string()),
                (
                    "min_wasm_api_version",
                    &wasm_api_versions.start().to_string(),
                ),
                ("max_wasm_api_version", &wasm_api_versions.end().to_string()),
                ("ids", &extension_ids),
            ],
            cx,
        );
        cx.spawn(async move |this, cx| {
            let extensions = task.await?;
            this.update(cx, |this, _cx| {
                extensions
                    .into_iter()
                    .filter(|extension| {
                        this.extension_index
                            .extensions
                            .get(&extension.id)
                            .is_none_or(|installed_extension| {
                                installed_extension.manifest.version != extension.manifest.version
                            })
                    })
                    .collect()
            })
        })
    }

    pub fn fetch_extension_versions(
        &self,
        extension_id: &str,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<ExtensionMetadata>>> {
        self.fetch_extensions_from_api(&format!("/extensions/{extension_id}"), &[], cx)
    }
}
