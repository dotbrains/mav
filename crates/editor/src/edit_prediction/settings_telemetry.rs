use super::*;

impl Editor {
    fn accept_edit_prediction_keystroke(
        &self,
        granularity: EditPredictionGranularity,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<gpui::KeybindingKeystroke> {
        let key_context = self.key_context_internal(true, window, cx);

        let bindings =
            match granularity {
                EditPredictionGranularity::Word => window
                    .bindings_for_action_in_context(&AcceptNextWordEditPrediction, key_context),
                EditPredictionGranularity::Line => window
                    .bindings_for_action_in_context(&AcceptNextLineEditPrediction, key_context),
                EditPredictionGranularity::Full => {
                    window.bindings_for_action_in_context(&AcceptEditPrediction, key_context)
                }
            };

        bindings
            .into_iter()
            .rev()
            .find_map(|binding| match binding.keystrokes() {
                [keystroke, ..] => Some(keystroke.clone()),
                _ => None,
            })
    }

    fn edit_prediction_preview_modifiers_held(
        &self,
        modifiers: &Modifiers,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let can_supersede_active_menu =
            self.context_menu.borrow().as_ref().is_none_or(|menu| {
                !menu.visible() || matches!(menu, CodeContextMenu::Completions(_))
            });

        if !can_supersede_active_menu {
            return false;
        }

        let key_context = self.key_context_internal(true, window, cx);
        let actions: [&dyn Action; 3] = [
            &AcceptEditPrediction,
            &AcceptNextWordEditPrediction,
            &AcceptNextLineEditPrediction,
        ];

        actions.into_iter().any(|action| {
            window
                .bindings_for_action_in_context(action, key_context.clone())
                .into_iter()
                .rev()
                .any(|binding| {
                    binding.keystrokes().first().is_some_and(|keystroke| {
                        keystroke.modifiers().modified() && keystroke.modifiers() == modifiers
                    })
                })
        })
    }

    fn edit_prediction_cursor_popover_prefers_preview(
        &self,
        completion: &EditPredictionState,
        cx: &App,
    ) -> bool {
        let multibuffer_snapshot = self.buffer.read(cx).snapshot(cx);

        match &completion.completion {
            EditPrediction::Edit {
                edits, snapshot, ..
            } => {
                let mut start_row: Option<u32> = None;
                let mut end_row: Option<u32> = None;

                for (range, text) in edits {
                    let Some((_, range)) =
                        multibuffer_snapshot.anchor_range_to_buffer_anchor_range(range.clone())
                    else {
                        continue;
                    };
                    let edit_start_row = range.start.to_point(snapshot).row;
                    let old_end_row = range.end.to_point(snapshot).row;
                    let inserted_newline_count = text
                        .as_ref()
                        .chars()
                        .filter(|character| *character == '\n')
                        .count() as u32;
                    let deleted_newline_count = old_end_row - edit_start_row;
                    let preview_end_row = edit_start_row + inserted_newline_count;

                    start_row =
                        Some(start_row.map_or(edit_start_row, |row| row.min(edit_start_row)));
                    end_row = Some(end_row.map_or(preview_end_row, |row| row.max(preview_end_row)));

                    if deleted_newline_count > 1 {
                        end_row = Some(end_row.map_or(old_end_row, |row| row.max(old_end_row)));
                    }
                }

                start_row
                    .zip(end_row)
                    .is_some_and(|(start_row, end_row)| end_row > start_row)
            }
            EditPrediction::MoveWithin { .. } | EditPrediction::MoveOutside { .. } => false,
        }
    }

    fn edit_predictions_disabled_in_scope(
        &self,
        buffer: &Entity<Buffer>,
        buffer_position: language::Anchor,
        cx: &App,
    ) -> bool {
        let snapshot = buffer.read(cx).snapshot();
        let settings = snapshot.settings_at(buffer_position, cx);

        let Some(scope) = snapshot.language_scope_at(buffer_position) else {
            return false;
        };

        scope.override_name().is_some_and(|scope_name| {
            settings
                .edit_predictions_disabled_in
                .iter()
                .any(|s| s == scope_name)
        })
    }

    fn edit_prediction_settings_at_position(
        &self,
        buffer: &Entity<Buffer>,
        buffer_position: language::Anchor,
        cx: &App,
    ) -> EditPredictionSettings {
        if !self.mode.is_full()
            || !self.show_edit_predictions_override.unwrap_or(true)
            || self.edit_predictions_disabled_in_scope(buffer, buffer_position, cx)
        {
            return EditPredictionSettings::Disabled;
        }

        if !LanguageSettings::for_buffer(&buffer.read(cx), cx).show_edit_predictions {
            return EditPredictionSettings::Disabled;
        };

        let by_provider = matches!(
            self.menu_edit_predictions_policy,
            MenuEditPredictionsPolicy::ByProvider
        );

        let show_in_menu = by_provider
            && self
                .edit_prediction_provider
                .as_ref()
                .is_some_and(|provider| provider.provider.show_predictions_in_menu());

        let file = buffer.read(cx).file();
        let preview_requires_modifier =
            all_language_settings(file, cx).edit_predictions_mode() == EditPredictionsMode::Subtle;

        EditPredictionSettings::Enabled {
            show_in_menu,
            preview_requires_modifier,
        }
    }

    fn should_show_edit_predictions(&self) -> bool {
        self.snippet_stack.is_empty() && self.edit_predictions_enabled()
    }

    fn edit_predictions_enabled_in_buffer(
        &self,
        buffer: &Entity<Buffer>,
        buffer_position: language::Anchor,
        cx: &App,
    ) -> bool {
        maybe!({
            if self.read_only(cx) || self.leader_id.is_some() {
                return Some(false);
            }
            let provider = self.edit_prediction_provider()?;
            if !provider.is_enabled(buffer, buffer_position, cx) {
                return Some(false);
            }
            let buffer = buffer.read(cx);
            let Some(file) = buffer.file() else {
                return Some(true);
            };
            let settings = all_language_settings(Some(file), cx);
            Some(settings.edit_predictions_enabled_for_file(file, cx))
        })
        .unwrap_or(false)
    }

    fn report_edit_prediction_event(&self, id: Option<SharedString>, accepted: bool, cx: &App) {
        let Some(provider) = self.edit_prediction_provider() else {
            return;
        };

        let buffer_snapshot = self.buffer.read(cx).snapshot(cx);
        let Some((position, _)) =
            buffer_snapshot.anchor_to_buffer_anchor(self.selections.newest_anchor().head())
        else {
            return;
        };
        let Some(buffer) = self.buffer.read(cx).buffer(position.buffer_id) else {
            return;
        };

        let extension = buffer
            .read(cx)
            .file()
            .and_then(|file| Some(file.path().extension()?.to_string()));

        let event_type = match accepted {
            true => "Edit Prediction Accepted",
            false => "Edit Prediction Discarded",
        };
        telemetry::event!(
            event_type,
            provider = provider.name(),
            prediction_id = id,
            suggestion_accepted = accepted,
            file_extension = extension,
        );
    }

    fn open_editor_at_anchor(
        snapshot: &language::BufferSnapshot,
        target: language::Anchor,
        workspace: &Entity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        workspace.update(cx, |workspace, cx| {
            let path = snapshot.file().map(|file| file.full_path(cx));
            let Some(path) =
                path.and_then(|path| workspace.project().read(cx).find_project_path(path, cx))
            else {
                return Task::ready(Err(anyhow::anyhow!("Project path not found")));
            };
            let target = text::ToPoint::to_point(&target, snapshot);
            let item = workspace.open_path(path, None, true, window, cx);
            window.spawn(cx, async move |cx| {
                let Some(editor) = item.await?.downcast::<Editor>() else {
                    return Ok(());
                };
                editor
                    .update_in(cx, |editor, window, cx| {
                        editor.go_to_singleton_buffer_point(target, window, cx);
                    })
                    .ok();
                anyhow::Ok(())
            })
        })
    }

    const EDIT_PREDICTION_POPOVER_PADDING_X: Pixels = px(24.);

    const EDIT_PREDICTION_POPOVER_PADDING_Y: Pixels = px(2.);
}
