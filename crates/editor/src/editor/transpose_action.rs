use super::*;

impl Editor {
    pub fn transpose(&mut self, _: &Transpose, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only(cx) {
            return;
        }
        let text_layout_details = &self.text_layout_details(window, cx);
        self.transact(window, cx, |this, window, cx| {
            let edits = this.change_selections(Default::default(), window, cx, |s| {
                let mut edits: Vec<(Range<MultiBufferOffset>, String)> = Default::default();
                s.move_with(&mut |display_map, selection| {
                    if !selection.is_empty() {
                        return;
                    }

                    let mut head = selection.head();
                    let mut transpose_offset = head.to_offset(display_map, Bias::Right);
                    if head.column() == display_map.line_len(head.row()) {
                        transpose_offset = display_map
                            .buffer_snapshot()
                            .clip_offset(transpose_offset.saturating_sub_usize(1), Bias::Left);
                    }

                    if transpose_offset == MultiBufferOffset(0) {
                        return;
                    }

                    *head.column_mut() += 1;
                    head = display_map.clip_point(head, Bias::Right);
                    let goal = SelectionGoal::HorizontalPosition(
                        display_map
                            .x_for_display_point(head, text_layout_details)
                            .into(),
                    );
                    selection.collapse_to(head, goal);

                    let transpose_start = display_map
                        .buffer_snapshot()
                        .clip_offset(transpose_offset.saturating_sub_usize(1), Bias::Left);
                    if edits.last().is_none_or(|e| e.0.end <= transpose_start) {
                        let transpose_end = display_map
                            .buffer_snapshot()
                            .clip_offset(transpose_offset + 1usize, Bias::Right);
                        if let Some(ch) = display_map
                            .buffer_snapshot()
                            .chars_at(transpose_start)
                            .next()
                        {
                            edits.push((transpose_start..transpose_offset, String::new()));
                            edits.push((transpose_end..transpose_end, ch.to_string()));
                        }
                    }
                });
                edits
            });
            this.buffer
                .update(cx, |buffer, cx| buffer.edit(edits, None, cx));
            let selections = this
                .selections
                .all::<MultiBufferOffset>(&this.display_snapshot(cx));
            this.change_selections(Default::default(), window, cx, |s| {
                s.select(selections);
            });
        });
    }
}
