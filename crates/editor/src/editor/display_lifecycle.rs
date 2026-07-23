use super::*;

impl Editor {
    pub(crate) fn create_style(&self, cx: &App) -> EditorStyle {
        let settings = ThemeSettings::get_global(cx);
        let editor_settings = EditorSettings::get_global(cx);

        let mut text_style = match self.mode {
            EditorMode::SingleLine | EditorMode::AutoHeight { .. } => TextStyle {
                color: cx.theme().colors().editor_foreground,
                font_family: settings.ui_font.family.clone(),
                font_features: settings.ui_font.features.clone(),
                font_fallbacks: settings.ui_font.fallbacks.clone(),
                font_size: rems(0.875).into(),
                font_weight: settings.ui_font.weight,
                line_height: relative(settings.buffer_line_height.value()),
                ..Default::default()
            },
            EditorMode::Full { .. } | EditorMode::Minimap { .. } => TextStyle {
                color: cx.theme().colors().editor_foreground,
                font_family: settings.buffer_font.family.clone(),
                font_features: settings.buffer_font.features.clone(),
                font_fallbacks: settings.buffer_font.fallbacks.clone(),
                font_size: settings.buffer_font_size(cx).into(),
                font_weight: settings.buffer_font.weight,
                line_height: relative(settings.buffer_line_height.value()),
                ..Default::default()
            },
        };
        if let Some(text_style_refinement) = &self.text_style_refinement {
            text_style.refine(text_style_refinement)
        }

        let background = match self.mode {
            EditorMode::SingleLine => cx.theme().system().transparent,
            EditorMode::AutoHeight { .. } => cx.theme().system().transparent,
            EditorMode::Full { .. } => cx.theme().colors().editor_background,
            EditorMode::Minimap { .. } => cx.theme().colors().editor_background.opacity(0.7),
        };

        EditorStyle {
            background,
            border: cx.theme().colors().border,
            local_player: cx.theme().players().local(),
            text: text_style,
            scrollbar_width: px(editor_settings.scrollbar.size.max(0.0)),
            syntax: cx.theme().syntax().clone(),
            status: cx.theme().status().clone(),
            inlay_hints_style: make_inlay_hints_style(cx),
            edit_prediction_styles: make_suggestion_styles(cx),
            unnecessary_code_fade: settings.unnecessary_code_fade,
            show_underlines: self.diagnostics_enabled(),
        }
    }

    pub(crate) fn breadcrumbs_inner(&self, cx: &App) -> Option<Vec<HighlightedText>> {
        let multibuffer = self.buffer().read(cx);
        let is_singleton = multibuffer.is_singleton();
        let show_symbols = EditorSettings::get_global(cx)
            .toolbar
            .show_breadcrumb_symbols;
        let buffer = if show_symbols {
            let (buffer_id, _) = self.outline_symbols_at_cursor.as_ref()?;
            multibuffer.buffer(*buffer_id)?
        } else {
            self.active_buffer(cx)?
        };

        let buffer = buffer.read(cx);
        let mut breadcrumbs = if is_singleton {
            let text = self.breadcrumb_header.clone().unwrap_or_else(|| {
                buffer
                    .snapshot()
                    .resolve_file_path(
                        self.project
                            .as_ref()
                            .map(|project| project.read(cx).visible_worktrees(cx).count() > 1)
                            .unwrap_or_default(),
                        cx,
                    )
                    .unwrap_or_else(|| {
                        if multibuffer.is_singleton() {
                            multibuffer.title(cx).to_string()
                        } else {
                            "untitled".to_string()
                        }
                    })
            });
            vec![HighlightedText {
                text: text.into(),
                highlights: vec![],
            }]
        } else {
            vec![]
        };

        if show_symbols {
            let (_, symbols) = self.outline_symbols_at_cursor.as_ref()?;
            breadcrumbs.extend(symbols.iter().map(|symbol| HighlightedText {
                text: symbol.text.clone(),
                highlights: symbol.highlight_ranges.clone(),
            }));
        }

        if breadcrumbs.is_empty() {
            None
        } else {
            Some(breadcrumbs)
        }
    }

    pub(crate) fn disable_lsp_data(&mut self) {
        self.enable_lsp_data = false;
    }

    pub(crate) fn disable_runnables(&mut self) {
        self.enable_runnables = false;
    }

    pub fn disable_code_lens(&mut self, cx: &mut Context<Self>) {
        self.enable_code_lens = false;
        self.clear_code_lenses(cx);
    }

    pub fn disable_mouse_wheel_zoom(&mut self) {
        self.enable_mouse_wheel_zoom = false;
    }

    pub(crate) fn update_data_on_scroll(
        &mut self,
        debounce: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if debounce {
            self.post_scroll_update = cx.spawn_in(window, async move |editor, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                editor
                    .update_in(cx, |editor, window, cx| {
                        editor.do_update_data_on_scroll(window, cx);
                    })
                    .ok();
            });
        } else {
            self.post_scroll_update = Task::ready(());
            self.do_update_data_on_scroll(window, cx);
        }
    }

    pub(crate) fn do_update_data_on_scroll(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.register_visible_buffers(cx);
        self.colorize_brackets(false, cx);
        self.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
        self.resolve_visible_code_lenses(cx);

        if !self.buffer().read(cx).is_singleton() || self.needs_initial_data_update {
            self.needs_initial_data_update = false;
            self.update_lsp_data(None, window, cx);
            self.refresh_runnables(None, window, cx);
        }
    }

    pub fn cursor_top_offset(&self, cx: &mut Context<Self>) -> Option<ScrollOffset> {
        let visible = self.visible_line_count()?;
        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let scroll_top = self.scroll_manager.scroll_position(&display_map, cx).y;
        let cursor_display_row = self
            .selections
            .newest::<Point>(&display_map)
            .head()
            .to_display_point(&display_map)
            .row()
            .as_f64();

        match cursor_display_row - scroll_top {
            offset if offset < 0.0 || offset >= visible => None,
            offset => Some(offset),
        }
    }
}
