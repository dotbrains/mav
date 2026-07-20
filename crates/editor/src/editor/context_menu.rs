use super::*;

impl Editor {
    pub fn context_menu_visible(&self) -> bool {
        !self.edit_prediction_preview_is_active()
            && self
                .context_menu
                .borrow()
                .as_ref()
                .is_some_and(|menu| menu.visible())
    }

    pub fn context_menu_origin(&self) -> Option<ContextMenuOrigin> {
        self.context_menu
            .borrow()
            .as_ref()
            .map(|menu| menu.origin())
    }

    pub fn set_context_menu_options(&mut self, options: ContextMenuOptions) {
        self.context_menu_options = Some(options);
    }

    pub(crate) fn current_user_player_color(&self, cx: &mut App) -> PlayerColor {
        if self.read_only(cx) {
            cx.theme().players().read_only()
        } else {
            self.style.as_ref().unwrap().local_player
        }
    }

    pub fn render_context_menu(
        &mut self,
        max_height_in_lines: u32,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Option<AnyElement> {
        let menu = self.context_menu.borrow();
        let menu = menu.as_ref()?;
        if !menu.visible() {
            return None;
        };
        self.style
            .as_ref()
            .map(|style| menu.render(style, max_height_in_lines, window, cx))
    }

    pub(crate) fn render_context_menu_aside(
        &mut self,
        max_size: Size<Pixels>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Option<AnyElement> {
        self.context_menu.borrow_mut().as_mut().and_then(|menu| {
            if menu.visible() {
                menu.render_aside(max_size, window, cx)
            } else {
                None
            }
        })
    }

    pub(crate) fn hide_context_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<CodeContextMenu> {
        cx.notify();
        self.completion_tasks.clear();
        let context_menu = self.context_menu.borrow_mut().take();
        self.stale_edit_prediction_in_menu.take();
        self.update_visible_edit_prediction(window, cx);
        if let Some(CodeContextMenu::Completions(_)) = &context_menu
            && let Some(completion_provider) = &self.completion_provider
        {
            completion_provider.selection_changed(None, window, cx);
        }
        context_menu
    }

    pub(crate) fn set_gutter_context_menu(
        &mut self,
        display_row: DisplayRow,
        position: Option<Anchor>,
        clicked_point: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = self
            .buffer
            .read(cx)
            .snapshot(cx)
            .anchor_before(Point::new(display_row.0, 0u32));

        let context_menu = self.gutter_context_menu(position.unwrap_or(source), window, cx);

        self.mouse_context_menu = MouseContextMenu::pinned_to_editor(
            self,
            source,
            clicked_point,
            context_menu,
            window,
            cx,
        );
    }
}
