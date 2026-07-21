use super::*;

impl LspStore {
    pub(super) fn maintain_buffer_languages(
        languages: Arc<LanguageRegistry>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let mut subscription = languages.subscribe();
        let mut prev_reload_count = languages.reload_count();
        cx.spawn(async move |this, cx| {
            while let Some(()) = subscription.next().await {
                if let Some(this) = this.upgrade() {
                    // If the language registry has been reloaded, then remove and
                    // re-assign the languages on all open buffers.
                    let reload_count = languages.reload_count();
                    if reload_count > prev_reload_count {
                        prev_reload_count = reload_count;
                        this.update(cx, |this, cx| {
                            this.buffer_store.clone().update(cx, |buffer_store, cx| {
                                for buffer in buffer_store.buffers() {
                                    if let Some(f) = File::from_dyn(buffer.read(cx).file()).cloned()
                                    {
                                        buffer.update(cx, |buffer, cx| {
                                            buffer.set_language_async(None, cx)
                                        });
                                        if let Some(local) = this.as_local_mut() {
                                            local.reset_buffer(&buffer, &f, cx);

                                            if local
                                                .registered_buffers
                                                .contains_key(&buffer.read(cx).remote_id())
                                                && let Some(file_url) =
                                                    file_path_to_lsp_url(&f.abs_path(cx)).log_err()
                                            {
                                                local.unregister_buffer_from_language_servers(
                                                    &buffer, &file_url, cx,
                                                );
                                            }
                                        }
                                    }
                                }
                            });
                        });
                    }

                    this.update(cx, |this, cx| {
                        let mut plain_text_buffers = Vec::new();
                        let mut buffers_with_language = Vec::new();
                        let mut buffers_with_unknown_injections = Vec::new();
                        for handle in this.buffer_store.read(cx).buffers() {
                            let buffer = handle.read(cx);
                            if buffer.language().is_none()
                                || buffer.language() == Some(&*language::PLAIN_TEXT)
                            {
                                plain_text_buffers.push(handle);
                            } else {
                                if buffer.contains_unknown_injections() {
                                    buffers_with_unknown_injections.push(handle.clone());
                                }
                                buffers_with_language.push(handle);
                            }
                        }

                        // Deprioritize the invisible worktrees so main worktrees' language servers can be started first,
                        // and reused later in the invisible worktrees.
                        plain_text_buffers.sort_by_key(|buffer| {
                            Reverse(
                                File::from_dyn(buffer.read(cx).file())
                                    .map(|file| file.worktree.read(cx).is_visible()),
                            )
                        });

                        for buffer in plain_text_buffers {
                            this.detect_language_for_buffer(&buffer, cx);
                            if let Some(local) = this.as_local_mut() {
                                local.initialize_buffer(&buffer, cx);
                                if local
                                    .registered_buffers
                                    .contains_key(&buffer.read(cx).remote_id())
                                {
                                    local.register_buffer_with_language_servers(
                                        &buffer,
                                        HashSet::default(),
                                        cx,
                                    );
                                }
                            }
                        }

                        // Also register buffers that already have a language with
                        // any newly-available language servers (e.g., from extensions
                        // that finished loading after buffers were restored).
                        if let Some(local) = this.as_local_mut() {
                            for buffer in buffers_with_language {
                                if local
                                    .registered_buffers
                                    .contains_key(&buffer.read(cx).remote_id())
                                {
                                    local.register_buffer_with_language_servers(
                                        &buffer,
                                        HashSet::default(),
                                        cx,
                                    );
                                }
                            }
                        }

                        for buffer in buffers_with_unknown_injections {
                            buffer.update(cx, |buffer, cx| buffer.reparse(cx, false));
                        }
                    });
                }
            }
        })
    }

