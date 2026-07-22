use super::*;

impl SettingsStore {
    pub async fn load_settings(fs: &Arc<dyn Fs>) -> Result<String> {
        match fs.load(paths::settings_file()).await {
            result @ Ok(_) => result,
            Err(err) => {
                if let Some(e) = err.downcast_ref::<std::io::Error>()
                    && e.kind() == std::io::ErrorKind::NotFound
                {
                    return Ok(crate::initial_user_settings_content().to_string());
                }
                Err(err)
            }
        }
    }

    fn update_settings_file_inner(
        &self,
        fs: Arc<dyn Fs>,
        update: impl 'static + Send + FnOnce(String, AsyncApp) -> Result<String>,
    ) -> oneshot::Receiver<Result<()>> {
        let (tx, rx) = oneshot::channel::<Result<()>>();
        self.setting_file_updates_tx
            .unbounded_send(Box::new(move |cx: AsyncApp| {
                async move {
                    let res = async move {
                        let old_text = Self::load_settings(&fs).await?;
                        let new_text = update(old_text, cx.clone())?;

                        let settings_path = paths::settings_file().as_path();
                        if fs.is_file(settings_path).await {
                            let resolved_path =
                                fs.canonicalize(settings_path).await.with_context(|| {
                                    format!(
                                        "Failed to canonicalize settings path {:?}",
                                        settings_path
                                    )
                                })?;

                            fs.atomic_write(resolved_path.clone(), new_text.clone())
                                .await
                                .with_context(|| {
                                    format!("Failed to write settings to file {:?}", resolved_path)
                                })?;
                        } else {
                            fs.atomic_write(settings_path.to_path_buf(), new_text.clone())
                                .await
                                .with_context(|| {
                                    format!("Failed to write settings to file {:?}", settings_path)
                                })?;
                        }

                        cx.update_global(|store: &mut SettingsStore, cx| {
                            store.set_user_settings(&new_text, cx).result().map(|_| ())
                        })
                    }
                    .await;

                    let new_res = match &res {
                        Ok(_) => anyhow::Ok(()),
                        Err(e) => Err(anyhow::anyhow!("{:?}", e)),
                    };

                    _ = tx.send(new_res);
                    res
                }
                .boxed_local()
            }))
            .map_err(|err| anyhow::format_err!("Failed to update settings file: {}", err))
            .log_with_level(log::Level::Warn);
        return rx;
    }

    pub fn update_settings_file(
        &self,
        fs: Arc<dyn Fs>,
        update: impl 'static + Send + FnOnce(&mut SettingsContent, &App),
    ) {
        _ = self.update_settings_file_with_completion(fs, update);
    }

    pub fn update_settings_file_with_completion(
        &self,
        fs: Arc<dyn Fs>,
        update: impl 'static + Send + FnOnce(&mut SettingsContent, &App),
    ) -> oneshot::Receiver<Result<()>> {
        self.update_settings_file_inner(fs, move |old_text: String, cx: AsyncApp| {
            cx.read_global(|store: &SettingsStore, cx| {
                store.new_text_for_update(old_text, |content| update(content, cx))
            })
        })
    }

    pub fn import_vscode_settings(
        &self,
        fs: Arc<dyn Fs>,
        vscode_settings: VsCodeSettings,
    ) -> oneshot::Receiver<Result<()>> {
        self.update_settings_file_inner(fs, move |old_text: String, cx: AsyncApp| {
            cx.read_global(|store: &SettingsStore, _cx| {
                store.get_vscode_edits(old_text, &vscode_settings)
            })
        })
    }

    pub fn get_all_files(&self) -> Vec<SettingsFile> {
        let mut files = Vec::from_iter(
            self.local_settings
                .keys()
                // rev because these are sorted by path, so highest precedence is last
                .rev()
                .cloned()
                .map(SettingsFile::Project),
        );

        if self.server_settings.is_some() {
            files.push(SettingsFile::Server);
        }
        // ignoring profiles
        // ignoring os profiles
        // ignoring release channel profiles
        // ignoring global
        // ignoring extension

        if self.user_settings.is_some() {
            files.push(SettingsFile::User);
        }
        files.push(SettingsFile::Default);
        files
    }

    pub fn get_content_for_file(&self, file: SettingsFile) -> Option<&SettingsContent> {
        match file {
            SettingsFile::User => self
                .user_settings
                .as_ref()
                .map(|settings| settings.content.as_ref()),
            SettingsFile::Default => Some(self.default_settings.as_ref()),
            SettingsFile::Server => self.server_settings.as_deref(),
            SettingsFile::Project(ref key) => self.local_settings.get(key),
            SettingsFile::Global => self.global_settings.as_deref(),
        }
    }

