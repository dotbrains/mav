use super::*;

impl Editor {
    pub fn edit_prediction_provider(&self) -> Option<Arc<dyn EditPredictionDelegateHandle>> {
        Some(self.edit_prediction_provider.as_ref()?.provider.clone())
    }

    pub(crate) fn preview_edit_prediction_keystroke(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<gpui::KeybindingKeystroke> {
        let key_context = self.key_context_internal(true, window, cx);
        let bindings = window.bindings_for_action_in_context(&AcceptEditPrediction, key_context);
        bindings
            .into_iter()
            .rev()
            .find_map(|binding| match binding.keystrokes() {
                [keystroke, ..] if keystroke.modifiers().modified() => Some(keystroke.clone()),
                _ => None,
            })
    }

    pub(crate) fn edit_prediction_keybind_display(
        &self,
        surface: EditPredictionKeybindSurface,
        window: &mut Window,
        cx: &mut App,
    ) -> EditPredictionKeybindDisplay {
        let accept_keystroke =
            self.accept_edit_prediction_keystroke(EditPredictionGranularity::Full, window, cx);
        let preview_keystroke = self.preview_edit_prediction_keystroke(window, cx);

        let action = match surface {
            EditPredictionKeybindSurface::Inline
            | EditPredictionKeybindSurface::CursorPopoverCompact => {
                if self.edit_prediction_requires_modifier() {
                    EditPredictionKeybindAction::Preview
                } else {
                    EditPredictionKeybindAction::Accept
                }
            }
            EditPredictionKeybindSurface::CursorPopoverExpanded => self
                .active_edit_prediction
                .as_ref()
                .filter(|completion| {
                    self.edit_prediction_cursor_popover_prefers_preview(completion, cx)
                })
                .map_or(EditPredictionKeybindAction::Accept, |_| {
                    EditPredictionKeybindAction::Preview
                }),
        };
        #[cfg(test)]
        let preview_copy = preview_keystroke.clone();
        #[cfg(test)]
        let accept_copy = accept_keystroke.clone();

        let displayed_keystroke = match surface {
            EditPredictionKeybindSurface::Inline => match action {
                EditPredictionKeybindAction::Accept => accept_keystroke,
                EditPredictionKeybindAction::Preview => preview_keystroke,
            },
            EditPredictionKeybindSurface::CursorPopoverCompact
            | EditPredictionKeybindSurface::CursorPopoverExpanded => match action {
                EditPredictionKeybindAction::Accept => accept_keystroke,
                EditPredictionKeybindAction::Preview => {
                    preview_keystroke.or_else(|| accept_keystroke.clone())
                }
            },
        };

        let missing_accept_keystroke = displayed_keystroke.is_none();

        EditPredictionKeybindDisplay {
            #[cfg(test)]
            accept_keystroke: accept_copy,
            #[cfg(test)]
            preview_keystroke: preview_copy,
            displayed_keystroke,
            action,
            missing_accept_keystroke,
            show_hold_label: matches!(surface, EditPredictionKeybindSurface::CursorPopoverCompact)
                && self.edit_prediction_preview.released_too_fast(),
        }
    }

    pub(crate) fn show_edit_predictions_in_menu(&self) -> bool {
        match self.edit_prediction_settings {
            EditPredictionSettings::Disabled => false,
            EditPredictionSettings::Enabled { show_in_menu, .. } => show_in_menu,
        }
    }

    pub(crate) fn edit_prediction_requires_modifier(&self) -> bool {
        match self.edit_prediction_settings {
            EditPredictionSettings::Disabled => false,
            EditPredictionSettings::Enabled {
                preview_requires_modifier,
                ..
            } => preview_requires_modifier,
        }
    }

    pub(crate) fn discard_edit_prediction(
        &mut self,
        reason: EditPredictionDiscardReason,
        cx: &mut Context<Self>,
    ) -> bool {
        if reason == EditPredictionDiscardReason::Rejected {
            let completion_id = self
                .active_edit_prediction
                .as_ref()
                .and_then(|active_completion| active_completion.completion_id.clone());

            self.report_edit_prediction_event(completion_id, false, cx);
        }

        if let Some(provider) = self.edit_prediction_provider() {
            provider.discard(reason, cx);
        }

        self.take_active_edit_prediction(reason == EditPredictionDiscardReason::Ignored, cx)
    }

    pub(crate) fn take_active_edit_prediction(
        &mut self,
        preserve_stale_in_menu: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active_edit_prediction) = self.active_edit_prediction.take() else {
            if !preserve_stale_in_menu {
                self.stale_edit_prediction_in_menu = None;
            }
            return false;
        };

        self.splice_inlays(&active_edit_prediction.inlay_ids, Default::default(), cx);
        self.clear_highlights(HighlightKey::EditPredictionHighlight, cx);
        self.stale_edit_prediction_in_menu =
            preserve_stale_in_menu.then_some(active_edit_prediction);
        true
    }

    pub(crate) fn update_edit_prediction_preview(
        &mut self,
        modifiers: &Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers_held = self.edit_prediction_preview_modifiers_held(modifiers, window, cx);

        if modifiers_held {
            if matches!(
                self.edit_prediction_preview,
                EditPredictionPreview::Inactive { .. }
            ) {
                self.edit_prediction_preview = EditPredictionPreview::Active {
                    previous_scroll_position: None,
                    since: Instant::now(),
                };

                self.update_visible_edit_prediction(window, cx);
                cx.notify();
            }
        } else if let EditPredictionPreview::Active {
            previous_scroll_position,
            since,
        } = self.edit_prediction_preview
        {
            if let (Some(previous_scroll_position), Some(position_map)) =
                (previous_scroll_position, self.last_position_map.as_ref())
            {
                self.set_scroll_position(
                    previous_scroll_position
                        .scroll_position(&position_map.snapshot.display_snapshot),
                    window,
                    cx,
                );
            }

            self.edit_prediction_preview = EditPredictionPreview::Inactive {
                released_too_fast: since.elapsed() < Duration::from_millis(200),
            };
            self.clear_row_highlights::<EditPredictionPreview>();
            self.update_visible_edit_prediction(window, cx);
            cx.notify();
        }
    }
}
