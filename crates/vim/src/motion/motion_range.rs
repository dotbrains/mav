use super::*;

impl Motion {
    // Get the range value after self is applied to the specified selection.
    pub fn range(
        &self,
        map: &DisplaySnapshot,
        mut selection: Selection<DisplayPoint>,
        times: Option<usize>,
        text_layout_details: &TextLayoutDetails,
        forced_motion: bool,
    ) -> Option<(Range<DisplayPoint>, MotionKind)> {
        if let Motion::MavSearchResult {
            prior_selections,
            new_selections,
        } = self
        {
            if let Some((prior_selection, new_selection)) =
                prior_selections.first().zip(new_selections.first())
            {
                let start = prior_selection
                    .start
                    .to_display_point(map)
                    .min(new_selection.start.to_display_point(map));
                let end = new_selection
                    .end
                    .to_display_point(map)
                    .max(prior_selection.end.to_display_point(map));

                if start < end {
                    return Some((start..end, MotionKind::Exclusive));
                } else {
                    return Some((end..start, MotionKind::Exclusive));
                }
            } else {
                return None;
            }
        }
        let maybe_new_point = self.move_point(
            map,
            selection.head(),
            selection.goal,
            times,
            text_layout_details,
        );

        let (new_head, goal) = match (maybe_new_point, forced_motion) {
            (Some((p, g)), _) => Some((p, g)),
            (None, false) => None,
            (None, true) => Some((selection.head(), selection.goal)),
        }?;

        selection.set_head(new_head, goal);

        let mut kind = match (self.default_kind(), forced_motion) {
            (MotionKind::Linewise, true) => MotionKind::Exclusive,
            (MotionKind::Exclusive, true) => MotionKind::Inclusive,
            (MotionKind::Inclusive, true) => MotionKind::Exclusive,
            (kind, false) => kind,
        };

        if let Motion::NextWordStart {
            ignore_punctuation: _,
        } = self
        {
            // Another special case: When using the "w" motion in combination with an
            // operator and the last word moved over is at the end of a line, the end of
            // that word becomes the end of the operated text, not the first word in the
            // next line.
            let start = selection.start.to_point(map);
            let end = selection.end.to_point(map);
            let start_row = MultiBufferRow(selection.start.to_point(map).row);
            if end.row > start.row {
                selection.end = Point::new(start_row.0, map.buffer_snapshot().line_len(start_row))
                    .to_display_point(map);

                // a bit of a hack, we need `cw` on a blank line to not delete the newline,
                // but dw on a blank line should. The `Linewise` returned from this method
                // causes the `d` operator to include the trailing newline.
                if selection.start == selection.end {
                    return Some((selection.start..selection.end, MotionKind::Linewise));
                }
            }
        } else if kind == MotionKind::Exclusive && !self.skip_exclusive_special_case() {
            let start_point = selection.start.to_point(map);
            let mut end_point = selection.end.to_point(map);
            let mut next_point = selection.end;
            *next_point.column_mut() += 1;
            next_point = map.clip_point(next_point, Bias::Right);
            if next_point.to_point(map) == end_point && forced_motion {
                selection.end = movement::saturating_left(map, selection.end);
            }

            if end_point.row > start_point.row {
                let first_non_blank_of_start_row = map
                    .line_indent_for_buffer_row(MultiBufferRow(start_point.row))
                    .raw_len();
                // https://github.com/neovim/neovim/blob/ee143aaf65a0e662c42c636aa4a959682858b3e7/src/nvim/ops.c#L6178-L6203
                if end_point.column == 0 {
                    // If the motion is exclusive and the end of the motion is in column 1, the
                    // end of the motion is moved to the end of the previous line and the motion
                    // becomes inclusive. Example: "}" moves to the first line after a paragraph,
                    // but "d}" will not include that line.
                    //
                    // If the motion is exclusive, the end of the motion is in column 1 and the
                    // start of the motion was at or before the first non-blank in the line, the
                    // motion becomes linewise.  Example: If a paragraph begins with some blanks
                    // and you do "d}" while standing on the first non-blank, all the lines of
                    // the paragraph are deleted, including the blanks.
                    if start_point.column <= first_non_blank_of_start_row {
                        kind = MotionKind::Linewise;
                    } else {
                        kind = MotionKind::Inclusive;
                    }
                    end_point.row -= 1;
                    end_point.column = 0;
                    selection.end = map.clip_point(map.next_line_boundary(end_point).1, Bias::Left);
                } else if let Motion::EndOfParagraph = self {
                    // Special case: When using the "}" motion, it's possible
                    // that there's no blank lines after the paragraph the
                    // cursor is currently on.
                    // In this situation the `end_point.column` value will be
                    // greater than 0, so the selection doesn't actually end on
                    // the first character of a blank line. In that case, we'll
                    // want to move one column to the right, to actually include
                    // all characters of the last non-blank line.
                    selection.end = movement::saturating_right(map, selection.end)
                }
            }
        } else if kind == MotionKind::Inclusive {
            selection.end = movement::saturating_right(map, selection.end)
        }

        if kind == MotionKind::Linewise {
            selection.start = map.prev_line_boundary(selection.start.to_point(map)).1;
            selection.end = map.next_line_boundary(selection.end.to_point(map)).1;
        }
        Some((selection.start..selection.end, kind))
    }

    // Expands a selection using self for an operator
    pub fn expand_selection(
        &self,
        map: &DisplaySnapshot,
        selection: &mut Selection<DisplayPoint>,
        times: Option<usize>,
        text_layout_details: &TextLayoutDetails,
        forced_motion: bool,
    ) -> Option<MotionKind> {
        let (range, kind) = self.range(
            map,
            selection.clone(),
            times,
            text_layout_details,
            forced_motion,
        )?;
        selection.start = range.start;
        selection.end = range.end;
        Some(kind)
    }
}
