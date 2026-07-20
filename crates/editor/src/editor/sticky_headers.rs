use super::*;

impl Editor {
    pub fn refresh_sticky_headers(
        &mut self,
        display_snapshot: &DisplaySnapshot,
        cx: &mut Context<Editor>,
    ) {
        if !self.mode.is_full() {
            return;
        }
        let multi_buffer = display_snapshot.buffer_snapshot().clone();
        let scroll_anchor = self
            .scroll_manager
            .native_anchor(display_snapshot, cx)
            .anchor;
        let Some(buffer_snapshot) = multi_buffer.as_singleton() else {
            return;
        };

        let buffer = buffer_snapshot.clone();
        let Some((buffer_visible_start, _)) = multi_buffer.anchor_to_buffer_anchor(scroll_anchor)
        else {
            return;
        };
        let buffer_visible_start = buffer_visible_start.to_point(&buffer);
        let max_row = buffer.max_point().row;
        let start_row = buffer_visible_start.row.min(max_row);
        let end_row = (buffer_visible_start.row + 10).min(max_row);

        let syntax = self.style(cx).syntax.clone();
        let background_task = cx.background_spawn(async move {
            buffer
                .outline_items_containing(
                    Point::new(start_row, 0)..Point::new(end_row, 0),
                    true,
                    Some(syntax.as_ref()),
                )
                .into_iter()
                .filter_map(|outline_item| {
                    Some(OutlineItem {
                        depth: outline_item.depth,
                        range: multi_buffer
                            .buffer_anchor_range_to_anchor_range(outline_item.range)?,
                        selection_range: multi_buffer
                            .buffer_anchor_range_to_anchor_range(outline_item.selection_range)?,
                        source_range_for_text: multi_buffer.buffer_anchor_range_to_anchor_range(
                            outline_item.source_range_for_text,
                        )?,
                        text: outline_item.text,
                        highlight_ranges: outline_item.highlight_ranges,
                        name_ranges: outline_item.name_ranges,
                        body_range: outline_item.body_range.and_then(|range| {
                            multi_buffer.buffer_anchor_range_to_anchor_range(range)
                        }),
                        annotation_range: outline_item.annotation_range.and_then(|range| {
                            multi_buffer.buffer_anchor_range_to_anchor_range(range)
                        }),
                    })
                })
                .collect()
        });
        self.sticky_headers_task = cx.spawn(async move |this, cx| {
            let sticky_headers = background_task.await;
            this.update(cx, |this, cx| {
                if this.sticky_headers.as_ref() != Some(&sticky_headers) {
                    this.sticky_headers = Some(sticky_headers);
                    cx.notify();
                }
            })
            .ok();
        });
    }
}
