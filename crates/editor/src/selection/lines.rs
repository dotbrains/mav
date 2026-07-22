use super::*;

impl Editor {
    pub fn split_selection_into_lines(
        &mut self,
        action: &SplitSelectionIntoLines,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selections = self
            .selections
            .all::<Point>(&self.display_snapshot(cx))
            .into_iter()
            .map(|selection| selection.start..selection.end)
            .collect::<Vec<_>>();
        self.unfold_ranges(&selections, true, false, cx);

        let mut new_selection_ranges = Vec::new();
        {
            let buffer = self.buffer.read(cx).read(cx);
            for selection in selections {
                for row in selection.start.row..selection.end.row {
                    let line_start = Point::new(row, 0);
                    let line_end = Point::new(row, buffer.line_len(MultiBufferRow(row)));

                    if action.keep_selections {
                        // Keep the selection range for each line
                        let selection_start = if row == selection.start.row {
                            selection.start
                        } else {
                            line_start
                        };
                        new_selection_ranges.push(selection_start..line_end);
                    } else {
                        // Collapse to cursor at end of line
                        new_selection_ranges.push(line_end..line_end);
                    }
                }

                let is_multiline_selection = selection.start.row != selection.end.row;
                // Don't insert last one if it's a multi-line selection ending at the start of a line,
                // so this action feels more ergonomic when paired with other selection operations
                let should_skip_last = is_multiline_selection && selection.end.column == 0;
                if !should_skip_last {
                    if action.keep_selections {
                        if is_multiline_selection {
                            let line_start = Point::new(selection.end.row, 0);
                            new_selection_ranges.push(line_start..selection.end);
                        } else {
                            new_selection_ranges.push(selection.start..selection.end);
                        }
                    } else {
                        new_selection_ranges.push(selection.end..selection.end);
                    }
                }
            }
        }
        self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(new_selection_ranges);
        });
    }
}
