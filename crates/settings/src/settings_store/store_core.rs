use super::*;

impl SettingsStore {
    pub fn new(cx: &mut App, default_settings: &str) -> Self {
        Self::new_with_semantic_tokens(cx, default_settings)
    }

    pub fn new_with_semantic_tokens(cx: &mut App, default_settings: &str) -> Self {
        let default_settings = Self::parse_default_settings(default_settings).unwrap();
        Self::from_settings_content(cx, default_settings)
    }

    fn from_settings_content(cx: &mut App, default_settings: SettingsContent) -> Self {
        let (setting_file_updates_tx, mut setting_file_updates_rx) = mpsc::unbounded();
        if !cx.has_global::<DefaultSemanticTokenRules>() {
            cx.set_global::<DefaultSemanticTokenRules>(
                crate::parse_json_with_comments::<SemanticTokenRules>(
                    &crate::default_semantic_token_rules(),
                )
                .map(DefaultSemanticTokenRules)
                .unwrap_or_default(),
            );
        }
        let default_settings: Rc<SettingsContent> = default_settings.into();
        let mut this = Self {
            setting_values: Default::default(),
            default_settings: default_settings.clone(),
            global_settings: None,
            server_settings: None,
            user_settings: None,
            extension_settings: None,
            language_semantic_token_rules: HashMap::default(),

            merged_settings: default_settings,
            last_user_settings_content: None,
            last_global_settings_content: None,
            local_settings: BTreeMap::default(),
            editorconfig_store: cx.new(|_| EditorconfigStore::default()),
            _settings_files_watcher: None,
            setting_file_updates_tx,
            _setting_file_updates: cx.spawn(async move |cx| {
                while let Some(setting_file_update) = setting_file_updates_rx.next().await {
                    (setting_file_update)(cx.clone()).await.log_err();
                }
            }),
            file_errors: BTreeMap::default(),
        };

        this.load_settings_types();

        this
    }

    pub fn observe_active_settings_profile_name(cx: &mut App) -> gpui::Subscription {
        cx.observe_global::<ActiveSettingsProfileName>(|cx| {
            Self::update_global(cx, |store, cx| {
                store.recompute_values(None, cx);
            });
        })
    }

    pub fn update<C, R>(cx: &mut C, f: impl FnOnce(&mut Self, &mut C) -> R) -> R
    where
        C: BorrowAppContext,
    {
        cx.update_global(f)
    }

    pub fn watch_settings_files(
        &mut self,
        fs: Arc<dyn Fs>,
        cx: &mut App,
        settings_changed: impl 'static + Fn(SettingsFile, SettingsParseResult, &mut App),
    ) {
        let (mut user_settings_file_rx, user_settings_watcher) = crate::watch_config_file(
            cx.background_executor(),
            fs.clone(),
            paths::settings_file().clone(),
        );
        let (mut global_settings_file_rx, global_settings_watcher) = crate::watch_config_file(
            cx.background_executor(),
            fs,
            paths::global_settings_file().clone(),
        );

        let global_content = cx
            .foreground_executor()
            .block_on(global_settings_file_rx.next())
            .unwrap();
        let user_content = cx
            .foreground_executor()
            .block_on(user_settings_file_rx.next())
            .unwrap();

        let result = self.set_user_settings(&user_content, cx);
        settings_changed(SettingsFile::User, result, cx);
        let result = self.set_global_settings(&global_content, cx);
        settings_changed(SettingsFile::Global, result, cx);

        self._settings_files_watcher = Some(cx.spawn(async move |cx| {
            let _user_settings_watcher = user_settings_watcher;
            let _global_settings_watcher = global_settings_watcher;
            let mut settings_streams = futures::stream::select(
                global_settings_file_rx.map(|content| (SettingsFile::Global, content)),
                user_settings_file_rx.map(|content| (SettingsFile::User, content)),
            );

            while let Some((settings_file, content)) = settings_streams.next().await {
                cx.update_global(|store: &mut SettingsStore, cx| {
                    let result = match settings_file {
                        SettingsFile::User => store.set_user_settings(&content, cx),
                        SettingsFile::Global => store.set_global_settings(&content, cx),
                        _ => return,
                    };
                    settings_changed(settings_file, result, cx);
                    cx.refresh_windows();
                });
            }
        }));
    }

