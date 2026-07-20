use super::*;

pub(crate) struct PositionMap {
    pub size: Size<Pixels>,
    pub line_height: Pixels,
    pub scroll_position: gpui::Point<ScrollOffset>,
    pub scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
    pub scroll_max: gpui::Point<ScrollOffset>,
    pub em_advance: Pixels,
    pub em_layout_width: Pixels,
    pub visible_row_range: Range<DisplayRow>,
    pub line_layouts: Vec<LineWithInvisibles>,
    pub snapshot: EditorSnapshot,
    pub text_align: TextAlign,
    pub content_width: Pixels,
    pub text_hitbox: Hitbox,
    pub gutter_hitbox: Hitbox,
    pub inline_blame_bounds: Option<(Bounds<Pixels>, BufferId, BlameEntry)>,
    pub display_hunks: Vec<(DisplayDiffHunk, Option<Hitbox>)>,
    pub diff_hunk_control_bounds: Vec<(DisplayRow, Bounds<Pixels>)>,
}

#[derive(Debug, Copy, Clone)]
pub struct PointForPosition {
    pub previous_valid: DisplayPoint,
    pub next_valid: DisplayPoint,
    pub nearest_valid: DisplayPoint,
    pub exact_unclipped: DisplayPoint,
    pub column_overshoot_after_line_end: u32,
}

impl PointForPosition {
    pub fn as_valid(&self) -> Option<DisplayPoint> {
        if self.previous_valid == self.exact_unclipped && self.next_valid == self.exact_unclipped {
            Some(self.previous_valid)
        } else {
            None
        }
    }

    pub fn intersects_selection(&self, selection: &Selection<DisplayPoint>) -> bool {
        let Some(valid_point) = self.as_valid() else {
            return false;
        };
        let range = selection.range();

        let candidate_row = valid_point.row();
        let candidate_col = valid_point.column();

        let start_row = range.start.row();
        let start_col = range.start.column();
        let end_row = range.end.row();
        let end_col = range.end.column();

        if candidate_row < start_row || candidate_row > end_row {
            false
        } else if start_row == end_row {
            candidate_col >= start_col && candidate_col < end_col
        } else if candidate_row == start_row {
            candidate_col >= start_col
        } else if candidate_row == end_row {
            candidate_col < end_col
        } else {
            true
        }
    }
}

impl PositionMap {
    pub(crate) fn point_for_position(&self, position: gpui::Point<Pixels>) -> PointForPosition {
        let text_bounds = self.text_hitbox.bounds;
        let scroll_position = self.scroll_position;
        let position = position - text_bounds.origin;
        let y = position.y.max(px(0.)).min(self.size.height);
        let x = position.x + (scroll_position.x as f32 * self.em_layout_width);
        let row = ((y / self.line_height) as f64 + scroll_position.y) as u32;

        let (column, x_overshoot_after_line_end) = if let Some(line_index) =
            row.checked_sub(self.visible_row_range.start.0)
            && let Some(line) = self.line_layouts.get(line_index as usize)
        {
            let alignment_offset = line.alignment_offset(self.text_align, self.content_width);
            let x_relative_to_text = x - alignment_offset;
            if let Some(ix) = line.index_for_x(x_relative_to_text) {
                (ix as u32, px(0.))
            } else {
                (line.len as u32, px(0.).max(x_relative_to_text - line.width))
            }
        } else {
            (0, x)
        };

        let mut exact_unclipped = DisplayPoint::new(DisplayRow(row), column);
        let previous_valid = self.snapshot.clip_point(exact_unclipped, Bias::Left);
        let next_valid = self.snapshot.clip_point(exact_unclipped, Bias::Right);

        let nearest_valid = if previous_valid == next_valid {
            previous_valid
        } else {
            match self.snapshot.inlay_bias_at(exact_unclipped) {
                Some(Bias::Left) => next_valid,
                Some(Bias::Right) => previous_valid,
                None => previous_valid,
            }
        };

        let column_overshoot_after_line_end =
            (x_overshoot_after_line_end / self.em_layout_width) as u32;
        *exact_unclipped.column_mut() += column_overshoot_after_line_end;
        PointForPosition {
            previous_valid,
            next_valid,
            nearest_valid,
            exact_unclipped,
            column_overshoot_after_line_end,
        }
    }

    pub(super) fn point_for_position_on_line(
        &self,
        position: gpui::Point<Pixels>,
        row: DisplayRow,
        line: &LineWithInvisibles,
    ) -> PointForPosition {
        let text_bounds = self.text_hitbox.bounds;
        let scroll_position = self.scroll_position;
        let position = position - text_bounds.origin;
        let x = position.x + (scroll_position.x as f32 * self.em_layout_width);

        let alignment_offset = line.alignment_offset(self.text_align, self.content_width);
        let x_relative_to_text = x - alignment_offset;
        let (column, x_overshoot_after_line_end) =
            if let Some(ix) = line.index_for_x(x_relative_to_text) {
                (ix as u32, px(0.))
            } else {
                (line.len as u32, px(0.).max(x_relative_to_text - line.width))
            };

        let mut exact_unclipped = DisplayPoint::new(row, column);
        let previous_valid = self.snapshot.clip_point(exact_unclipped, Bias::Left);
        let next_valid = self.snapshot.clip_point(exact_unclipped, Bias::Right);

        let nearest_valid = if previous_valid == next_valid {
            previous_valid
        } else {
            match self.snapshot.inlay_bias_at(exact_unclipped) {
                Some(Bias::Left) => next_valid,
                Some(Bias::Right) => previous_valid,
                None => previous_valid,
            }
        };

        let column_overshoot_after_line_end =
            (x_overshoot_after_line_end / self.em_layout_width) as u32;
        *exact_unclipped.column_mut() += column_overshoot_after_line_end;
        PointForPosition {
            previous_valid,
            next_valid,
            nearest_valid,
            exact_unclipped,
            column_overshoot_after_line_end,
        }
    }
}
