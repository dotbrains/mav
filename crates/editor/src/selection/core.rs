use super::*;

impl Editor {
    pub fn sync_selections(
        &mut self,
        other: Entity<Editor>,
        cx: &mut Context<Self>,
    ) -> gpui::Subscription {
        let other_selections = other.read(cx).selections.disjoint_anchors().to_vec();
        if !other_selections.is_empty() {
            self.selections
                .change_with(&self.display_snapshot(cx), |selections| {
                    selections.select_anchors(other_selections);
                });
        }

        let other_subscription = cx.subscribe(&other, |this, other, other_evt, cx| {
            if let EditorEvent::SelectionsChanged { local: true } = other_evt {
                let other_selections = other.read(cx).selections.disjoint_anchors().to_vec();
                if other_selections.is_empty() {
                    return;
                }
                let snapshot = this.display_snapshot(cx);
                this.selections.change_with(&snapshot, |selections| {
                    selections.select_anchors(other_selections);
                });
            }
        });

        let this_subscription = cx.subscribe_self::<EditorEvent>(move |this, this_evt, cx| {
            if let EditorEvent::SelectionsChanged { local: true } = this_evt {
                let these_selections = this.selections.disjoint_anchors().to_vec();
                if these_selections.is_empty() {
                    return;
                }
                other.update(cx, |other_editor, cx| {
                    let snapshot = other_editor.display_snapshot(cx);
                    other_editor
                        .selections
                        .change_with(&snapshot, |selections| {
                            selections.select_anchors(these_selections);
                        })
                });
            }
        });

        Subscription::join(other_subscription, this_subscription)
    }

