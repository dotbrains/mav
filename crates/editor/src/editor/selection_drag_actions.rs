use super::*;

impl Editor {
    pub fn move_selection_on_drop(
        &mut self,
        selection: &Selection<Anchor>,
        target: DisplayPoint,
        is_cut: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let buffer = display_map.buffer_snapshot();
        let mut edits = Vec::new();
        let insert_point = display_map
            .clip_point(target, Bias::Left)
            .to_point(&display_map);
        let text = buffer
            .text_for_range(selection.start..selection.end)
            .collect::<String>();
        if is_cut {
            edits.push(((selection.start..selection.end), String::new()));
        }
        let insert_anchor = buffer.anchor_before(insert_point);
        edits.push(((insert_anchor..insert_anchor), text));
        let last_edit_start = insert_anchor.bias_left(buffer);
        let last_edit_end = insert_anchor.bias_right(buffer);
        self.transact(window, cx, |this, window, cx| {
            this.buffer.update(cx, |buffer, cx| {
                buffer.edit(edits, None, cx);
            });
            this.change_selections(Default::default(), window, cx, |s| {
                s.select_anchor_ranges([last_edit_start..last_edit_end]);
            });
        });
    }

    pub fn clear_selection_drag_state(&mut self) {
        self.selection_drag_state = SelectionDragState::None;
    }
}
