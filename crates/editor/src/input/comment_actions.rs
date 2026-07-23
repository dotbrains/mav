use super::*;

impl Editor {
    pub fn toggle_block_comments(
        &mut self,
        _: &ToggleBlockComments,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        self.transact(window, cx, |this, _window, cx| {
            let mut selections = this
                .selections
                .all::<MultiBufferPoint>(&this.display_snapshot(cx));
            let mut edits = Vec::new();
            let snapshot = this.buffer.read(cx).read(cx);
            let empty_str: Arc<str> = Arc::default();
            let mut markers_inserted = Vec::new();

            for selection in &mut selections {
                let start_point = selection.start;
                let end_point = selection.end;

                let Some(language) =
                    snapshot.language_scope_at(Point::new(start_point.row, start_point.column))
                else {
                    continue;
                };

                let Some(BlockCommentConfig {
                    start: comment_start,
                    end: comment_end,
                    ..
                }) = language.block_comment()
                else {
                    continue;
                };

                let prefix_needle = comment_start.trim_end().as_bytes();
                let suffix_needle = comment_end.trim_start().as_bytes();

                // Collect full lines spanning the selection as the search region
                let region_start = Point::new(start_point.row, 0);
                let region_end = Point::new(
                    end_point.row,
                    snapshot.line_len(MultiBufferRow(end_point.row)),
                );
                let region_bytes: Vec<u8> = snapshot
                    .bytes_in_range(region_start..region_end)
                    .flatten()
                    .copied()
                    .collect();

                let region_start_offset = snapshot.point_to_offset(region_start);
                let start_byte = snapshot.point_to_offset(start_point) - region_start_offset;
                let end_byte = snapshot.point_to_offset(end_point) - region_start_offset;

                let mut is_commented = false;
                let mut prefix_range = start_point..start_point;
                let mut suffix_range = end_point..end_point;

                // Find rightmost /* at or before the selection end
                if let Some(prefix_pos) = region_bytes[..end_byte.min(region_bytes.len())]
                    .windows(prefix_needle.len())
                    .rposition(|w| w == prefix_needle)
                {
                    let after_prefix = prefix_pos + prefix_needle.len();

                    // Find the first */ after that /*
                    if let Some(suffix_pos) = region_bytes[after_prefix..]
                        .windows(suffix_needle.len())
                        .position(|w| w == suffix_needle)
                        .map(|p| p + after_prefix)
                    {
                        let suffix_end = suffix_pos + suffix_needle.len();

                        // Case 1: /* ... */ surrounds the selection
                        let markers_surround = prefix_pos <= start_byte
                            && suffix_end >= end_byte
                            && start_byte < suffix_end;

                        // Case 2: selection contains /* ... */ (only whitespace padding)
                        let selection_contains = start_byte <= prefix_pos
                            && suffix_end <= end_byte
                            && region_bytes[start_byte..prefix_pos]
                                .iter()
                                .all(|&b| b.is_ascii_whitespace())
                            && region_bytes[suffix_end..end_byte]
                                .iter()
                                .all(|&b| b.is_ascii_whitespace());

                        if markers_surround || selection_contains {
                            is_commented = true;
                            let prefix_pt =
                                snapshot.offset_to_point(region_start_offset + prefix_pos);
                            let suffix_pt =
                                snapshot.offset_to_point(region_start_offset + suffix_pos);
                            prefix_range = prefix_pt
                                ..Point::new(
                                    prefix_pt.row,
                                    prefix_pt.column + prefix_needle.len() as u32,
                                );
                            suffix_range = suffix_pt
                                ..Point::new(
                                    suffix_pt.row,
                                    suffix_pt.column + suffix_needle.len() as u32,
                                );
                        }
                    }
                }

                if is_commented {
                    // Also remove the space after /* and before */
                    if snapshot
                        .bytes_in_range(prefix_range.end..snapshot.max_point())
                        .flatten()
                        .next()
                        == Some(&b' ')
                    {
                        prefix_range.end.column += 1;
                    }
                    if suffix_range.start.column > 0 {
                        let before =
                            Point::new(suffix_range.start.row, suffix_range.start.column - 1);
                        if snapshot
                            .bytes_in_range(before..suffix_range.start)
                            .flatten()
                            .next()
                            == Some(&b' ')
                        {
                            suffix_range.start.column -= 1;
                        }
                    }

                    edits.push((prefix_range, empty_str.clone()));
                    edits.push((suffix_range, empty_str.clone()));
                } else {
                    let prefix: Arc<str> = if comment_start.ends_with(' ') {
                        comment_start.clone()
                    } else {
                        format!("{} ", comment_start).into()
                    };
                    let suffix: Arc<str> = if comment_end.starts_with(' ') {
                        comment_end.clone()
                    } else {
                        format!(" {}", comment_end).into()
                    };

                    edits.push((start_point..start_point, prefix.clone()));
                    edits.push((end_point..end_point, suffix.clone()));
                    markers_inserted.push((
                        selection.id,
                        prefix.len(),
                        suffix.len(),
                        selection.is_empty(),
                        end_point.row,
                    ));
                }
            }

            drop(snapshot);
            this.buffer.update(cx, |buffer, cx| {
                buffer.edit(edits, None, cx);
            });

            let mut selections = this
                .selections
                .all::<MultiBufferPoint>(&this.display_snapshot(cx));
            for selection in &mut selections {
                if let Some((_, prefix_len, suffix_len, was_empty, suffix_row)) = markers_inserted
                    .iter()
                    .find(|(id, _, _, _, _)| *id == selection.id)
                {
                    if *was_empty {
                        selection.start.column = selection
                            .start
                            .column
                            .saturating_sub((*prefix_len + *suffix_len) as u32);
                    } else {
                        selection.start.column =
                            selection.start.column.saturating_sub(*prefix_len as u32);
                        if selection.end.row == *suffix_row {
                            selection.end.column += *suffix_len as u32;
                        }
                    }
                }
            }
            this.change_selections(Default::default(), _window, cx, |s| s.select(selections));
        });
    }

