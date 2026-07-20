use super::*;

impl Editor {
    pub(super) fn active_run_indicators(
        &mut self,
        range: Range<DisplayRow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> HashSet<DisplayRow> {
        let snapshot = self.snapshot(window, cx);

        let offset_range_start =
            snapshot.display_point_to_point(DisplayPoint::new(range.start, 0), Bias::Left);

        let offset_range_end =
            snapshot.display_point_to_point(DisplayPoint::new(range.end, 0), Bias::Right);

        self.runnables
            .all_runnables()
            .filter_map(|tasks| {
                let multibuffer_point = tasks.offset.to_point(&snapshot.buffer_snapshot());
                if multibuffer_point < offset_range_start || multibuffer_point > offset_range_end {
                    return None;
                }
                let multibuffer_row = MultiBufferRow(multibuffer_point.row);
                let buffer_folded = snapshot
                    .buffer_snapshot()
                    .buffer_line_for_row(multibuffer_row)
                    .map(|(buffer_snapshot, _)| buffer_snapshot.remote_id())
                    .map(|buffer_id| self.is_buffer_folded(buffer_id, cx))
                    .unwrap_or(false);
                if buffer_folded {
                    return None;
                }

                if snapshot.is_line_folded(multibuffer_row)
                    && multibuffer_row
                        .0
                        .checked_sub(1)
                        .is_some_and(|previous_row| {
                            snapshot.is_line_folded(MultiBufferRow(previous_row))
                        })
                {
                    return None;
                }

                let display_row = multibuffer_point.to_display_point(&snapshot).row();
                Some(display_row)
            })
            .collect()
    }
}
