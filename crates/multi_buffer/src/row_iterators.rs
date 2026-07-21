use super::*;

#[derive(Clone)]
pub struct MultiBufferRows<'a> {
    pub(super) point: Point,
    pub(super) is_empty: bool,
    pub(super) is_singleton: bool,
    pub(super) cursor: MultiBufferCursor<'a, Point, Point>,
}

impl MultiBufferRows<'_> {
    pub fn seek(&mut self, MultiBufferRow(row): MultiBufferRow) {
        self.point = Point::new(row, 0);
        self.cursor.seek(&self.point);
    }
}

impl Iterator for MultiBufferRows<'_> {
    type Item = RowInfo;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_empty && self.point.row == 0 {
            self.point += Point::new(1, 0);
            return Some(RowInfo {
                buffer_id: None,
                buffer_row: Some(0),
                multibuffer_row: Some(MultiBufferRow(0)),
                diff_status: None,
                expand_info: None,
                wrapped_buffer_row: None,
            });
        }

        let mut region = self.cursor.region()?.clone();
        while self.point >= region.range.end {
            self.cursor.next();
            if let Some(next_region) = self.cursor.region() {
                region = next_region.clone();
            } else if self.point == self.cursor.diff_transforms.end().output_dimension.0 {
                let multibuffer_row = MultiBufferRow(self.point.row);
                let last_excerpt = self
                    .cursor
                    .excerpts
                    .item()
                    .or(self.cursor.excerpts.prev_item())?;
                let buffer_snapshot = last_excerpt.buffer_snapshot(self.cursor.snapshot);
                let last_row = last_excerpt.range.context.end.to_point(buffer_snapshot).row;

                let first_row = last_excerpt
                    .range
                    .context
                    .start
                    .to_point(buffer_snapshot)
                    .row;

                let expand_info = if self.is_singleton {
                    None
                } else {
                    let needs_expand_up = first_row == last_row
                        && last_row > 0
                        && !region.diff_hunk_status.is_some_and(|d| d.is_deleted());
                    let needs_expand_down = last_row < buffer_snapshot.max_point().row;

                    if needs_expand_up && needs_expand_down {
                        Some(ExpandExcerptDirection::UpAndDown)
                    } else if needs_expand_up {
                        Some(ExpandExcerptDirection::Up)
                    } else if needs_expand_down {
                        Some(ExpandExcerptDirection::Down)
                    } else {
                        None
                    }
                    .map(|direction| ExpandInfo {
                        direction,
                        start_anchor: Anchor::Excerpt(last_excerpt.start_anchor()),
                    })
                };
                self.point += Point::new(1, 0);
                return Some(RowInfo {
                    buffer_id: Some(last_excerpt.buffer_id),
                    buffer_row: Some(last_row),
                    multibuffer_row: Some(multibuffer_row),
                    diff_status: None,
                    wrapped_buffer_row: None,
                    expand_info,
                });
            } else {
                return None;
            };
        }

        let overshoot = self.point - region.range.start;
        let buffer_point = region.buffer_range.start + overshoot;
        let expand_info = if self.is_singleton {
            None
        } else {
            let needs_expand_up = self.point.row == region.range.start.row
                && self.cursor.is_at_start_of_excerpt()
                && buffer_point.row > 0;
            let needs_expand_down = (region.excerpt.has_trailing_newline
                && self.point.row + 1 == region.range.end.row
                || !region.excerpt.has_trailing_newline && self.point.row == region.range.end.row)
                && self.cursor.is_at_end_of_excerpt()
                && buffer_point.row < region.buffer.max_point().row;

            if needs_expand_up && needs_expand_down {
                Some(ExpandExcerptDirection::UpAndDown)
            } else if needs_expand_up {
                Some(ExpandExcerptDirection::Up)
            } else if needs_expand_down {
                Some(ExpandExcerptDirection::Down)
            } else {
                None
            }
            .map(|direction| ExpandInfo {
                direction,
                start_anchor: Anchor::Excerpt(region.excerpt.start_anchor()),
            })
        };

        let result = Some(RowInfo {
            buffer_id: Some(region.buffer.remote_id()),
            buffer_row: Some(buffer_point.row),
            multibuffer_row: Some(MultiBufferRow(self.point.row)),
            diff_status: region
                .diff_hunk_status
                .filter(|_| self.point < region.range.end),
            expand_info,
            wrapped_buffer_row: None,
        });
        self.point += Point::new(1, 0);
        result
    }
}