    /// Add a new type of setting to the store.
    pub fn register_setting<T: Settings>(&mut self) {
        self.register_setting_internal(&RegisteredSetting {
            settings_value: || {
                Box::new(SettingValue::<T> {
                    global_value: None,
                    local_values: Vec::new(),
                })
            },
            from_settings: |content| Box::new(T::from_settings(content)),
            id: || TypeId::of::<T>(),
        });
    }

    fn load_settings_types(&mut self) {
        for registered_setting in inventory::iter::<RegisteredSetting>() {
            self.register_setting_internal(registered_setting);
        }
    }

    fn register_setting_internal(&mut self, registered_setting: &RegisteredSetting) {
        let entry = self.setting_values.entry((registered_setting.id)());

        if matches!(entry, hash_map::Entry::Occupied(_)) {
            return;
        }

        let setting_value = entry.or_insert((registered_setting.settings_value)());
        let value = (registered_setting.from_settings)(&self.merged_settings);
        setting_value.set_global_value(value);
    }

    pub fn merged_settings(&self) -> &SettingsContent {
        &self.merged_settings
    }

    /// Get the value of a setting.
    ///
    /// Panics if the given setting type has not been registered, or if there is no
    /// value for this setting.
    pub fn get<T: Settings>(&self, path: Option<SettingsLocation>) -> &T {
        self.setting_values
            .get(&TypeId::of::<T>())
            .unwrap_or_else(|| panic!("unregistered setting type {}", type_name::<T>()))
            .value_for_path(path)
            .downcast_ref::<T>()
            .expect("no default value for setting type")
    }

    /// Get the value of a setting.
    ///
    /// Does not panic
    pub fn try_get<T: Settings>(&self, path: Option<SettingsLocation>) -> Option<&T> {
        self.setting_values
            .get(&TypeId::of::<T>())
            .map(|value| value.value_for_path(path))
            .and_then(|value| value.downcast_ref::<T>())
    }

    /// Get all values from project specific settings
    pub fn get_all_locals<T: Settings>(&self) -> Vec<(WorktreeId, Arc<RelPath>, &T)> {
        self.setting_values
            .get(&TypeId::of::<T>())
            .unwrap_or_else(|| panic!("unregistered setting type {}", type_name::<T>()))
            .all_local_values()
            .into_iter()
            .map(|(id, path, any)| {
                (
                    id,
                    path,
                    any.downcast_ref::<T>()
                        .expect("wrong value type for setting"),
                )
            })
            .collect()
    }

    /// Override the global value for a setting.
    ///
    /// The given value will be overwritten if the user settings file changes.
    pub fn override_global<T: Settings>(&mut self, value: T) {
        self.setting_values
            .get_mut(&TypeId::of::<T>())
            .unwrap_or_else(|| panic!("unregistered setting type {}", type_name::<T>()))
            .set_global_value(Box::new(value))
    }

    /// Get the user's settings content.
    ///
    /// For user-facing functionality use the typed setting interface.
    /// (e.g. ProjectSettings::get_global(cx))
    pub fn raw_user_settings(&self) -> Option<&UserSettingsContent> {
        self.user_settings.as_ref()
    }

    /// Get the default settings content as a raw JSON value.
    pub fn raw_default_settings(&self) -> &SettingsContent {
        &self.default_settings
    }

    /// Get the configured settings profile names.
    pub fn configured_settings_profiles(&self) -> impl Iterator<Item = &str> {
        self.user_settings
            .iter()
            .flat_map(|settings| settings.profiles.keys().map(|k| k.as_str()))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test(cx: &mut App) -> Self {
        static CACHED_SETTINGS_CONTENT: std::sync::LazyLock<SettingsContent> =
            std::sync::LazyLock::new(|| {
                SettingsContent::parse_json_with_comments(crate::test_settings()).unwrap()
            });
        Self::from_settings_content(cx, CACHED_SETTINGS_CONTENT.clone())
    }

    /// Updates the value of a setting in the user's global configuration.
    ///
    /// This is only for tests. Normally, settings are only loaded from
    /// JSON files.
    #[cfg(any(test, feature = "test-support"))]
    pub fn update_user_settings(
        &mut self,
        cx: &mut App,
        update: impl FnOnce(&mut SettingsContent),
    ) {
        let mut content = self.user_settings.clone().unwrap_or_default().content;
        update(&mut content);
        fn trail(this: &mut SettingsStore, content: Box<SettingsContent>, cx: &mut App) {
            let new_text = serde_json::to_string(&UserSettingsContent {
                content,
                ..Default::default()
            })
            .unwrap();
            _ = this.set_user_settings(&new_text, cx);
        }
        trail(self, content, cx);
    }
}
