use super::*;

impl SplittableEditor {
    pub fn split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.lhs.is_some() {
            return;
        }
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let project = workspace.read(cx).project().clone();
        let all_paths = self.diff_paths(cx);
        if all_paths.is_empty() && !self.rhs_multibuffer.read(cx).is_empty() {
            return;
        }

        let rhs_has_headers = self.rhs_multibuffer.read(cx).snapshot(cx).show_headers();
        let lhs_multibuffer = cx.new(|cx| {
            let mut multibuffer = if !rhs_has_headers {
                MultiBuffer::without_headers(Capability::ReadOnly)
            } else {
                MultiBuffer::new(Capability::ReadOnly)
            };
            multibuffer.set_all_diff_hunks_expanded(cx);
            multibuffer
        });

        let render_diff_hunk_controls = self.rhs_editor.read(cx).render_diff_hunk_controls.clone();
        let render_diff_hunks_as_unstaged = self.rhs_editor.read(cx).render_diff_hunks_as_unstaged;
        let lhs_editor = cx.new(|cx| {
            let mut editor =
                Editor::for_multibuffer(lhs_multibuffer.clone(), Some(project.clone()), window, cx);
            editor.set_render_diff_hunks_as_unstaged(render_diff_hunks_as_unstaged, cx);
            editor.set_number_deleted_lines(true, cx);
            editor.set_delegate_expand_excerpts(true);
            editor.set_delegate_stage_and_restore(true);
            editor.set_delegate_open_excerpts(true);
            editor.set_show_vertical_scrollbar(false, cx);
            editor.disable_lsp_data();
            editor.disable_runnables();
            editor.disable_diagnostics(cx);
            editor.disable_mouse_wheel_zoom();
            editor.set_minimap_visibility(crate::MinimapVisibility::Disabled, window, cx);
            editor
        });

        lhs_editor.update(cx, |editor, cx| {
            editor.set_render_diff_hunk_controls(render_diff_hunk_controls, cx);
        });

        let mut subscriptions = vec![cx.subscribe_in(
            &lhs_editor,
            window,
            |this, _, event: &EditorEvent, window, cx| match event {
                EditorEvent::ExpandExcerptsRequested {
                    excerpt_anchors,
                    lines,
                    direction,
                } => {
                    if let Some(lhs) = &this.lhs {
                        let rhs_snapshot = this.rhs_multibuffer.read(cx).snapshot(cx);
                        let lhs_snapshot = lhs.multibuffer.read(cx).snapshot(cx);
                        let rhs_anchors = excerpt_anchors
                            .iter()
                            .filter_map(|anchor| {
                                let (anchor, lhs_buffer) =
                                    lhs_snapshot.anchor_to_buffer_anchor(*anchor)?;
                                let diff = lhs_snapshot.diff_for_buffer_id(anchor.buffer_id)?;
                                let rhs_buffer_id = diff.buffer_id();
                                let rhs_buffer = rhs_snapshot.buffer_for_id(rhs_buffer_id)?;
                                let rhs_point = diff.base_text_point_to_buffer_point(
                                    anchor.to_point(&lhs_buffer),
                                    &rhs_buffer,
                                );
                                rhs_snapshot.anchor_in_excerpt(rhs_buffer.anchor_before(rhs_point))
                            })
                            .collect::<Vec<_>>();
                        this.expand_excerpts(rhs_anchors.into_iter(), *lines, *direction, cx);
                    }
                }
                EditorEvent::StageOrUnstageRequested { stage, hunks } => {
                    if this.lhs.is_some() {
                        let translated = translate_lhs_hunks_to_rhs(hunks, this, cx);
                        if !translated.is_empty() {
                            let stage = *stage;
                            this.rhs_editor.update(cx, |editor, cx| {
                                let chunk_by = translated.into_iter().chunk_by(|h| h.buffer_id);
                                for (buffer_id, hunks) in &chunk_by {
                                    editor.do_stage_or_unstage(stage, buffer_id, hunks, cx);
                                }
                            });
                        }
                    }
                }
                EditorEvent::RestoreRequested { hunks } => {
                    if this.lhs.is_some() {
                        let translated = translate_lhs_hunks_to_rhs(hunks, this, cx);
                        if !translated.is_empty() {
                            this.rhs_editor.update(cx, |editor, cx| {
                                editor.restore_diff_hunks(translated, cx);
                            });
                        }
                    }
                }
                EditorEvent::OpenExcerptsRequested {
                    selections_by_buffer,
                    split,
                } => {
                    if this.lhs.is_some() {
                        let translated =
                            translate_lhs_selections_to_rhs(selections_by_buffer, this, cx);
                        if !translated.is_empty() {
                            let workspace = this.workspace.clone();
                            let split = *split;
                            Editor::open_buffers_in_workspace(
                                workspace, translated, split, window, cx,
                            );
                        }
                    }
                }
                _ => cx.emit(event.clone()),
            },
        )];

        subscriptions.push(
            cx.subscribe(&lhs_editor, |this, _, event: &SearchEvent, cx| {
                if this.searched_side == Some(SplitSide::Left) {
                    cx.emit(event.clone());
                }
            }),
        );

        let lhs_focus_handle = lhs_editor.read(cx).focus_handle(cx);
        subscriptions.push(
            cx.on_focus_in(&lhs_focus_handle, window, |this, _window, cx| {
                if let Some(lhs) = &mut this.lhs {
                    if !lhs.was_last_focused {
                        lhs.was_last_focused = true;
                        cx.notify();
                    }
                }
            }),
        );

