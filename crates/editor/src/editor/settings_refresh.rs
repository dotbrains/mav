use super::*;

impl Editor {
    pub(crate) fn fetch_accent_data(&self, cx: &App) -> Option<AccentData> {
        if !self.mode.is_full() {
            return None;
        }

        let theme_settings = theme_settings::ThemeSettings::get_global(cx);
        let theme = cx.theme();
        let accent_colors = theme.accents().clone();

        let accent_overrides = theme_settings
            .theme_overrides
            .get(theme.name.as_ref())
            .map(|theme_style| &theme_style.accents)
            .into_iter()
            .flatten()
            .chain(
                theme_settings
                    .experimental_theme_overrides
                    .as_ref()
                    .map(|overrides| &overrides.accents)
                    .into_iter()
                    .flatten(),
            )
            .flat_map(|accent| accent.0.clone().map(SharedString::from))
            .collect();

        Some(AccentData {
            colors: accent_colors,
            overrides: accent_overrides,
        })
    }

    pub(crate) fn fetch_applicable_language_settings(
        &self,
        cx: &App,
    ) -> HashMap<Option<LanguageName>, LanguageSettings> {
        if !self.mode.is_full() {
            return HashMap::default();
        }

        self.buffer().read(cx).all_buffers().into_iter().fold(
            HashMap::default(),
            |mut acc, buffer| {
                let buffer = buffer.read(cx);
                let language = buffer.language().map(|language| language.name());
                if let hash_map::Entry::Vacant(v) = acc.entry(language) {
                    v.insert(LanguageSettings::for_buffer(&buffer, cx).into_owned());
                }
                acc
            },
        )
    }

    pub(crate) fn settings_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_language_settings = self.fetch_applicable_language_settings(cx);
        let language_settings_changed = new_language_settings != self.applicable_language_settings;
        self.applicable_language_settings = new_language_settings;

        let new_accents = self.fetch_accent_data(cx);
        let accents_changed = new_accents != self.accent_data;
        self.accent_data = new_accents;

        if self.diagnostics_enabled() {
            let new_severity = EditorSettings::get_global(cx)
                .diagnostics_max_severity
                .unwrap_or(DiagnosticSeverity::Hint);
            self.set_max_diagnostics_severity(new_severity, cx);
        }
        self.refresh_runnables(None, window, cx);
        self.update_edit_prediction_settings(cx);
        self.refresh_edit_prediction(true, false, EditPredictionRequestTrigger::Other, window, cx);
        self.refresh_inline_values(cx);

        let old_cursor_shape = self.cursor_shape;
        let old_breadcrumbs_visible = self.breadcrumbs_visible();

        {
            let editor_settings = EditorSettings::get_global(cx);
            self.scroll_manager.vertical_scroll_margin = editor_settings.vertical_scroll_margin;
            if self.breadcrumbs_visibility.settings_visibility()
                != editor_settings.toolbar.breadcrumbs
            {
                self.breadcrumbs_visibility =
                    BreadcrumbsVisibility::new(editor_settings.toolbar.breadcrumbs);
            }
            self.cursor_shape = editor_settings.cursor_shape.unwrap_or_default();
        }

        if old_cursor_shape != self.cursor_shape {
            cx.emit(EditorEvent::CursorShapeChanged);
        }

        if old_breadcrumbs_visible != self.breadcrumbs_visible() {
            cx.emit(EditorEvent::BreadcrumbsChanged);
        }

        let (restore_unsaved_buffers, show_inline_diagnostics, inline_blame_enabled) = {
            let project_settings = ProjectSettings::get_global(cx);
            (
                project_settings.session.restore_unsaved_buffers,
                project_settings.diagnostics.inline.enabled,
                project_settings.git.inline_blame.enabled,
            )
        };
        self.buffer_serialization = self
            .should_serialize_buffer()
            .then(|| BufferSerialization::new(restore_unsaved_buffers));

        if self.mode.is_full() {
            if self.show_inline_diagnostics != show_inline_diagnostics {
                self.show_inline_diagnostics = show_inline_diagnostics;
                self.refresh_inline_diagnostics(false, window, cx);
            }

            if self.git_blame_inline_enabled != inline_blame_enabled {
                self.toggle_git_blame_inline_internal(false, window, cx);
            }

            let minimap_settings = EditorSettings::get_global(cx).minimap;
            if self.minimap_visibility != MinimapVisibility::Disabled {
                if self.minimap_visibility.settings_visibility()
                    != minimap_settings.minimap_enabled()
                {
                    self.set_minimap_visibility(
                        MinimapVisibility::for_mode(self.mode(), cx),
                        window,
                        cx,
                    );
                } else if let Some(minimap_entity) = self.minimap.as_ref() {
                    minimap_entity.update(cx, |minimap_editor, cx| {
                        minimap_editor.update_minimap_configuration(minimap_settings, cx)
                    })
                }
            }

            if language_settings_changed || accents_changed {
                self.colorize_brackets(true, cx);
            }

            if language_settings_changed {
                self.clear_disabled_lsp_folding_ranges(window, cx);
                self.refresh_document_symbols(None, cx);
            }

            if let Some(inlay_splice) = self.colors.as_mut().and_then(|colors| {
                colors.render_mode_updated(EditorSettings::get_global(cx).lsp_document_colors)
            }) {
                if !inlay_splice.is_empty() {
                    self.splice_inlays(&inlay_splice.to_remove, inlay_splice.to_insert, cx);
                }
                self.refresh_document_colors(None, window, cx);
            }

            let code_lens_inline =
                self.enable_code_lens && EditorSettings::get_global(cx).code_lens.inline();
            let was_inline = self.code_lens.is_some();
            if code_lens_inline != was_inline {
                self.toggle_code_lens(code_lens_inline, window, cx);
            }

            let lsp_document_links_enabled = EditorSettings::get_global(cx).lsp_document_links;
            if lsp_document_links_enabled != self.lsp_document_links.enabled {
                self.lsp_document_links.enabled = lsp_document_links_enabled;
                if lsp_document_links_enabled {
                    self.refresh_document_links(None, cx);
                } else {
                    self.lsp_document_links.per_buffer.clear();
                    self.lsp_document_links.refresh_task = Task::ready(());
                }
            }

            self.refresh_inlay_hints(
                InlayHintRefreshReason::SettingsChange(inlay_hint_settings(
                    self.selections.newest_anchor().head(),
                    &self.buffer.read(cx).snapshot(cx),
                    cx,
                )),
                cx,
            );

            let new_semantic_token_rules = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .semantic_token_rules
                .clone();
            let semantic_token_rules_changed = self
                .semantic_token_state
                .update_rules(new_semantic_token_rules);
            if language_settings_changed || semantic_token_rules_changed {
                self.invalidate_semantic_tokens(None);
                self.refresh_semantic_tokens(None, None, cx);
            }
        }

        cx.notify();
    }

    pub(crate) fn theme_changed(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if !self.mode.is_full() {
            return;
        }

        let new_accents = self.fetch_accent_data(cx);
        if new_accents != self.accent_data {
            self.accent_data = new_accents;
            self.colorize_brackets(true, cx);
        }

        self.invalidate_semantic_tokens(None);
        self.refresh_semantic_tokens(None, None, cx);
        self.refresh_outline_symbols_at_cursor(cx);
    }
}