    pub fn toggle_comments(
        &mut self,
        action: &ToggleComments,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        let text_layout_details = &self.text_layout_details(window, cx);
        self.transact(window, cx, |this, window, cx| {
            let mut selections = this
                .selections
                .all::<MultiBufferPoint>(&this.display_snapshot(cx));
            let mut edits = Vec::new();
            let mut selection_edit_ranges = Vec::new();
            let mut last_toggled_row = None;
            let snapshot = this.buffer.read(cx).read(cx);
            let empty_str: Arc<str> = Arc::default();
            let mut suffixes_inserted = Vec::new();
            let ignore_indent = action.ignore_indent;

            pub(crate) fn comment_prefix_range(
                snapshot: &MultiBufferSnapshot,
                row: MultiBufferRow,
                comment_prefix: &str,
                comment_prefix_whitespace: &str,
                ignore_indent: bool,
            ) -> Range<Point> {
                let indent_size = if ignore_indent {
                    0
                } else {
                    snapshot.indent_size_for_line(row).len
                };

                let start = Point::new(row.0, indent_size);

                let mut line_bytes = snapshot
                    .bytes_in_range(start..snapshot.max_point())
                    .flatten()
                    .copied();

                // If this line currently begins with the line comment prefix, then record
                // the range containing the prefix.
                if line_bytes
                    .by_ref()
                    .take(comment_prefix.len())
                    .eq(comment_prefix.bytes())
                {
                    // Include any whitespace that matches the comment prefix.
                    let matching_whitespace_len = line_bytes
                        .zip(comment_prefix_whitespace.bytes())
                        .take_while(|(a, b)| a == b)
                        .count() as u32;
                    let end = Point::new(
                        start.row,
                        start.column + comment_prefix.len() as u32 + matching_whitespace_len,
                    );
                    start..end
                } else {
                    start..start
                }
            }

            pub(crate) fn comment_suffix_range(
                snapshot: &MultiBufferSnapshot,
                row: MultiBufferRow,
                comment_suffix: &str,
                comment_suffix_has_leading_space: bool,
            ) -> Range<Point> {
                let end = Point::new(row.0, snapshot.line_len(row));
                let suffix_start_column = end.column.saturating_sub(comment_suffix.len() as u32);

                let mut line_end_bytes = snapshot
                    .bytes_in_range(Point::new(end.row, suffix_start_column.saturating_sub(1))..end)
                    .flatten()
                    .copied();

                let leading_space_len = if suffix_start_column > 0
                    && line_end_bytes.next() == Some(b' ')
                    && comment_suffix_has_leading_space
                {
                    1
                } else {
                    0
                };

                // If this line currently begins with the line comment prefix, then record
                // the range containing the prefix.
                if line_end_bytes.by_ref().eq(comment_suffix.bytes()) {
                    let start = Point::new(end.row, suffix_start_column - leading_space_len);
                    start..end
                } else {
                    end..end
                }
            }

            // TODO: Handle selections that cross excerpts
            for selection in &mut selections {
                let start_column = snapshot
                    .indent_size_for_line(MultiBufferRow(selection.start.row))
                    .len;
                let language = if let Some(language) =
                    snapshot.language_scope_at(Point::new(selection.start.row, start_column))
                {
                    language
                } else {
                    continue;
                };

                selection_edit_ranges.clear();

                // If multiple selections contain a given row, avoid processing that
                // row more than once.
                let mut start_row = MultiBufferRow(selection.start.row);
                if last_toggled_row == Some(start_row) {
                    start_row = start_row.next_row();
                }
                let end_row =
                    if selection.end.row > selection.start.row && selection.end.column == 0 {
                        MultiBufferRow(selection.end.row - 1)
                    } else {
                        MultiBufferRow(selection.end.row)
                    };
                last_toggled_row = Some(end_row);

                if start_row > end_row {
                    continue;
                }

                // If the language has line comments, toggle those.
                let mut full_comment_prefixes = language.line_comment_prefixes().to_vec();

                // If ignore_indent is set, trim spaces from the right side of all full_comment_prefixes
                if ignore_indent {
                    full_comment_prefixes = full_comment_prefixes
                        .into_iter()
                        .map(|s| Arc::from(s.trim_end()))
                        .collect();
                }

                if !full_comment_prefixes.is_empty() {
                    let first_prefix = full_comment_prefixes
                        .first()
                        .expect("prefixes is non-empty");
                    let prefix_trimmed_lengths = full_comment_prefixes
                        .iter()
                        .map(|p| p.trim_end_matches(' ').len())
                        .collect::<SmallVec<[usize; 4]>>();

                    let mut all_selection_lines_are_comments = true;

                    for row in start_row.0..=end_row.0 {
                        let row = MultiBufferRow(row);
                        if start_row < end_row && snapshot.is_line_blank(row) {
                            continue;
                        }

                        let prefix_range = full_comment_prefixes
                            .iter()
                            .zip(prefix_trimmed_lengths.iter().copied())
                            .map(|(prefix, trimmed_prefix_len)| {
                                comment_prefix_range(
                                    snapshot.deref(),
                                    row,
                                    &prefix[..trimmed_prefix_len],
                                    &prefix[trimmed_prefix_len..],
                                    ignore_indent,
                                )
                            })
                            .max_by_key(|range| range.end.column - range.start.column)
                            .expect("prefixes is non-empty");

                        if prefix_range.is_empty() {
                            all_selection_lines_are_comments = false;
                        }

                        selection_edit_ranges.push(prefix_range);
                    }

                    if all_selection_lines_are_comments {
                        edits.extend(
                            selection_edit_ranges
                                .iter()
                                .cloned()
                                .map(|range| (range, empty_str.clone())),
                        );
                    } else {
                        let min_column = selection_edit_ranges
                            .iter()
                            .map(|range| range.start.column)
                            .min()
                            .unwrap_or(0);
                        edits.extend(selection_edit_ranges.iter().map(|range| {
                            let position = Point::new(range.start.row, min_column);
                            (position..position, first_prefix.clone())
                        }));
                    }
                } else if let Some(BlockCommentConfig {
                    start: full_comment_prefix,
                    end: comment_suffix,
                    ..
                }) = language.block_comment()
                {
                    let comment_prefix = full_comment_prefix.trim_end_matches(' ');
                    let comment_prefix_whitespace = &full_comment_prefix[comment_prefix.len()..];
                    let prefix_range = comment_prefix_range(
                        snapshot.deref(),
                        start_row,
                        comment_prefix,
                        comment_prefix_whitespace,
                        ignore_indent,
                    );
                    let suffix_range = comment_suffix_range(
                        snapshot.deref(),
                        end_row,
                        comment_suffix.trim_start_matches(' '),
                        comment_suffix.starts_with(' '),
                    );

                    if prefix_range.is_empty() || suffix_range.is_empty() {
                        edits.push((
                            prefix_range.start..prefix_range.start,
                            full_comment_prefix.clone(),
                        ));
                        edits.push((suffix_range.end..suffix_range.end, comment_suffix.clone()));
                        suffixes_inserted.push((end_row, comment_suffix.len()));
                    } else {
                        edits.push((prefix_range, empty_str.clone()));
                        edits.push((suffix_range, empty_str.clone()));
                    }
                } else {
                    continue;
                }
            }

            drop(snapshot);
            this.buffer.update(cx, |buffer, cx| {
                buffer.edit(edits, None, cx);
            });

            // Adjust selections so that they end before any comment suffixes that
            // were inserted.
            let mut suffixes_inserted = suffixes_inserted.into_iter().peekable();
            let mut selections = this.selections.all::<Point>(&this.display_snapshot(cx));
            let snapshot = this.buffer.read(cx).read(cx);
            for selection in &mut selections {
                while let Some((row, suffix_len)) = suffixes_inserted.peek().copied() {
                    match row.cmp(&MultiBufferRow(selection.end.row)) {
                        Ordering::Less => {
                            suffixes_inserted.next();
                            continue;
                        }
                        Ordering::Greater => break,
                        Ordering::Equal => {
                            if selection.end.column == snapshot.line_len(row) {
                                if selection.is_empty() {
                                    selection.start.column -= suffix_len as u32;
                                }
                                selection.end.column -= suffix_len as u32;
                            }
                            break;
                        }
                    }
                }
            }

            drop(snapshot);
            this.change_selections(Default::default(), window, cx, |s| s.select(selections));

            let selections = this.selections.all::<Point>(&this.display_snapshot(cx));
            let selections_on_single_row = selections.windows(2).all(|selections| {
                selections[0].start.row == selections[1].start.row
                    && selections[0].end.row == selections[1].end.row
                    && selections[0].start.row == selections[0].end.row
            });
            let selections_selecting = selections
                .iter()
                .any(|selection| selection.start != selection.end);
            let advance_downwards = action.advance_downwards
                && selections_on_single_row
                && !selections_selecting
                && !matches!(this.mode, EditorMode::SingleLine);

            if advance_downwards {
                let snapshot = this.buffer.read(cx).snapshot(cx);

                this.change_selections(Default::default(), window, cx, |s| {
                    s.move_cursors_with(&mut |display_snapshot, display_point, _| {
                        let mut point = display_point.to_point(display_snapshot);
                        point.row += 1;
                        point = snapshot.clip_point(point, Bias::Left);
                        let display_point = point.to_display_point(display_snapshot);
                        let goal = SelectionGoal::HorizontalPosition(
                            display_snapshot
                                .x_for_display_point(display_point, text_layout_details)
                                .into(),
                        );
                        (display_point, goal)
                    })
                });
            }
        });
    }
}