    /// Changes selections using the provided mutation function. Changes to `self.selections` occur
    /// immediately, but when run within `transact` or `with_selection_effects_deferred` other
    /// effects of selection change occur at the end of the transaction.
    pub fn change_selections<R>(
        &mut self,
        effects: SelectionEffects,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut MutableSelectionsCollection<'_, '_>) -> R,
    ) -> R {
        let snapshot = self.display_snapshot(cx);
        if let Some(state) = &mut self.deferred_selection_effects_state {
            state.effects.scroll = effects.scroll.or(state.effects.scroll);
            state.effects.completions = effects.completions;
            state.effects.nav_history = effects.nav_history.or(state.effects.nav_history);
            let (changed, result) = self.selections.change_with(&snapshot, change);
            state.changed |= changed;
            return result;
        }
        let mut state = DeferredSelectionEffectsState {
            changed: false,
            effects,
            old_cursor_position: self.selections.newest_anchor().head(),
            history_entry: SelectionHistoryEntry {
                selections: self.selections.disjoint_anchors_arc(),
                select_next_state: self.select_next_state.clone(),
                select_prev_state: self.select_prev_state.clone(),
                add_selections_state: self.add_selections_state.clone(),
            },
        };
        let (changed, result) = self.selections.change_with(&snapshot, change);
        state.changed = state.changed || changed;
        if self.defer_selection_effects {
            self.deferred_selection_effects_state = Some(state);
        } else {
            self.apply_selection_effects(state, window, cx);
        }
        result
    }

    /// Defers the effects of selection change, so that the effects of multiple calls to
    /// `change_selections` are applied at the end. This way these intermediate states aren't added
    /// to selection history and the state of popovers based on selection position aren't
    /// erroneously updated.
    pub fn with_selection_effects_deferred<R>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) -> R,
    ) -> R {
        let already_deferred = self.defer_selection_effects;
        self.defer_selection_effects = true;
        let result = update(self, window, cx);
        if !already_deferred {
            self.defer_selection_effects = false;
            if let Some(state) = self.deferred_selection_effects_state.take() {
                self.apply_selection_effects(state, window, cx);
            }
        }
        result
    }

    pub fn has_non_empty_selection(&self, snapshot: &DisplaySnapshot) -> bool {
        self.selections
            .all_adjusted(snapshot)
            .iter()
            .any(|selection| !selection.is_empty())
    }

    pub fn is_range_selected(&mut self, range: &Range<Anchor>, cx: &mut Context<Self>) -> bool {
        if self
            .selections
            .pending_anchor()
            .is_some_and(|pending_selection| {
                let snapshot = self.buffer().read(cx).snapshot(cx);
                pending_selection.range().includes(range, &snapshot)
            })
        {
            return true;
        }

        self.selections
            .disjoint_in_range::<MultiBufferOffset>(range.clone(), &self.display_snapshot(cx))
            .into_iter()
            .any(|selection| {
                // This is needed to cover a corner case, if we just check for an existing
                // selection in the fold range, having a cursor at the start of the fold
                // marks it as selected. Non-empty selections don't cause this.
                let length = selection.end - selection.start;
                length > 0
            })
    }

    pub fn has_pending_nonempty_selection(&self) -> bool {
        let pending_nonempty_selection = match self.selections.pending_anchor() {
            Some(Selection { start, end, .. }) => start != end,
            None => false,
        };

        pending_nonempty_selection
            || (self.columnar_selection_state.is_some()
                && self.selections.disjoint_anchors().len() > 1)
    }

    pub fn has_pending_selection(&self) -> bool {
        self.selections.pending_anchor().is_some() || self.columnar_selection_state.is_some()
    }

    pub fn set_selections_from_remote(
        &mut self,
        selections: Vec<Selection<Anchor>>,
        pending_selection: Option<Selection<Anchor>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_cursor_position = self.selections.newest_anchor().head();
        self.selections
            .change_with(&self.display_snapshot(cx), |s| {
                s.select_anchors(selections);
                if let Some(pending_selection) = pending_selection {
                    s.set_pending(pending_selection, SelectMode::Character);
                } else {
                    s.clear_pending();
                }
            });
        self.selections_did_change(
            false,
            &old_cursor_position,
            SelectionEffects::default(),
            window,
            cx,
        );
    }

    pub fn set_mark(&mut self, _: &actions::SetMark, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection_mark_mode {
            self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.move_with(&mut |_, sel| {
                    sel.collapse_to(sel.head(), SelectionGoal::None);
                });
            })
        }
        self.selection_mark_mode = true;
        cx.notify();
    }

    pub fn swap_selection_ends(
        &mut self,
        _: &actions::SwapSelectionEnds,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.move_with(&mut |_, sel| {
                if sel.start != sel.end {
                    sel.reversed = !sel.reversed
                }
            });
        });
        self.request_autoscroll(Autoscroll::newest(), cx);
        cx.notify();
    }

    pub fn select_to_end(&mut self, _: &SelectToEnd, window: &mut Window, cx: &mut Context<Self>) {
        let buffer = self.buffer.read(cx).snapshot(cx);
        let mut selection = self
            .selections
            .first::<MultiBufferOffset>(&self.display_snapshot(cx));
        selection.set_head(buffer.len(), SelectionGoal::None);
        self.change_selections(Default::default(), window, cx, |s| {
            s.select(vec![selection]);
        });
    }

    pub fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(vec![Anchor::Min..Anchor::Max]);
        });
    }

    pub fn select_line(&mut self, _: &SelectLine, window: &mut Window, cx: &mut Context<Self>) {
        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let mut selections = self.selections.all::<Point>(&display_map);
        let max_point = display_map.buffer_snapshot().max_point();
        for selection in &mut selections {
            let rows = selection.spanned_rows(true, &display_map);
            selection.start = Point::new(rows.start.0, 0);
            selection.end = cmp::min(max_point, Point::new(rows.end.0, 0));
            selection.reversed = false;
        }
        self.change_selections(Default::default(), window, cx, |s| {
            s.select(selections);
        });
    }

    fn selections_did_change(
        &mut self,
        local: bool,
        old_cursor_position: &Anchor,
        effects: SelectionEffects,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.last_selection_from_search = effects.from_search;
        window.invalidate_character_coordinates();

        // Copy selections to primary selection buffer
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        if local {
            let selections = self
                .selections
                .all::<MultiBufferOffset>(&self.display_snapshot(cx));
            let buffer_handle = self.buffer.read(cx).read(cx);

            let mut text = String::new();
            for (index, selection) in selections.iter().enumerate() {
                let text_for_selection = buffer_handle
                    .text_for_range(selection.start..selection.end)
                    .collect::<String>();

                text.push_str(&text_for_selection);
                if index != selections.len() - 1 {
                    text.push('\n');
                }
            }

            if !text.is_empty() {
                cx.write_to_primary(ClipboardItem::new_string(text));
            }
        }

        let selection_anchors = self.selections.disjoint_anchors_arc();

        if self.focus_handle.is_focused(window) && self.leader_id.is_none() {
            self.buffer.update(cx, |buffer, cx| {
                buffer.set_active_selections(
                    &selection_anchors,
                    self.selections.line_mode(),
                    self.cursor_shape,
                    cx,
                )
            });
        }
        let display_map = self
            .display_map
            .update(cx, |display_map, cx| display_map.snapshot(cx));
        let buffer = display_map.buffer_snapshot();
        if self.selections.count() == 1 {
            self.add_selections_state = None;
        }
        self.select_next_state = None;
        self.select_prev_state = None;
        self.select_syntax_node_history.try_clear();
        self.invalidate_autoclose_regions(&selection_anchors, buffer);
        self.snippet_stack.invalidate(&selection_anchors, buffer);
        self.take_rename(false, window, cx);

        let newest_selection = self.selections.newest_anchor();
        let new_cursor_position = newest_selection.head();
        let selection_start = newest_selection.start;

        if effects.nav_history.is_none() || effects.nav_history == Some(true) {
            self.push_to_nav_history(
                *old_cursor_position,
                Some(new_cursor_position.to_point(buffer)),
                false,
                effects.nav_history == Some(true),
                cx,
            );
        }

        if local {
            if let Some((anchor, _)) = buffer.anchor_to_buffer_anchor(new_cursor_position) {
                self.register_buffer(anchor.buffer_id, cx);
            }

            let mut context_menu = self.context_menu.borrow_mut();
            let completion_menu = match context_menu.as_ref() {
                Some(CodeContextMenu::Completions(menu)) => Some(menu),
                Some(CodeContextMenu::CodeActions(_)) => {
                    *context_menu = None;
                    None
                }
                None => None,
            };
            let completion_position = completion_menu.map(|menu| menu.initial_position);
            drop(context_menu);

            if effects.completions
                && let Some(completion_position) = completion_position
            {
                let start_offset = selection_start.to_offset(buffer);
                let position_matches = start_offset == completion_position.to_offset(buffer);
                let continue_showing = if let Some((snap, ..)) =
                    buffer.point_to_buffer_offset(completion_position)
                    && !snap.capability.editable()
                {
                    false
                } else if position_matches {
                    if self.snippet_stack.is_empty() {
                        buffer.char_kind_before(start_offset, Some(CharScopeContext::Completion))
                            == Some(CharKind::Word)
                    } else {
                        // Snippet choices can be shown even when the cursor is in whitespace.
                        // Dismissing the menu with actions like backspace is handled by
                        // invalidation regions.
                        true
                    }
                } else {
                    false
                };

                if continue_showing {
                    self.open_or_update_completions_menu(None, None, false, window, cx);
                } else {
                    self.hide_context_menu(window, cx);
                }
            }

            hide_hover(self, cx);

            self.refresh_code_actions_for_selection(window, cx);
            self.refresh_document_highlights(cx);
            refresh_linked_ranges(self, window, cx);

            self.refresh_selected_text_highlights(&display_map, false, window, cx);
            self.refresh_matching_bracket_highlights(&display_map, cx);
            self.refresh_outline_symbols_at_cursor(cx);
            self.update_visible_edit_prediction(window, cx);
            self.hide_blame_popover(true, cx);
            if self.git_blame_inline_enabled {
                self.start_inline_blame_timer(window, cx);
            }
        }

        self.blink_manager.update(cx, BlinkManager::pause_blinking);

        if local && !self.suppress_selection_callback {
            if let Some(callback) = self.on_local_selections_changed.as_ref() {
                let cursor_position = self.selections.newest::<Point>(&display_map).head();
                callback(cursor_position, window, cx);
            }
        }

        cx.emit(EditorEvent::SelectionsChanged { local });

        let selections = &self.selections.disjoint_anchors_arc();
        if local && let Some(buffer_snapshot) = buffer.as_singleton() {
            let inmemory_selections = selections
                .iter()
                .map(|s| {
                    let start = s.range().start.text_anchor_in(buffer_snapshot);
                    let end = s.range().end.text_anchor_in(buffer_snapshot);
                    (start..end).to_point(buffer_snapshot)
                })
                .collect();
            self.update_restoration_data(cx, |data| {
                data.selections = inmemory_selections;
            });

            if WorkspaceSettings::get(None, cx).restore_on_startup
                != RestoreOnStartupBehavior::EmptyTab
                && let Some(workspace_id) = self.workspace_serialization_id(cx)
            {
                let snapshot = self.buffer().read(cx).snapshot(cx);
                let selections = selections.clone();
                let background_executor = cx.background_executor().clone();
                let editor_id = cx.entity().entity_id().as_u64() as ItemId;
                let db = EditorDb::global(cx);
                self.serialize_selections = cx.background_spawn(async move {
                    background_executor.timer(SERIALIZATION_THROTTLE_TIME).await;
                    let db_selections = selections
                        .iter()
                        .map(|selection| {
                            (
                                selection.start.to_offset(&snapshot).0,
                                selection.end.to_offset(&snapshot).0,
                            )
                        })
                        .collect();

                    db.save_editor_selections(editor_id, workspace_id, db_selections)
                        .await
                        .with_context(|| {
                            format!(
                                "persisting editor selections for editor {editor_id}, \
                                workspace {workspace_id:?}"
                            )
                        })
                        .log_err();
                });
            }
        }

        cx.notify();
    }

    fn apply_selection_effects(
        &mut self,
        state: DeferredSelectionEffectsState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if state.changed {
            self.selection_history.push(state.history_entry);

            if let Some(autoscroll) = state.effects.scroll {
                self.request_autoscroll(autoscroll, cx);
            }

            let old_cursor_position = &state.old_cursor_position;

            self.selections_did_change(true, old_cursor_position, state.effects, window, cx);

            if self.should_open_signature_help_automatically(old_cursor_position, cx) {
                self.show_signature_help_auto(window, cx);
            }
        }
    }
}