    pub(super) fn parse_modeline(
        &mut self,
        buffer_handle: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> bool {
        let buffer = buffer_handle.read(cx);
        let content = buffer.as_rope();

        let modeline_settings = {
            let settings_store = cx.global::<SettingsStore>();
            let modeline_lines = settings_store
                .raw_user_settings()
                .and_then(|s| s.content.modeline_lines)
                .or(settings_store.raw_default_settings().modeline_lines)
                .unwrap_or(5);

            const MAX_MODELINE_BYTES: usize = 1024;

            let first_bytes =
                content.clip_offset(content.len().min(MAX_MODELINE_BYTES), Bias::Left);
            let mut first_lines = Vec::new();
            let mut lines = content.chunks_in_range(0..first_bytes).lines();
            for _ in 0..modeline_lines {
                if let Some(line) = lines.next() {
                    first_lines.push(line.to_string());
                } else {
                    break;
                }
            }
            let first_lines_ref: Vec<_> = first_lines.iter().map(|line| line.as_str()).collect();

            let last_start =
                content.clip_offset(content.len().saturating_sub(MAX_MODELINE_BYTES), Bias::Left);
            let mut last_lines = Vec::new();
            let mut lines = content
                .reversed_chunks_in_range(last_start..content.len())
                .lines();
            for _ in 0..modeline_lines {
                if let Some(line) = lines.next() {
                    last_lines.push(line.to_string());
                } else {
                    break;
                }
            }
            let last_lines_ref: Vec<_> =
                last_lines.iter().rev().map(|line| line.as_str()).collect();
            modeline::parse_modeline(&first_lines_ref, &last_lines_ref)
        };

        log::debug!("Parsed modeline settings: {:?}", modeline_settings);

        buffer_handle.update(cx, |buffer, _cx| buffer.set_modeline(modeline_settings))
    }

    pub(super) fn detect_language_for_buffer(
        &mut self,
        buffer_handle: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Option<language::AvailableLanguage> {
        // If the buffer has a language, set it and start the language server if we haven't already.
        let buffer = buffer_handle.read(cx);
        let file = buffer.file()?;
        let content = buffer.as_rope();
        let modeline_settings = buffer.modeline().map(Arc::as_ref);

        let available_language = if let Some(ModelineSettings {
            mode: Some(mode_name),
            ..
        }) = modeline_settings
        {
            self.languages
                .available_language_for_modeline_name(mode_name)
        } else {
            self.languages.language_for_file(file, Some(content), cx)
        };
        if let Some(available_language) = &available_language {
            if let Some(Ok(Ok(new_language))) = self
                .languages
                .load_language(available_language)
                .now_or_never()
            {
                self.set_language_for_buffer(buffer_handle, new_language, cx);
            }
        } else {
            cx.emit(LspStoreEvent::LanguageDetected {
                buffer: buffer_handle.clone(),
                new_language: None,
            });
        }

        available_language
    }

    pub(crate) fn set_language_for_buffer(
        &mut self,
        buffer_entity: &Entity<Buffer>,
        new_language: Arc<Language>,
        cx: &mut Context<Self>,
    ) {
        let buffer = buffer_entity.read(cx);
        let buffer_file = buffer.file().cloned();
        let buffer_id = buffer.remote_id();
        if let Some(local_store) = self.as_local_mut()
            && local_store.registered_buffers.contains_key(&buffer_id)
            && let Some(abs_path) =
                File::from_dyn(buffer_file.as_ref()).map(|file| file.abs_path(cx))
            && let Some(file_url) = file_path_to_lsp_url(&abs_path).log_err()
        {
            local_store.unregister_buffer_from_language_servers(buffer_entity, &file_url, cx);
        }
        buffer_entity.update(cx, |buffer, cx| {
            if buffer
                .language()
                .is_none_or(|old_language| !Arc::ptr_eq(old_language, &new_language))
            {
                buffer.set_language_async(Some(new_language.clone()), cx);
            }
        });

        let settings = LanguageSettings::resolve(
            Some(&buffer_entity.read(cx)),
            Some(&new_language.name()),
            cx,
        )
        .into_owned();
        let buffer_file = File::from_dyn(buffer_file.as_ref());

        let worktree_id = if let Some(file) = buffer_file {
            let worktree = file.worktree.clone();

            if let Some(local) = self.as_local_mut()
                && local.registered_buffers.contains_key(&buffer_id)
            {
                local.register_buffer_with_language_servers(buffer_entity, HashSet::default(), cx);
            }
            Some(worktree.read(cx).id())
        } else {
            None
        };

        if settings.prettier.allowed
            && let Some(prettier_plugins) = prettier_store::prettier_plugins_for_language(&settings)
        {
            let prettier_store = self.as_local().map(|s| s.prettier_store.clone());
            if let Some(prettier_store) = prettier_store {
                prettier_store.update(cx, |prettier_store, cx| {
                    prettier_store.install_default_prettier(
                        worktree_id,
                        prettier_plugins.iter().map(|s| Arc::from(s.as_str())),
                        cx,
                    )
                })
            }
        }

        cx.emit(LspStoreEvent::LanguageDetected {
            buffer: buffer_entity.clone(),
            new_language: Some(new_language),
        })
    }
}
