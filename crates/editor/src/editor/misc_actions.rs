use super::*;

impl Editor {
    pub(crate) fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    pub fn open_selections_in_multibuffer(
        &mut self,
        _: &OpenSelectionsInMultibuffer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let multibuffer = self.buffer.read(cx);

        let Some(buffer) = multibuffer.as_singleton() else {
            return;
        };
        let buffer_snapshot = buffer.read(cx).snapshot();

        let Some(workspace) = self.workspace() else {
            return;
        };

        let title = multibuffer.title(cx).to_string();

        let locations = self
            .selections
            .all_anchors(&self.display_snapshot(cx))
            .iter()
            .map(|selection| {
                (
                    buffer.clone(),
                    (selection.start.text_anchor_in(&buffer_snapshot)
                        ..selection.end.text_anchor_in(&buffer_snapshot))
                        .to_point(buffer.read(cx)),
                )
            })
            .into_group_map();

        cx.spawn_in(window, async move |_, cx| {
            workspace.update_in(cx, |workspace, window, cx| {
                Self::open_locations_in_multibuffer(
                    workspace,
                    locations,
                    format!("Selections for '{title}'"),
                    false,
                    false,
                    MultibufferSelectionMode::All,
                    window,
                    cx,
                );
            })
        })
        .detach();
    }

    pub(crate) fn selection_replacement_ranges(
        &self,
        range: Range<MultiBufferOffsetUtf16>,
        cx: &mut App,
    ) -> Vec<Range<MultiBufferOffsetUtf16>> {
        let selections = self
            .selections
            .all::<MultiBufferOffsetUtf16>(&self.display_snapshot(cx));
        let newest_selection = selections
            .iter()
            .max_by_key(|selection| selection.id)
            .unwrap();
        let start_delta = range.start.0.0 as isize - newest_selection.start.0.0 as isize;
        let end_delta = range.end.0.0 as isize - newest_selection.end.0.0 as isize;
        let snapshot = self.buffer.read(cx).read(cx);
        selections
            .into_iter()
            .map(|mut selection| {
                selection.start.0.0 =
                    (selection.start.0.0 as isize).saturating_add(start_delta) as usize;
                selection.end.0.0 = (selection.end.0.0 as isize).saturating_add(end_delta) as usize;
                snapshot.clip_offset_utf16(selection.start, Bias::Left)
                    ..snapshot.clip_offset_utf16(selection.end, Bias::Right)
            })
            .collect()
    }

    pub(crate) fn report_editor_event(
        &self,
        reported_event: ReportEditorEvent,
        file_extension: Option<String>,
        cx: &App,
    ) {
        if cfg!(any(test, feature = "test-support")) {
            return;
        }

        let Some(project) = &self.project else { return };

        // If None, we are in a file without an extension
        let file = self
            .buffer
            .read(cx)
            .as_singleton()
            .and_then(|b| b.read(cx).file());
        let file_extension = file_extension.or(file
            .as_ref()
            .and_then(|file| Path::new(file.file_name(cx)).extension())
            .and_then(|e| e.to_str())
            .map(|a| a.to_string()));

        let vim_mode = vim_mode_setting::VimModeSetting::try_get(cx)
            .map(|vim_mode| vim_mode.0)
            .unwrap_or(false);

        let edit_predictions_provider = all_language_settings(file, cx).edit_predictions.provider;
        let copilot_enabled = edit_predictions_provider
            == language::language_settings::EditPredictionProvider::Copilot;
        let copilot_enabled_for_language = self
            .buffer
            .read(cx)
            .language_settings(cx)
            .show_edit_predictions;

        let project = project.read(cx);
        let event_type = reported_event.event_type();

        if let ReportEditorEvent::Saved { auto_saved } = reported_event {
            telemetry::event!(
                event_type,
                type = if auto_saved {"autosave"} else {"manual"},
                file_extension,
                vim_mode,
                copilot_enabled,
                copilot_enabled_for_language,
                edit_predictions_provider,
                is_via_ssh = project.is_via_remote_server(),
            );
        } else {
            telemetry::event!(
                event_type,
                file_extension,
                vim_mode,
                copilot_enabled,
                copilot_enabled_for_language,
                edit_predictions_provider,
                is_via_ssh = project.is_via_remote_server(),
            );
        };
    }
}