        let rhs_focus_handle = self.rhs_editor.read(cx).focus_handle(cx);
        subscriptions.push(
            cx.on_focus_in(&rhs_focus_handle, window, |this, _window, cx| {
                if let Some(lhs) = &mut this.lhs {
                    if lhs.was_last_focused {
                        lhs.was_last_focused = false;
                        cx.notify();
                    }
                }
            }),
        );

        let rhs_display_map = self.rhs_editor.read(cx).display_map.clone();
        let lhs_display_map = lhs_editor.read(cx).display_map.clone();
        let rhs_display_map_id = rhs_display_map.entity_id();
        let companion = cx.new(|_| Companion::new(rhs_display_map_id));
        let lhs = LhsEditor {
            editor: lhs_editor,
            multibuffer: lhs_multibuffer,
            was_last_focused: false,
            _subscriptions: subscriptions,
        };

        self.rhs_editor.update(cx, |editor, cx| {
            editor.set_delegate_expand_excerpts(true);
            editor.buffer().update(cx, |rhs_multibuffer, cx| {
                rhs_multibuffer.set_show_deleted_hunks(false, cx);
                rhs_multibuffer.set_use_extended_diff_range(true, cx);
            })
        });

        self.lhs = Some(lhs);

        self.sync_lhs_for_paths(all_paths, cx);

        rhs_display_map.update(cx, |dm, cx| {
            dm.set_companion(Some((lhs_display_map, companion.clone())), cx);
        });

        let lhs = self.lhs.as_ref().unwrap();

        let shared_scroll_anchor = self
            .rhs_editor
            .read(cx)
            .scroll_manager
            .scroll_anchor_entity();
        lhs.editor.update(cx, |editor, _cx| {
            editor
                .scroll_manager
                .set_shared_scroll_anchor(shared_scroll_anchor);
        });

        let this = cx.entity().downgrade();
        self.rhs_editor.update(cx, |editor, _cx| {
            let this = this.clone();
            editor.set_on_local_selections_changed(Some(Box::new(
                move |cursor_position, window, cx| {
                    let this = this.clone();
                    window.defer(cx, move |window, cx| {
                        this.update(cx, |this, cx| {
                            this.sync_cursor_to_other_side(true, cursor_position, window, cx);
                        })
                        .ok();
                    })
                },
            )));
        });
        lhs.editor.update(cx, |editor, _cx| {
            let this = this.clone();
            editor.set_on_local_selections_changed(Some(Box::new(
                move |cursor_position, window, cx| {
                    let this = this.clone();
                    window.defer(cx, move |window, cx| {
                        this.update(cx, |this, cx| {
                            this.sync_cursor_to_other_side(false, cursor_position, window, cx);
                        })
                        .ok();
                    })
                },
            )));
        });

        // Copy soft wrap state from rhs (source of truth) to lhs
        let rhs_soft_wrap_override = self.rhs_editor.read(cx).soft_wrap_mode_override;
        lhs.editor.update(cx, |editor, cx| {
            editor.soft_wrap_mode_override = rhs_soft_wrap_override;
            cx.notify();
        });

        cx.notify();
    }

    fn diff_paths(&self, cx: &App) -> Vec<(PathKey, Entity<BufferDiff>)> {
        let rhs_multibuffer = self.rhs_multibuffer.read(cx);
        let rhs_multibuffer_snapshot = rhs_multibuffer.snapshot(cx);
        rhs_multibuffer_snapshot
            .buffers_with_paths()
            .filter_map(|(buffer, path)| {
                let diff = rhs_multibuffer.diff_for(buffer.remote_id())?;
                Some((path.clone(), diff))
            })
            .collect()
    }

    fn activate_pane_left(
        &mut self,
        _: &ActivatePaneLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(lhs) = &self.lhs {
            if !lhs.was_last_focused {
                lhs.editor.read(cx).focus_handle(cx).focus(window, cx);
                lhs.editor.update(cx, |editor, cx| {
                    editor.request_autoscroll(Autoscroll::fit(), cx);
                });
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    fn activate_pane_right(
        &mut self,
        _: &ActivatePaneRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(lhs) = &self.lhs {
            if lhs.was_last_focused {
                self.rhs_editor.read(cx).focus_handle(cx).focus(window, cx);
                self.rhs_editor.update(cx, |editor, cx| {
                    editor.request_autoscroll(Autoscroll::fit(), cx);
                });
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    fn sync_cursor_to_other_side(
        &mut self,
        from_rhs: bool,
        source_point: Point,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(lhs) = &self.lhs else {
            return;
        };

        let (source_editor, target_editor) = if from_rhs {
            (&self.rhs_editor, &lhs.editor)
        } else {
            (&lhs.editor, &self.rhs_editor)
        };

        let source_snapshot = source_editor.update(cx, |editor, cx| editor.snapshot(window, cx));
        let target_snapshot = target_editor.update(cx, |editor, cx| editor.snapshot(window, cx));

        let display_point = source_snapshot
            .display_snapshot
            .point_to_display_point(source_point, Bias::Right);
        let display_point = target_snapshot.clip_point(display_point, Bias::Right);
        let target_point = target_snapshot.display_point_to_point(display_point, Bias::Right);

        target_editor.update(cx, |editor, cx| {
            editor.set_suppress_selection_callback(true);
            editor.change_selections(crate::SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_ranges([target_point..target_point]);
            });
            editor.set_suppress_selection_callback(false);
        });
    }
}
