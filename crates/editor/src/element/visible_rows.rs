use super::*;

impl EditorElement {
    pub(super) fn visible_rows(
        bounds: Bounds<Pixels>,
        line_height: Pixels,
        scroll_position: gpui::Point<ScrollOffset>,
        snapshot: &EditorSnapshot,
        window: &Window,
    ) -> layout_data::VisibleRows {
        // Calculate how much of the editor is clipped by parent containers (e.g., List).
        // This allows us to only render lines that are actually visible, which is
        // critical for performance when large content-sized editors are inside Lists.
        let visible_bounds = window.content_mask().bounds;
        let visible_top = bounds.top().max(visible_bounds.top());
        let visible_bottom = bounds.bottom().min(visible_bounds.bottom());
        let clipped_top = (visible_top - bounds.top()).max(px(0.));
        let visible_height = (visible_bottom - visible_top).max(px(0.));
        let clipped_top_in_lines = f64::from(clipped_top / line_height);
        let visible_height_in_lines = f64::from(visible_height / line_height);

        // The scroll position is a fractional point, the whole number of which represents
        // the top of the window in terms of display rows.
        // We add clipped_top_in_lines to skip rows that are clipped by parent containers,
        // but we don't modify scroll_position itself since the parent handles positioning.
        let max_row = snapshot.max_point().row();
        let start_row = cmp::min(
            DisplayRow((scroll_position.y + clipped_top_in_lines).floor() as u32),
            max_row,
        );
        let end_row = DisplayRow(cmp::min(
            (scroll_position.y + clipped_top_in_lines + visible_height_in_lines).ceil() as u32,
            max_row.next_row().0,
        ));

        let row_infos = snapshot
            .row_infos(start_row)
            .take((start_row..end_row).len())
            .collect::<Vec<RowInfo>>();

        let start_anchor = if start_row == Default::default() {
            Anchor::Min
        } else {
            snapshot
                .buffer_snapshot()
                .anchor_before(DisplayPoint::new(start_row, 0).to_offset(snapshot, Bias::Left))
        };
        let end_anchor = if end_row > max_row {
            Anchor::Max
        } else {
            snapshot
                .buffer_snapshot()
                .anchor_before(DisplayPoint::new(end_row, 0).to_offset(snapshot, Bias::Right))
        };

        layout_data::VisibleRows {
            max_row,
            start_row,
            end_row,
            row_infos,
            start_anchor,
            end_anchor,
        }
    }
}
