use super::*;

impl Editor {
    pub fn select_enclosing_symbol(
        &mut self,
        _: &SelectEnclosingSymbol,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let buffer = self.buffer.read(cx).snapshot(cx);
        let old_selections = self
            .selections
            .all::<MultiBufferOffset>(&self.display_snapshot(cx))
            .into_boxed_slice();

        fn update_selection(
            selection: &Selection<MultiBufferOffset>,
            buffer_snap: &MultiBufferSnapshot,
        ) -> Option<Selection<MultiBufferOffset>> {
            let cursor = selection.head();
            let (_buffer_id, symbols) = buffer_snap.symbols_containing(cursor, None)?;
            for symbol in symbols.iter().rev() {
                let start = symbol.range.start.to_offset(buffer_snap);
                let end = symbol.range.end.to_offset(buffer_snap);
                let new_range = start..end;
                if start < selection.start || end > selection.end {
                    return Some(Selection {
                        id: selection.id,
                        start: new_range.start,
                        end: new_range.end,
                        goal: SelectionGoal::None,
                        reversed: selection.reversed,
                    });
                }
            }
            None
        }

        let mut selected_larger_symbol = false;
        let new_selections = old_selections
            .iter()
            .map(|selection| match update_selection(selection, &buffer) {
                Some(new_selection) => {
                    if new_selection.range() != selection.range() {
                        selected_larger_symbol = true;
                    }
                    new_selection
                }
                None => selection.clone(),
            })
            .collect::<Vec<_>>();

        if selected_larger_symbol {
            self.change_selections(Default::default(), window, cx, |s| {
                s.select(new_selections);
            });
        }
    }

