use super::*;

impl Editor {
    pub fn undo_selection(
        &mut self,
        _: &UndoSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry) = self.selection_history.undo_stack.pop_back() {
            self.selection_history.mode = SelectionHistoryMode::Undoing;
            self.with_selection_effects_deferred(window, cx, |this, window, cx| {
                this.end_selection(window, cx);
                this.change_selections(
                    SelectionEffects::scroll(Autoscroll::newest()),
                    window,
                    cx,
                    |s| s.select_anchors(entry.selections.to_vec()),
                );
            });
            self.selection_history.mode = SelectionHistoryMode::Normal;

            self.select_next_state = entry.select_next_state;
            self.select_prev_state = entry.select_prev_state;
            self.add_selections_state = entry.add_selections_state;
        }
    }

    pub fn redo_selection(
        &mut self,
        _: &RedoSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry) = self.selection_history.redo_stack.pop_back() {
            self.selection_history.mode = SelectionHistoryMode::Redoing;
            self.with_selection_effects_deferred(window, cx, |this, window, cx| {
                this.end_selection(window, cx);
                this.change_selections(
                    SelectionEffects::scroll(Autoscroll::newest()),
                    window,
                    cx,
                    |s| s.select_anchors(entry.selections.to_vec()),
                );
            });
            self.selection_history.mode = SelectionHistoryMode::Normal;

            self.select_next_state = entry.select_next_state;
            self.select_prev_state = entry.select_prev_state;
            self.add_selections_state = entry.add_selections_state;
        }
    }

    pub(crate) fn select(
        &mut self,
        phase: SelectPhase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hide_context_menu(window, cx);

        match phase {
            SelectPhase::Begin {
                position,
                add,
                click_count,
            } => self.begin_selection(position, add, click_count, window, cx),
            SelectPhase::BeginColumnar {
                position,
                goal_column,
                reset,
                mode,
            } => self.begin_columnar_selection(position, goal_column, reset, mode, window, cx),
            SelectPhase::Extend {
                position,
                click_count,
            } => self.extend_selection(position, click_count, window, cx),
            SelectPhase::Update {
                position,
                goal_column,
                scroll_delta,
            } => self.update_selection(position, goal_column, scroll_delta, window, cx),
            SelectPhase::End => self.end_selection(window, cx),
        }
    }

    pub(crate) fn extend_selection(
        &mut self,
        position: DisplayPoint,
        click_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let tail = self
            .selections
            .newest::<MultiBufferOffset>(&display_map)
            .tail();
        let click_count = click_count.max(match self.selections.select_mode() {
            SelectMode::Character => 1,
            SelectMode::Word(_) => 2,
            SelectMode::Line(_) => 3,
            SelectMode::All => 4,
        });
        self.begin_selection(position, false, click_count, window, cx);

        let tail_anchor = display_map.buffer_snapshot().anchor_before(tail);

        let current_selection = match self.selections.select_mode() {
            SelectMode::Character | SelectMode::All => tail_anchor..tail_anchor,
            SelectMode::Word(range) | SelectMode::Line(range) => range.clone(),
        };

        let Some((mut pending_selection, mut pending_mode)) = self.pending_selection_and_mode()
        else {
            log::error!("extend_selection dispatched with no pending selection");
            return;
        };

        if pending_selection
            .start
            .cmp(&current_selection.start, display_map.buffer_snapshot())
            == Ordering::Greater
        {
            pending_selection.start = current_selection.start;
        }
        if pending_selection
            .end
            .cmp(&current_selection.end, display_map.buffer_snapshot())
            == Ordering::Less
        {
            pending_selection.end = current_selection.end;
            pending_selection.reversed = true;
        }

        match &mut pending_mode {
            SelectMode::Word(range) | SelectMode::Line(range) => *range = current_selection,
            _ => {}
        }

        let effects = if EditorSettings::get_global(cx).autoscroll_on_clicks {
            SelectionEffects::scroll(Autoscroll::fit())
        } else {
            SelectionEffects::no_scroll()
        };

        self.change_selections(effects, window, cx, |s| {
            s.set_pending(pending_selection.clone(), pending_mode);
            s.set_is_extending(true);
        });
    }

    pub(crate) fn begin_selection(
        &mut self,
        position: DisplayPoint,
        add: bool,
        click_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window) {
            self.last_focused_descendant = None;
            window.focus(&self.focus_handle, cx);
        }

        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let buffer = display_map.buffer_snapshot();
        let position = display_map.clip_point(position, Bias::Left);

        let start;
        let end;
        let mode;
        let mut auto_scroll;
        match click_count {
            1 => {
                start = buffer.anchor_before(position.to_point(&display_map));
                end = start;
                mode = SelectMode::Character;
                auto_scroll = true;
            }
            2 => {
                let position = display_map
                    .clip_point(position, Bias::Left)
                    .to_offset(&display_map, Bias::Left);
                let (range, _) = buffer.surrounding_word(position, None);
                start = buffer.anchor_before(range.start);
                end = buffer.anchor_before(range.end);
                mode = SelectMode::Word(start..end);
                auto_scroll = true;
            }
            3 => {
                let position = display_map
                    .clip_point(position, Bias::Left)
                    .to_point(&display_map);
                let line_start = display_map.prev_line_boundary(position).0;
                let next_line_start = buffer.clip_point(
                    display_map.next_line_boundary(position).0 + Point::new(1, 0),
                    Bias::Left,
                );
                start = buffer.anchor_before(line_start);
                end = buffer.anchor_before(next_line_start);
                mode = SelectMode::Line(start..end);
                auto_scroll = true;
            }
            _ => {
                start = buffer.anchor_before(MultiBufferOffset(0));
                end = buffer.anchor_before(buffer.len());
                mode = SelectMode::All;
                auto_scroll = false;
            }
        }
        auto_scroll &= EditorSettings::get_global(cx).autoscroll_on_clicks;

        let point_to_delete: Option<usize> = {
            let selected_points: Vec<Selection<Point>> =
                self.selections.disjoint_in_range(start..end, &display_map);

            if !add || click_count > 1 {
                None
            } else if !selected_points.is_empty() {
                Some(selected_points[0].id)
            } else {
                let clicked_point_already_selected =
                    self.selections.disjoint_anchors().iter().find(|selection| {
                        selection.start.to_point(buffer) == start.to_point(buffer)
                            || selection.end.to_point(buffer) == end.to_point(buffer)
                    });

                clicked_point_already_selected.map(|selection| selection.id)
            }
        };

        let selections_count = self.selections.count();
        let effects = if auto_scroll {
            SelectionEffects::default()
        } else {
            SelectionEffects::no_scroll()
        };

        self.change_selections(effects, window, cx, |s| {
            if let Some(point_to_delete) = point_to_delete {
                s.delete(point_to_delete);

                if selections_count == 1 {
                    s.set_pending_anchor_range(start..end, mode);
                }
            } else {
                if !add {
                    s.clear_disjoint();
                }

                s.set_pending_anchor_range(start..end, mode);
            }
        });
    }

    pub(crate) fn update_selection(
        &mut self,
        position: DisplayPoint,
        goal_column: u32,
        scroll_delta: gpui::Point<f32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));

        if self.columnar_selection_state.is_some() {
            self.select_columns(position, goal_column, &display_map, window, cx);
        } else if let Some((mut pending, mode)) = self.pending_selection_and_mode() {
            let buffer = display_map.buffer_snapshot();
            let head;
            let tail;
            match &mode {
                SelectMode::Character => {
                    head = position.to_point(&display_map);
                    tail = pending.tail().to_point(buffer);
                }
                SelectMode::Word(original_range) => {
                    let offset = display_map
                        .clip_point(position, Bias::Left)
                        .to_offset(&display_map, Bias::Left);
                    let original_range = original_range.to_offset(buffer);

                    let head_offset = if buffer.is_inside_word(offset, None)
                        || original_range.contains(&offset)
                    {
                        let (word_range, _) = buffer.surrounding_word(offset, None);
                        if word_range.start < original_range.start {
                            word_range.start
                        } else {
                            word_range.end
                        }
                    } else {
                        offset
                    };

                    head = head_offset.to_point(buffer);
                    if head_offset <= original_range.start {
                        tail = original_range.end.to_point(buffer);
                    } else {
                        tail = original_range.start.to_point(buffer);
                    }
                }
                SelectMode::Line(original_range) => {
                    let original_range = original_range.to_point(display_map.buffer_snapshot());

                    let position = display_map
                        .clip_point(position, Bias::Left)
                        .to_point(&display_map);
                    let line_start = display_map.prev_line_boundary(position).0;
                    let next_line_start = buffer.clip_point(
                        display_map.next_line_boundary(position).0 + Point::new(1, 0),
                        Bias::Left,
                    );

                    if line_start < original_range.start {
                        head = line_start
                    } else {
                        head = next_line_start
                    }

                    if head <= original_range.start {
                        tail = original_range.end;
                    } else {
                        tail = original_range.start;
                    }
                }
                SelectMode::All => {
                    return;
                }
            };

            if head < tail {
                pending.start = buffer.anchor_before(head);
                pending.end = buffer.anchor_before(tail);
                pending.reversed = true;
            } else {
                pending.start = buffer.anchor_before(tail);
                pending.end = buffer.anchor_before(head);
                pending.reversed = false;
            }

            self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.set_pending(pending.clone(), mode);
            });
        } else {
            log::error!("update_selection dispatched with no pending selection");
            return;
        }

        self.apply_scroll_delta(scroll_delta, window, cx);
        cx.notify();
    }

    pub(crate) fn end_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.columnar_selection_state.take();
        if let Some(pending_mode) = self.selections.pending_mode() {
            let selections = self
                .selections
                .all::<MultiBufferOffset>(&self.display_snapshot(cx));
            self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select(selections);
                s.clear_pending();
                if s.is_extending() {
                    s.set_is_extending(false);
                } else {
                    s.set_select_mode(pending_mode);
                }
            });
        }
    }
}
