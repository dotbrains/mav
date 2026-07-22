use super::*;

impl SettingsStore {
    /// Sets the user settings via a JSON string.
    #[must_use]
    pub fn set_user_settings(
        &mut self,
        user_settings_content: &str,
        cx: &mut App,
    ) -> SettingsParseResult {
        if self.last_user_settings_content.as_deref() == Some(user_settings_content) {
            return SettingsParseResult {
                parse_status: ParseStatus::Unchanged,
                migration_status: MigrationStatus::NotNeeded,
            };
        }
        self.last_user_settings_content = Some(user_settings_content.to_string());

        let (settings, parse_result) = self.parse_and_migrate_mav_settings::<UserSettingsContent>(
            user_settings_content,
            SettingsFile::User,
        );

        if let Some(settings) = settings {
            self.user_settings = Some(settings);
            self.recompute_values(None, cx);
        }
        return parse_result;
    }

    /// Sets the global settings via a JSON string.
    #[must_use]
    pub fn set_global_settings(
        &mut self,
        global_settings_content: &str,
        cx: &mut App,
    ) -> SettingsParseResult {
        if self.last_global_settings_content.as_deref() == Some(global_settings_content) {
            return SettingsParseResult {
                parse_status: ParseStatus::Unchanged,
                migration_status: MigrationStatus::NotNeeded,
            };
        }
        self.last_global_settings_content = Some(global_settings_content.to_string());

        let (settings, parse_result) = self.parse_and_migrate_mav_settings::<SettingsContent>(
            global_settings_content,
            SettingsFile::Global,
        );

        if let Some(settings) = settings {
            self.global_settings = Some(Box::new(settings));
            self.recompute_values(None, cx);
        }
        return parse_result;
    }

    pub fn set_server_settings(
        &mut self,
        server_settings_content: &str,
        cx: &mut App,
    ) -> Result<()> {
        let settings = if server_settings_content.is_empty() {
            None
        } else {
            Option::<SettingsContent>::parse_json_with_comments(server_settings_content)?
        };

        // Rewrite the server settings into a content type
        self.server_settings = settings.map(|settings| Box::new(settings));

        self.recompute_values(None, cx);
        Ok(())
    }

    /// Sets language-specific semantic token rules.
    ///
    /// These rules are registered by language modules (e.g. the Rust language module)
    /// or by third-party extensions (via `semantic_token_rules.json` in their language
    /// directories). They are stored separately from the global rules and are only
    /// applied to buffers of the matching language by the `SemanticTokenStylizer`.
    ///
    /// This triggers a settings recomputation so that observers (e.g. `LspStore`)
    /// are notified and can invalidate cached stylizers.
    pub fn set_language_semantic_token_rules(
        &mut self,
        language: SharedString,
        rules: SemanticTokenRules,
        cx: &mut App,
    ) {
        self.language_semantic_token_rules.insert(language, rules);
        self.recompute_values(None, cx);
    }

    /// Removes language-specific semantic token rules for the given language.
    ///
    /// This should be called when an extension that registered rules for a language
    /// is unloaded. Triggers a settings recomputation so that observers (e.g.
    /// `LspStore`) are notified and can invalidate cached stylizers.
    pub fn remove_language_semantic_token_rules(&mut self, language: &str, cx: &mut App) {
        self.language_semantic_token_rules.remove(language);
        self.recompute_values(None, cx);
    }

    /// Returns the language-specific semantic token rules for the given language,
    /// if any have been registered.
    pub fn language_semantic_token_rules(&self, language: &str) -> Option<&SemanticTokenRules> {
        self.language_semantic_token_rules.get(language)
    }

