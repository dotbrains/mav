use super::*;

impl Editor {
    pub(crate) fn finish_initialization(
        mut editor: Self,
        full_mode: bool,
        is_minimap: bool,
        multi_buffer: &Entity<MultiBuffer>,
        project_subscriptions: Vec<Subscription>,
        inlay_hint_settings: language_settings::InlayHintSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if is_minimap {
            return editor;
        }

        editor.applicable_language_settings = editor.fetch_applicable_language_settings(cx);
        editor.accent_data = editor.fetch_accent_data(cx);

        if let Some(breakpoints) = editor.breakpoint_store.as_ref() {
            editor
                ._subscriptions
                .push(cx.observe(breakpoints, |_, _, cx| {
                    cx.notify();
                }));
        }
        editor._subscriptions.extend(project_subscriptions);

        editor._subscriptions.push(cx.subscribe_in(
            &cx.entity(),
            window,
            |editor, _, e: &EditorEvent, window, cx| match e {
                EditorEvent::ScrollPositionChanged { local, .. } => {
                    if *local {
                        editor.hide_signature_help(cx, SignatureHelpHiddenBy::Escape);
                        editor.hide_blame_popover(true, cx);
                        let snapshot = editor.snapshot(window, cx);
                        let new_anchor = editor
                            .scroll_manager
                            .native_anchor(&snapshot.display_snapshot, cx);
                        editor.update_restoration_data(cx, move |data| {
                            data.scroll_position = (
                                new_anchor.top_row(snapshot.buffer_snapshot()),
                                new_anchor.offset,
                            );
                        });

                        editor.update_data_on_scroll(true, window, cx);
                    }
                    editor.refresh_sticky_headers(&editor.snapshot(window, cx), cx);
                }
                EditorEvent::Edited { .. } => {
                    let vim_mode = vim_mode_setting::VimModeSetting::try_get(cx)
                        .map(|vim_mode| vim_mode.0)
                        .unwrap_or(false);
                    if !vim_mode {
                        let display_map = editor.display_snapshot(cx);
                        let selections = editor.selections.all_adjusted_display(&display_map);
                        let pop_state = editor
                            .change_list
                            .last()
                            .map(|previous| {
                                previous.len() == selections.len()
                                    && previous.iter().enumerate().all(|(ix, p)| {
                                        p.to_display_point(&display_map).row()
                                            == selections[ix].head().row()
                                    })
                            })
                            .unwrap_or(false);
                        let new_positions = selections
                            .into_iter()
                            .map(|s| display_map.display_point_to_anchor(s.head(), Bias::Left))
                            .collect();
                        editor
                            .change_list
                            .push_to_change_list(pop_state, new_positions);
                    }
                }
                _ => (),
            },
        ));

        if let Some(dap_store) = editor
            .project
            .as_ref()
            .map(|project| project.read(cx).dap_store())
        {
            let weak_editor = cx.weak_entity();

            editor
                ._subscriptions
                .push(
                    cx.observe_new::<project::debugger::session::Session>(move |_, _, cx| {
                        let session_entity = cx.entity();
                        weak_editor
                            .update(cx, |editor, cx| {
                                editor._subscriptions.push(
                                    cx.subscribe(&session_entity, Self::on_debug_session_event),
                                );
                            })
                            .ok();
                    }),
                );

            for session in dap_store.read(cx).sessions().cloned().collect::<Vec<_>>() {
                editor
                    ._subscriptions
                    .push(cx.subscribe(&session, Self::on_debug_session_event));
            }
        }

        // skip adding the initial selection to selection history
        editor.selection_history.mode = SelectionHistoryMode::Skipping;
        editor.end_selection(window, cx);
        editor.selection_history.mode = SelectionHistoryMode::Normal;

        editor.scroll_manager.show_scrollbars(window, cx);
        jsx_tag_auto_close::refresh_enabled_in_any_buffer(&mut editor, multi_buffer, cx);

        if full_mode {
            let should_auto_hide_scrollbars = cx.should_auto_hide_scrollbars();
            cx.set_global(ScrollbarAutoHide(should_auto_hide_scrollbars));

            if editor.git_blame_inline_enabled {
                editor.start_git_blame_inline(false, window, cx);
            }

            editor.go_to_active_debug_line(window, cx);

            editor.minimap =
                editor.create_minimap(EditorSettings::get_global(cx).minimap, window, cx);
            editor.colors = Some(LspColorData::new(cx));
            editor.use_document_folding_ranges = true;
            editor.inlay_hints = Some(LspInlayHintData::new(inlay_hint_settings));
            if editor.enable_code_lens && EditorSettings::get_global(cx).code_lens.inline() {
                editor.code_lens = Some(CodeLensState::default());
            }

            if let Some(buffer) = multi_buffer.read(cx).as_singleton() {
                editor.register_buffer(buffer.read(cx).remote_id(), cx);
            }
            editor.report_editor_event(ReportEditorEvent::EditorOpened, None, cx);
        }

        editor
    }
}