    pub fn select_larger_syntax_node(
        &mut self,
        _: &SelectLargerSyntaxNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(visible_row_count) = self.visible_row_count() else {
            return;
        };
        let old_selections: Box<[_]> = self
            .selections
            .all::<MultiBufferOffset>(&self.display_snapshot(cx))
            .into();
        if old_selections.is_empty() {
            return;
        }

        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let buffer = self.buffer.read(cx).snapshot(cx);

        let mut selected_larger_node = false;
        let mut new_selections = old_selections
            .iter()
            .map(|selection| {
                let old_range = selection.start..selection.end;

                if let Some((node, _)) = buffer.syntax_ancestor(old_range.clone()) {
                    // manually select word at selection
                    if ["string_content", "inline"].contains(&node.kind()) {
                        let (word_range, _) = buffer.surrounding_word(old_range.start, None);
                        // ignore if word is already selected
                        if !word_range.is_empty() && old_range != word_range {
                            let (last_word_range, _) = buffer.surrounding_word(old_range.end, None);
                            // only select word if start and end point belongs to same word
                            if word_range == last_word_range {
                                selected_larger_node = true;
                                return Selection {
                                    id: selection.id,
                                    start: word_range.start,
                                    end: word_range.end,
                                    goal: SelectionGoal::None,
                                    reversed: selection.reversed,
                                };
                            }
                        }
                    }
                }

                let mut new_range = old_range.clone();
                while let Some((node, range)) = buffer.syntax_ancestor(new_range.clone()) {
                    new_range = range;
                    if !node.is_named() {
                        continue;
                    }
                    if !display_map.intersects_fold(new_range.start)
                        && !display_map.intersects_fold(new_range.end)
                    {
                        break;
                    }
                }

                selected_larger_node |= new_range != old_range;
                Selection {
                    id: selection.id,
                    start: new_range.start,
                    end: new_range.end,
                    goal: SelectionGoal::None,
                    reversed: selection.reversed,
                }
            })
            .collect::<Vec<_>>();

        if !selected_larger_node {
            return; // don't put this call in the history
        }

        // scroll based on transformation done to the last selection created by the user
        let (last_old, last_new) = old_selections
            .last()
            .zip(new_selections.last().cloned())
            .expect("old_selections isn't empty");

        let is_selection_reversed = if new_selections.len() == 1 {
            let should_be_reversed = last_old.start != last_new.start;
            new_selections.last_mut().expect("checked above").reversed = should_be_reversed;
            should_be_reversed
        } else {
            last_new.reversed
        };

        self.select_syntax_node_history.disable_clearing = true;
        self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select(new_selections.clone());
        });
        self.select_syntax_node_history.disable_clearing = false;

        let start_row = last_new.start.to_display_point(&display_map).row().0;
        let end_row = last_new.end.to_display_point(&display_map).row().0;
        let selection_height = end_row - start_row + 1;
        let scroll_margin_rows = self.vertical_scroll_margin() as u32;

        let fits_on_the_screen = visible_row_count >= selection_height + scroll_margin_rows * 2;
        let scroll_behavior = if fits_on_the_screen {
            self.request_autoscroll(Autoscroll::fit(), cx);
            SelectSyntaxNodeScrollBehavior::FitSelection
        } else if is_selection_reversed {
            self.scroll_cursor_top(&ScrollCursorTop, window, cx);
            SelectSyntaxNodeScrollBehavior::CursorTop
        } else {
            self.scroll_cursor_bottom(&ScrollCursorBottom, window, cx);
            SelectSyntaxNodeScrollBehavior::CursorBottom
        };

        let old_selections: Box<[Selection<Anchor>]> = old_selections
            .iter()
            .map(|s| s.map(|offset| buffer.anchor_before(offset)))
            .collect();
        self.select_syntax_node_history.push((
            old_selections,
            scroll_behavior,
            is_selection_reversed,
        ));
    }

    pub fn select_smaller_syntax_node(
        &mut self,
        _: &SelectSmallerSyntaxNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((mut selections, scroll_behavior, is_selection_reversed)) =
            self.select_syntax_node_history.pop()
        {
            if let Some(selection) = selections.last_mut() {
                selection.reversed = is_selection_reversed;
            }

            let snapshot = self.buffer.read(cx).snapshot(cx);
            let selections: Vec<Selection<MultiBufferOffset>> = selections
                .iter()
                .map(|s| s.map(|anchor| anchor.to_offset(&snapshot)))
                .collect();

            self.select_syntax_node_history.disable_clearing = true;
            self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select(selections);
            });
            self.select_syntax_node_history.disable_clearing = false;

            match scroll_behavior {
                SelectSyntaxNodeScrollBehavior::CursorTop => {
                    self.scroll_cursor_top(&ScrollCursorTop, window, cx);
                }
                SelectSyntaxNodeScrollBehavior::FitSelection => {
                    self.request_autoscroll(Autoscroll::fit(), cx);
                }
                SelectSyntaxNodeScrollBehavior::CursorBottom => {
                    self.scroll_cursor_bottom(&ScrollCursorBottom, window, cx);
                }
            }
        }
    }

    pub fn select_next_syntax_node(
        &mut self,
        _: &SelectNextSyntaxNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_selections = self.selections.all_anchors(&self.display_snapshot(cx));
        if old_selections.is_empty() {
            return;
        }

        let buffer = self.buffer.read(cx).snapshot(cx);
        let mut selected_sibling = false;

        let new_selections = old_selections
            .iter()
            .map(|selection| {
                let old_range =
                    selection.start.to_offset(&buffer)..selection.end.to_offset(&buffer);
                if let Some(results) = buffer.map_excerpt_ranges(
                    old_range,
                    |buf, _excerpt_range, input_buffer_range| {
                        let Some(node) = buf.syntax_next_sibling(input_buffer_range) else {
                            return Vec::new();
                        };
                        vec![(
                            BufferOffset(node.byte_range().start)
                                ..BufferOffset(node.byte_range().end),
                            (),
                        )]
                    },
                ) && let [(new_range, _)] = results.as_slice()
                {
                    selected_sibling = true;
                    let new_range =
                        buffer.anchor_after(new_range.start)..buffer.anchor_before(new_range.end);
                    Selection {
                        id: selection.id,
                        start: new_range.start,
                        end: new_range.end,
                        goal: SelectionGoal::None,
                        reversed: selection.reversed,
                    }
                } else {
                    selection.clone()
                }
            })
            .collect::<Vec<_>>();

        if selected_sibling {
            self.change_selections(
                SelectionEffects::scroll(Autoscroll::fit()),
                window,
                cx,
                |s| {
                    s.select(new_selections);
                },
            );
        }
    }

    pub fn select_prev_syntax_node(
        &mut self,
        _: &SelectPreviousSyntaxNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_selections: Arc<[_]> = self.selections.all_anchors(&self.display_snapshot(cx));

        let multibuffer_snapshot = self.buffer.read(cx).snapshot(cx);
        let mut selected_sibling = false;

        let new_selections = old_selections
            .iter()
            .map(|selection| {
                let old_range = selection.start.to_offset(&multibuffer_snapshot)
                    ..selection.end.to_offset(&multibuffer_snapshot);
                if let Some(results) = multibuffer_snapshot.map_excerpt_ranges(
                    old_range,
                    |buf, _excerpt_range, input_buffer_range| {
                        let Some(node) = buf.syntax_prev_sibling(input_buffer_range) else {
                            return Vec::new();
                        };
                        vec![(
                            BufferOffset(node.byte_range().start)
                                ..BufferOffset(node.byte_range().end),
                            (),
                        )]
                    },
                ) && let [(new_range, _)] = results.as_slice()
                {
                    selected_sibling = true;
                    let new_range = multibuffer_snapshot.anchor_after(new_range.start)
                        ..multibuffer_snapshot.anchor_before(new_range.end);
                    Selection {
                        id: selection.id,
                        start: new_range.start,
                        end: new_range.end,
                        goal: SelectionGoal::None,
                        reversed: selection.reversed,
                    }
                } else {
                    selection.clone()
                }
            })
            .collect::<Vec<_>>();

        if selected_sibling {
            self.change_selections(
                SelectionEffects::scroll(Autoscroll::fit()),
                window,
                cx,
                |s| {
                    s.select(new_selections);
                },
            );
        }
    }

    pub fn move_to_start_of_larger_syntax_node(
        &mut self,
        _: &MoveToStartOfLargerSyntaxNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_cursors_to_syntax_nodes(window, cx, false);
    }

    pub fn move_to_end_of_larger_syntax_node(
        &mut self,
        _: &MoveToEndOfLargerSyntaxNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_cursors_to_syntax_nodes(window, cx, true);
    }

    pub fn select_to_start_of_larger_syntax_node(
        &mut self,
        _: &SelectToStartOfLargerSyntaxNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to_syntax_nodes(window, cx, false);
    }

    pub fn select_to_end_of_larger_syntax_node(
        &mut self,
        _: &SelectToEndOfLargerSyntaxNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to_syntax_nodes(window, cx, true);
    }
}