    /// Add or remove a set of local settings via a JSON string.
    pub fn set_local_settings(
        &mut self,
        root_id: WorktreeId,
        path: LocalSettingsPath,
        kind: LocalSettingsKind,
        settings_content: Option<&str>,
        cx: &mut App,
    ) -> std::result::Result<(), InvalidSettingsError> {
        let content = settings_content
            .map(|content| content.trim())
            .filter(|content| !content.is_empty());
        let mut mav_settings_changed = false;
        match (path.clone(), kind, content) {
            (LocalSettingsPath::InWorktree(directory_path), LocalSettingsKind::Tasks, _) => {
                return Err(InvalidSettingsError::Tasks {
                    message: "Attempted to submit tasks into the settings store".to_string(),
                    path: directory_path
                        .join(RelPath::unix(task_file_name()).unwrap())
                        .as_std_path()
                        .to_path_buf(),
                });
            }
            (LocalSettingsPath::InWorktree(directory_path), LocalSettingsKind::Debug, _) => {
                return Err(InvalidSettingsError::Debug {
                    message: "Attempted to submit debugger config into the settings store"
                        .to_string(),
                    path: directory_path
                        .join(RelPath::unix(task_file_name()).unwrap())
                        .as_std_path()
                        .to_path_buf(),
                });
            }
            (LocalSettingsPath::InWorktree(directory_path), LocalSettingsKind::Settings, None) => {
                mav_settings_changed = self
                    .local_settings
                    .remove(&(root_id, directory_path.clone()))
                    .is_some();
                self.file_errors
                    .remove(&SettingsFile::Project((root_id, directory_path)));
            }
            (
                LocalSettingsPath::InWorktree(directory_path),
                LocalSettingsKind::Settings,
                Some(settings_contents),
            ) => {
                let (new_settings, parse_result) = self
                    .parse_and_migrate_mav_settings::<ProjectSettingsContent>(
                        settings_contents,
                        SettingsFile::Project((root_id, directory_path.clone())),
                    );
                match parse_result.parse_status {
                    ParseStatus::Success => Ok(()),
                    ParseStatus::Unchanged => Ok(()),
                    ParseStatus::Failed { error } => Err(InvalidSettingsError::LocalSettings {
                        path: directory_path.join(local_settings_file_relative_path()),
                        message: error,
                    }),
                }?;
                if let Some(new_settings) = new_settings {
                    match self.local_settings.entry((root_id, directory_path)) {
                        btree_map::Entry::Vacant(v) => {
                            v.insert(SettingsContent {
                                project: new_settings,
                                ..Default::default()
                            });
                            mav_settings_changed = true;
                        }
                        btree_map::Entry::Occupied(mut o) => {
                            if &o.get().project != &new_settings {
                                o.insert(SettingsContent {
                                    project: new_settings,
                                    ..Default::default()
                                });
                                mav_settings_changed = true;
                            }
                        }
                    }
                }
            }
            (directory_path, LocalSettingsKind::Editorconfig, editorconfig_contents) => {
                self.editorconfig_store.update(cx, |store, _| {
                    store.set_configs(root_id, directory_path, editorconfig_contents)
                })?;
            }
            (LocalSettingsPath::OutsideWorktree(path), kind, _) => {
                log::error!(
                    "OutsideWorktree path {:?} with kind {:?} is only supported by editorconfig",
                    path,
                    kind
                );
                return Ok(());
            }
        }
        if let LocalSettingsPath::InWorktree(directory_path) = &path {
            if mav_settings_changed {
                self.recompute_values(Some((root_id, &directory_path)), cx);
            }
        }
        Ok(())
    }

    pub fn set_extension_settings(
        &mut self,
        content: ExtensionsSettingsContent,
        cx: &mut App,
    ) -> Result<()> {
        self.extension_settings = Some(Box::new(SettingsContent {
            project: ProjectSettingsContent {
                all_languages: content.all_languages,
                ..Default::default()
            },
            ..Default::default()
        }));
        self.recompute_values(None, cx);
        Ok(())
    }

    /// Add or remove a set of local settings via a JSON string.
    pub fn clear_local_settings(&mut self, root_id: WorktreeId, cx: &mut App) -> Result<()> {
        self.local_settings
            .retain(|(worktree_id, _), _| worktree_id != &root_id);

        self.editorconfig_store
            .update(cx, |store, _cx| store.remove_for_worktree(root_id));

        for setting_value in self.setting_values.values_mut() {
            setting_value.clear_local_values(root_id);
        }
        self.recompute_values(Some((root_id, RelPath::empty())), cx);
        Ok(())
    }

    pub fn local_settings(
        &self,
        root_id: WorktreeId,
    ) -> impl '_ + Iterator<Item = (Arc<RelPath>, &ProjectSettingsContent)> {
        self.local_settings
            .range(
                (root_id, RelPath::empty_arc())
                    ..(
                        WorktreeId::from_usize(root_id.to_usize() + 1),
                        RelPath::empty_arc(),
                    ),
            )
            .map(|((_, path), content)| (path.clone(), &content.project))
    }
}
