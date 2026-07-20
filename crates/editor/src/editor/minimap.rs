use super::*;

impl Editor {
    pub fn supports_minimap(&self, cx: &App) -> bool {
        !self.minimap_visibility.disabled() && self.buffer_kind(cx) == ItemBufferKind::Singleton
    }

    pub fn toggle_minimap(
        &mut self,
        _: &ToggleMinimap,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        if self.supports_minimap(cx) {
            self.set_minimap_visibility(self.minimap_visibility.toggle_visibility(), window, cx);
        }
    }

    pub(super) fn create_minimap(
        &self,
        minimap_settings: MinimapSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<Self>> {
        (minimap_settings.minimap_enabled() && self.buffer_kind(cx) == ItemBufferKind::Singleton)
            .then(|| self.initialize_new_minimap(minimap_settings, window, cx))
    }

    fn initialize_new_minimap(
        &self,
        minimap_settings: MinimapSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Self> {
        const MINIMAP_FONT_WEIGHT: gpui::FontWeight = gpui::FontWeight::BLACK;
        const MINIMAP_FONT_FAMILY: SharedString = SharedString::new_static(".MavMono");

        let mut minimap = Editor::new_internal(
            EditorMode::Minimap {
                parent: cx.weak_entity(),
            },
            self.buffer.clone(),
            None,
            Some(self.display_map.clone()),
            window,
            cx,
        );
        let my_snapshot = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let minimap_snapshot = minimap.display_map.update(cx, |map, cx| map.snapshot(cx));
        minimap.scroll_manager.clone_state(
            &self.scroll_manager,
            &my_snapshot,
            &minimap_snapshot,
            cx,
        );
        minimap.set_text_style_refinement(TextStyleRefinement {
            font_size: Some(MINIMAP_FONT_SIZE),
            font_weight: Some(MINIMAP_FONT_WEIGHT),
            font_family: Some(MINIMAP_FONT_FAMILY),
            ..Default::default()
        });
        minimap.update_minimap_configuration(minimap_settings, cx);
        cx.new(|_| minimap)
    }

    pub(super) fn update_minimap_configuration(
        &mut self,
        minimap_settings: MinimapSettings,
        cx: &App,
    ) {
        let current_line_highlight = minimap_settings
            .current_line_highlight
            .unwrap_or_else(|| EditorSettings::get_global(cx).current_line_highlight);
        self.set_current_line_highlight(Some(current_line_highlight));
    }

    pub fn minimap(&self) -> Option<&Entity<Self>> {
        self.minimap
            .as_ref()
            .filter(|_| self.minimap_visibility.visible())
    }
}
