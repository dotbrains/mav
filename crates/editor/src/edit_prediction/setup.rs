use super::*;
impl Editor {
    pub fn set_edit_prediction_provider<T>(
        &mut self,
        provider: Option<Entity<T>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) where
        T: EditPredictionDelegate,
    {
        self.edit_prediction_provider = provider.map(|provider| RegisteredEditPredictionDelegate {
            _subscription: cx.observe_in(&provider, window, |this, _, window, cx| {
                if this.focus_handle.is_focused(window) {
                    this.update_visible_edit_prediction(window, cx);
                }
            }),
            provider: Arc::new(provider),
        });
        self.update_edit_prediction_settings(cx);
        self.refresh_edit_prediction(
            false,
            false,
            EditPredictionRequestTrigger::Other,
            window,
            cx,
        );
    }

    pub fn set_edit_predictions_hidden_for_vim_mode(
        &mut self,
        hidden: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if hidden != self.edit_predictions_hidden_for_vim_mode {
            self.edit_predictions_hidden_for_vim_mode = hidden;
            if hidden {
                self.update_visible_edit_prediction(window, cx);
            } else {
                self.refresh_edit_prediction(
                    true,
                    false,
                    EditPredictionRequestTrigger::Other,
                    window,
                    cx,
                );
            }
        }
    }

    pub fn toggle_edit_predictions(
        &mut self,
        _: &ToggleEditPrediction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.show_edit_predictions_override.is_some() {
            self.set_show_edit_predictions(None, window, cx);
        } else {
            let show_edit_predictions = !self.edit_predictions_enabled();
            self.set_show_edit_predictions(Some(show_edit_predictions), window, cx);
        }
    }

    pub fn set_show_edit_predictions(
        &mut self,
        show_edit_predictions: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_edit_predictions_override = show_edit_predictions;
        self.update_edit_prediction_settings(cx);

        if let Some(false) = show_edit_predictions {
            self.discard_edit_prediction(EditPredictionDiscardReason::Ignored, cx);
        } else {
            self.refresh_edit_prediction(
                false,
                true,
                EditPredictionRequestTrigger::Explicit,
                window,
                cx,
            );
        }
    }

    pub fn refresh_edit_prediction(
        &mut self,
        debounce: bool,
        user_requested: bool,
        trigger: EditPredictionRequestTrigger,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        if self.leader_id.is_some() {
            self.discard_edit_prediction(EditPredictionDiscardReason::Ignored, cx);
            return None;
        }

        let cursor = self.selections.newest_anchor().head();
        let (buffer, cursor_buffer_position) =
            self.buffer.read(cx).text_anchor_for_position(cursor, cx)?;

        if DisableAiSettings::is_ai_disabled_for_buffer(Some(&buffer), cx) {
            return None;
        }

        if !self.edit_predictions_enabled_in_buffer(&buffer, cursor_buffer_position, cx) {
            self.discard_edit_prediction(EditPredictionDiscardReason::Ignored, cx);
            return None;
        }

        self.update_visible_edit_prediction(window, cx);

        if !user_requested
            && (!self.should_show_edit_predictions()
                || !self.is_focused(window)
                || buffer.read(cx).is_empty())
        {
            self.discard_edit_prediction(EditPredictionDiscardReason::Ignored, cx);
            return None;
        }

        self.edit_prediction_provider()?.refresh(
            buffer,
            cursor_buffer_position,
            debounce,
            trigger,
            cx,
        );
        Some(())
    }

    pub fn edit_predictions_enabled(&self) -> bool {
        match self.edit_prediction_settings {
            EditPredictionSettings::Disabled => false,
            EditPredictionSettings::Enabled { .. } => true,
        }
    }

    pub fn update_edit_prediction_settings(&mut self, cx: &mut Context<Self>) {
        if self.edit_prediction_provider.is_none() {
            self.edit_prediction_settings = EditPredictionSettings::Disabled;
            self.discard_edit_prediction(EditPredictionDiscardReason::Ignored, cx);
            return;
        }

        let selection = self.selections.newest_anchor();
        let cursor = selection.head();

        if let Some((buffer, cursor_buffer_position)) =
            self.buffer.read(cx).text_anchor_for_position(cursor, cx)
        {
            if DisableAiSettings::is_ai_disabled_for_buffer(Some(&buffer), cx) {
                self.edit_prediction_settings = EditPredictionSettings::Disabled;
                self.discard_edit_prediction(EditPredictionDiscardReason::Ignored, cx);
                return;
            }
            self.edit_prediction_settings =
                self.edit_prediction_settings_at_position(&buffer, cursor_buffer_position, cx);
        }
    }

    pub fn edit_prediction_preview_is_active(&self) -> bool {
        matches!(
            self.edit_prediction_preview,
            EditPredictionPreview::Active { .. }
        )
    }

    pub fn edit_predictions_enabled_at_cursor(&self, cx: &App) -> bool {
        let cursor = self.selections.newest_anchor().head();
        if let Some((buffer, cursor_position)) =
            self.buffer.read(cx).text_anchor_for_position(cursor, cx)
        {
            self.edit_predictions_enabled_in_buffer(&buffer, cursor_position, cx)
        } else {
            false
        }
    }

    pub fn show_edit_prediction(
        &mut self,
        _: &ShowEditPrediction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.has_active_edit_prediction() {
            self.refresh_edit_prediction(
                false,
                true,
                EditPredictionRequestTrigger::Explicit,
                window,
                cx,
            );
            return;
        }

        self.update_visible_edit_prediction(window, cx);
    }
}
