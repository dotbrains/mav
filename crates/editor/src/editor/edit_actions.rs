use super::*;

impl Editor {
    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.transact(window, cx, |this, window, cx| {
            this.select_all(&SelectAll, window, cx);
            this.insert("", window, cx);
        });
    }

    pub fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only(cx) {
            return;
        }
        self.transact(window, cx, |this, window, cx| {
            this.select_autoclose_pair(window, cx);

            let linked_edits = this.linked_edits_for_selections(Arc::from(""), cx);

            let display_map = this.display_map.update(cx, |map, cx| map.snapshot(cx));
            let mut selections = this.selections.all::<MultiBufferPoint>(&display_map);
            for selection in &mut selections {
                if selection.is_empty() {
                    let old_head = selection.head();
                    let mut new_head =
                        movement::left(&display_map, old_head.to_display_point(&display_map))
                            .to_point(&display_map);
                    if let Some((buffer, line_buffer_range)) = display_map
                        .buffer_snapshot()
                        .buffer_line_for_row(MultiBufferRow(old_head.row))
                    {
                        let indent_size = buffer.indent_size_for_line(line_buffer_range.start.row);
                        let indent_len = match indent_size.kind {
                            IndentKind::Space => {
                                buffer.settings_at(line_buffer_range.start, cx).tab_size
                            }
                            IndentKind::Tab => NonZeroU32::new(1).unwrap(),
                        };
                        if old_head.column <= indent_size.len && old_head.column > 0 {
                            let indent_len = indent_len.get();
                            new_head = cmp::min(
                                new_head,
                                MultiBufferPoint::new(
                                    old_head.row,
                                    ((old_head.column - 1) / indent_len) * indent_len,
                                ),
                            );
                        }
                    }

                    selection.set_head(new_head, SelectionGoal::None);
                }
            }

            this.change_selections(Default::default(), window, cx, |s| s.select(selections));
            this.insert("", window, cx);
            linked_edits.apply_with_left_expansion(cx);
            this.refresh_edit_prediction(
                true,
                false,
                EditPredictionRequestTrigger::BufferEdit,
                window,
                cx,
            );
            refresh_linked_ranges(this, window, cx);
        });
    }

    pub fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only(cx) {
            return;
        }
        self.transact(window, cx, |this, window, cx| {
            this.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |map, selection| {
                    if selection.is_empty() {
                        let cursor = movement::right(map, selection.head());
                        selection.end = cursor;
                        selection.reversed = true;
                        selection.goal = SelectionGoal::None;
                    }
                })
            });
            let linked_edits = this.linked_edits_for_selections(Arc::from(""), cx);
            this.insert("", window, cx);
            linked_edits.apply(cx);
            this.refresh_edit_prediction(
                true,
                false,
                EditPredictionRequestTrigger::BufferEdit,
                window,
                cx,
            );
            refresh_linked_ranges(this, window, cx);
        });
    }
}
