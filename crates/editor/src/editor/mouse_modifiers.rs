use super::*;

impl Editor {
    pub fn display_cursor_names(
        &mut self,
        _: &DisplayCursorNames,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_cursor_names(window, cx);
    }

    pub(crate) fn show_cursor_names(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_cursor_names = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(CURSORS_VISIBLE_FOR).await;
            this.update(cx, |this, cx| {
                this.show_cursor_names = false;
                cx.notify()
            })
            .ok()
        })
        .detach();
    }

    pub(crate) fn handle_modifiers_changed(
        &mut self,
        modifiers: Modifiers,
        position_map: &PositionMap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_edit_prediction_settings(cx);

        if self.show_edit_predictions_in_menu()
            || self.edit_prediction_requires_modifier()
            || matches!(
                self.edit_prediction_preview,
                EditPredictionPreview::Active { .. }
            )
        {
            self.update_edit_prediction_preview(&modifiers, window, cx);
        }

        self.update_selection_mode(&modifiers, position_map, window, cx);

        let mouse_position = window.mouse_position();
        if !position_map.text_hitbox.is_hovered(window) {
            if self.gutter_hover_button.0.is_some() {
                cx.notify();
            }
            return;
        }

        self.update_hovered_link(
            position_map.point_for_position(mouse_position),
            Some(mouse_position),
            &position_map.snapshot,
            modifiers,
            window,
            cx,
        )
    }

    pub(crate) fn is_cmd_or_ctrl_pressed(modifiers: &Modifiers, cx: &mut Context<Self>) -> bool {
        match EditorSettings::get_global(cx).multi_cursor_modifier {
            MultiCursorModifier::Alt => modifiers.secondary(),
            MultiCursorModifier::CmdOrCtrl => modifiers.alt,
        }
    }

    pub(crate) fn is_alt_pressed(modifiers: &Modifiers, cx: &mut Context<Self>) -> bool {
        match EditorSettings::get_global(cx).multi_cursor_modifier {
            MultiCursorModifier::Alt => modifiers.alt,
            MultiCursorModifier::CmdOrCtrl => modifiers.secondary(),
        }
    }

    pub(crate) fn columnar_selection_mode(
        modifiers: &Modifiers,
        cx: &mut Context<Self>,
    ) -> Option<ColumnarMode> {
        if modifiers.shift && modifiers.number_of_modifiers() == 2 {
            if Self::is_cmd_or_ctrl_pressed(modifiers, cx) {
                Some(ColumnarMode::FromMouse)
            } else if Self::is_alt_pressed(modifiers, cx) {
                Some(ColumnarMode::FromSelection)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub(crate) fn update_selection_mode(
        &mut self,
        modifiers: &Modifiers,
        position_map: &PositionMap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mode) = Self::columnar_selection_mode(modifiers, cx) else {
            return;
        };
        if self.selections.pending_anchor().is_none() {
            return;
        }

        let mouse_position = window.mouse_position();
        let point_for_position = position_map.point_for_position(mouse_position);
        let position = point_for_position.previous_valid;

        self.select(
            SelectPhase::BeginColumnar {
                position,
                reset: false,
                mode,
                goal_column: point_for_position.exact_unclipped.column(),
            },
            window,
            cx,
        );
    }
}