    pub fn get_overrides_for_field<T>(
        &self,
        target_file: SettingsFile,
        get: fn(&SettingsContent) -> &Option<T>,
    ) -> Vec<SettingsFile> {
        let all_files = self.get_all_files();
        let mut found_file = false;
        let mut overrides = Vec::new();

        for file in all_files.into_iter().rev() {
            if !found_file {
                found_file = file == target_file;
                continue;
            }

            if let SettingsFile::Project((wt_id, ref path)) = file
                && let SettingsFile::Project((target_wt_id, ref target_path)) = target_file
                && (wt_id != target_wt_id || !target_path.starts_with(path))
            {
                // if requesting value from a local file, don't return values from local files in different worktrees
                continue;
            }

            let Some(content) = self.get_content_for_file(file.clone()) else {
                continue;
            };
            if get(content).is_some() {
                overrides.push(file);
            }
        }

        overrides
    }

    /// Checks the given file, and files that the passed file overrides for the given field.
    /// Returns the first file found that contains the value.
    /// The value will only be None if no file contains the value.
    /// I.e. if no file contains the value, returns `(File::Default, None)`
    pub fn get_value_from_file<'a, T: 'a>(
        &'a self,
        target_file: SettingsFile,
        pick: fn(&'a SettingsContent) -> Option<T>,
    ) -> (SettingsFile, Option<T>) {
        self.get_value_from_file_inner(target_file, pick, true)
    }

    /// Same as `Self::get_value_from_file` except that it does not include the current file.
    /// Therefore it returns the value that was potentially overloaded by the target file.
    pub fn get_value_up_to_file<'a, T: 'a>(
        &'a self,
        target_file: SettingsFile,
        pick: fn(&'a SettingsContent) -> Option<T>,
    ) -> (SettingsFile, Option<T>) {
        self.get_value_from_file_inner(target_file, pick, false)
    }

    fn get_value_from_file_inner<'a, T: 'a>(
        &'a self,
        target_file: SettingsFile,
        pick: fn(&'a SettingsContent) -> Option<T>,
        include_target_file: bool,
    ) -> (SettingsFile, Option<T>) {
        // todo(settings_ui): Add a metadata field for overriding the "overrides" tag, for contextually different settings
        //  e.g. disable AI isn't overridden, or a vec that gets extended instead or some such

        // todo(settings_ui) cache all files
        let all_files = self.get_all_files();
        let mut found_file = false;

        for file in all_files.into_iter() {
            if !found_file && file != SettingsFile::Default {
                if file != target_file {
                    continue;
                }
                found_file = true;
                if !include_target_file {
                    continue;
                }
            }

            if let SettingsFile::Project((worktree_id, ref path)) = file
                && let SettingsFile::Project((target_worktree_id, ref target_path)) = target_file
                && (worktree_id != target_worktree_id || !target_path.starts_with(&path))
            {
                // if requesting value from a local file, don't return values from local files in different worktrees
                continue;
            }

            let Some(content) = self.get_content_for_file(file.clone()) else {
                continue;
            };
            if let Some(value) = pick(content) {
                return (file, Some(value));
            }
        }

        (SettingsFile::Default, None)
    }

    #[inline(always)]
    pub(super) fn parse_and_migrate_mav_settings<SettingsContentType: RootUserSettings>(
        &mut self,
        user_settings_content: &str,
        file: SettingsFile,
    ) -> (Option<SettingsContentType>, SettingsParseResult) {
        let mut migration_status = MigrationStatus::NotNeeded;
        let (settings, parse_status) = if user_settings_content.is_empty() {
            SettingsContentType::parse_json("{}")
        } else {
            let migration_res = migrator::migrate_settings(user_settings_content);
            migration_status = match &migration_res {
                Ok(Some(_)) => MigrationStatus::Succeeded,
                Ok(None) => MigrationStatus::NotNeeded,
                Err(err) => MigrationStatus::Failed {
                    error: err.to_string(),
                },
            };
            let content = match &migration_res {
                Ok(Some(content)) => content,
                Ok(None) => user_settings_content,
                Err(_) => user_settings_content,
            };
            SettingsContentType::parse_json(content)
        };

        let result = SettingsParseResult {
            parse_status,
            migration_status,
        };
        self.file_errors.insert(file, result.clone());
        return (settings, result);
    }

    pub fn error_for_file(&self, file: SettingsFile) -> Option<SettingsParseResult> {
        self.file_errors
            .get(&file)
            .filter(|parse_result| parse_result.requires_user_action())
            .cloned()
    }
}
