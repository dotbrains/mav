use super::*;

impl DisplaySnapshot {
    pub fn sticky_header_excerpt(&self, row: f64) -> Option<StickyHeaderExcerpt<'_>> {
        self.block_snapshot.sticky_header_excerpt(row)
    }

    pub fn block_for_id(&self, id: BlockId) -> Option<Block> {
        self.block_snapshot.block_for_id(id)
    }

    pub fn intersects_fold<T: ToOffset>(&self, offset: T) -> bool {
        self.fold_snapshot().intersects_fold(offset)
    }

    pub fn is_line_folded(&self, buffer_row: MultiBufferRow) -> bool {
        self.block_snapshot.is_line_replaced(buffer_row)
            || self.fold_snapshot().is_line_folded(buffer_row)
    }

    pub fn is_block_line(&self, display_row: DisplayRow) -> bool {
        self.block_snapshot.is_block_line(BlockRow(display_row.0))
    }

    pub fn is_folded_buffer_header(&self, display_row: DisplayRow) -> bool {
        self.block_snapshot
            .is_folded_buffer_header(BlockRow(display_row.0))
    }

    pub fn soft_wrap_indent(&self, display_row: DisplayRow) -> Option<u32> {
        let wrap_row = self
            .block_snapshot
            .to_wrap_point(BlockPoint::new(BlockRow(display_row.0), 0), Bias::Left)
            .row();
        self.wrap_snapshot().soft_wrap_indent(wrap_row)
    }

    pub fn text(&self) -> String {
        self.text_chunks(DisplayRow(0)).collect()
    }

    pub fn line(&self, display_row: DisplayRow) -> String {
        let mut result = String::new();
        for chunk in self.text_chunks(display_row) {
            if let Some(ix) = chunk.find('\n') {
                result.push_str(&chunk[0..ix]);
                break;
            } else {
                result.push_str(chunk);
            }
        }
        result
    }

    pub fn line_indent_for_buffer_row(&self, buffer_row: MultiBufferRow) -> LineIndent {
        self.buffer_snapshot().line_indent_for_row(buffer_row)
    }

    pub fn line_len(&self, row: DisplayRow) -> u32 {
        self.block_snapshot.line_len(BlockRow(row.0))
    }

    pub fn longest_row(&self) -> DisplayRow {
        DisplayRow(self.block_snapshot.longest_row().0)
    }

    pub fn longest_row_in_range(&self, range: Range<DisplayRow>) -> DisplayRow {
        let block_range = BlockRow(range.start.0)..BlockRow(range.end.0);
        let longest_row = self.block_snapshot.longest_row_in_range(block_range);
        DisplayRow(longest_row.0)
    }

    pub fn starts_indent(&self, buffer_row: MultiBufferRow) -> bool {
        let max_row = self.buffer_snapshot().max_row();
        if buffer_row >= max_row {
            return false;
        }

        let line_indent = self.line_indent_for_buffer_row(buffer_row);
        if line_indent.is_line_blank() {
            return false;
        }

        (buffer_row.0 + 1..=max_row.0)
            .find_map(|next_row| {
                let next_line_indent = self.line_indent_for_buffer_row(MultiBufferRow(next_row));
                if next_line_indent.raw_len() > line_indent.raw_len() {
                    Some(true)
                } else if !next_line_indent.is_line_blank() {
                    Some(false)
                } else {
                    None
                }
            })
            .unwrap_or(false)
    }

    /// Returns the indent length of `row` if it starts with a closing bracket.
    fn closing_bracket_indent_len(&self, row: u32) -> Option<u32> {
        let snapshot = self.buffer_snapshot();
        let indent_len = self
            .line_indent_for_buffer_row(MultiBufferRow(row))
            .raw_len();
        let content_start = Point::new(row, indent_len);
        let line_text: String = snapshot
            .chars_at(content_start)
            .take_while(|ch| *ch != '\n')
            .collect();

        let scope = snapshot.language_scope_at(Point::new(row, 0))?;
        if scope
            .brackets()
            .any(|(pair, _)| line_text.starts_with(&pair.end))
        {
            return Some(indent_len);
        }

        None
    }

    #[instrument(skip_all)]
    pub fn crease_for_buffer_row(&self, buffer_row: MultiBufferRow) -> Option<Crease<Point>> {
        let start =
            MultiBufferPoint::new(buffer_row.0, self.buffer_snapshot().line_len(buffer_row));
        if let Some(crease) = self
            .crease_snapshot
            .query_row(buffer_row, self.buffer_snapshot())
        {
            match crease {
                Crease::Inline {
                    range,
                    placeholder,
                    render_toggle,
                    render_trailer,
                    metadata,
                } => Some(Crease::Inline {
                    range: range.to_point(self.buffer_snapshot()),
                    placeholder: placeholder.clone(),
                    render_toggle: render_toggle.clone(),
                    render_trailer: render_trailer.clone(),
                    metadata: metadata.clone(),
                }),
                Crease::Block {
                    range,
                    block_height,
                    block_style,
                    render_block,
                    block_priority,
                    render_toggle,
                } => Some(Crease::Block {
                    range: range.to_point(self.buffer_snapshot()),
                    block_height: *block_height,
                    block_style: *block_style,
                    render_block: render_block.clone(),
                    block_priority: *block_priority,
                    render_toggle: render_toggle.clone(),
                }),
            }
        } else if !self.use_lsp_folding_ranges
            && self.starts_indent(MultiBufferRow(start.row))
            && !self.is_line_folded(MultiBufferRow(start.row))
        {
            let start_line_indent = self.line_indent_for_buffer_row(buffer_row);
            let snapshot = self.buffer_snapshot();
            let max_point = snapshot.max_point();
            let mut closing_row = None;

            // End byte of the smallest syntactic node enclosing `buffer_row`.
            // Used to tell standalone top-level comments (which terminate the
            // fold) apart from unindented content inside a multi-line string
            // or block comment belonging to the folded node (which does not).
            let foldable_node_end = {
                let row_start = Point::new(buffer_row.0, 0);
                let row_end = Point::new(buffer_row.0, snapshot.line_len(buffer_row));
                snapshot
                    .syntax_ancestor(row_start..row_end)
                    .map(|(_, range)| range.end)
            };

            for row in (buffer_row.0 + 1)..=max_point.row {
                let line_indent = self.line_indent_for_buffer_row(MultiBufferRow(row));
                if !line_indent.is_line_blank()
                    && line_indent.raw_len() <= start_line_indent.raw_len()
                {
                    let in_string_or_comment_scope = snapshot
                        .language_scope_at(Point::new(row, 0))
                        .is_some_and(|scope| {
                            matches!(
                                scope.override_name(),
                                Some("string") | Some("comment") | Some("comment.inclusive")
                            )
                        });
                    if in_string_or_comment_scope
                        && let Some(end) = foldable_node_end
                        && Point::new(row, 0).to_offset(snapshot) < end
                    {
                        continue;
                    }

                    closing_row = Some(row);
                    break;
                }
            }

            let last_non_blank_row = |from_row: u32| -> Point {
                let mut row = from_row;
                while row > start.row && self.buffer_snapshot().is_line_blank(MultiBufferRow(row)) {
                    row -= 1;
                }
                Point::new(row, self.buffer_snapshot().line_len(MultiBufferRow(row)))
            };

            let end = if let Some(row) = closing_row {
                if let Some(indent_len) = self.closing_bracket_indent_len(row) {
                    // Include newline and whitespace before closing delimiter,
                    // so it appears on the same display line as the fold placeholder
                    Point::new(row, indent_len)
                } else {
                    last_non_blank_row(row - 1)
                }
            } else {
                last_non_blank_row(max_point.row)
            };

            Some(Crease::Inline {
                range: start..end,
                placeholder: self.fold_placeholder.clone(),
                render_toggle: None,
                render_trailer: None,
                metadata: None,
            })
        } else {
            None
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    #[instrument(skip_all)]
    pub fn text_highlight_ranges(
        &self,
        key: HighlightKey,
    ) -> Option<Arc<(HighlightStyle, Vec<Range<Anchor>>)>> {
        self.text_highlights.get(&key).cloned()
    }

    #[cfg(any(test, feature = "test-support"))]
    #[instrument(skip_all)]
    pub fn all_text_highlight_ranges(
        &self,
        f: &dyn Fn(&HighlightKey) -> bool,
    ) -> Vec<(gpui::Hsla, Range<Point>)> {
        use itertools::Itertools;

        self.text_highlights
            .iter()
            .filter(|(key, _)| f(key))
            .map(|(_, value)| value.clone())
            .flat_map(|ranges| {
                ranges
                    .1
                    .iter()
                    .flat_map(|range| {
                        Some((ranges.0.color?, range.to_point(self.buffer_snapshot())))
                    })
                    .collect::<Vec<_>>()
            })
            .sorted_by_key(|(_, range)| range.start)
            .collect()
    }

    #[allow(unused)]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn inlay_highlights(
        &self,
        key: HighlightKey,
    ) -> Option<&TreeMap<InlayId, (HighlightStyle, InlayHighlight)>> {
        self.inlay_highlights.get(&key)
    }

    pub fn buffer_header_height(&self) -> u32 {
        self.block_snapshot.buffer_header_height
    }

    pub fn excerpt_header_height(&self) -> u32 {
        self.block_snapshot.excerpt_header_height
    }

    /// Given a `DisplayPoint`, returns another `DisplayPoint` corresponding to
    /// the start of the buffer row that is a given number of buffer rows away
    /// from the provided point.
    ///
    /// This moves by buffer rows instead of display rows, a distinction that is
    /// important when soft wrapping is enabled.
    #[instrument(skip_all)]
    pub fn start_of_relative_buffer_row(&self, point: DisplayPoint, times: isize) -> DisplayPoint {
        let start = self.display_point_to_fold_point(point, Bias::Left);
        let target = start.row() as isize + times;
        let new_row = (target.max(0) as u32).min(self.fold_snapshot().max_point().row());

        self.clip_point(
            self.fold_point_to_display_point(
                self.fold_snapshot()
                    .clip_point(FoldPoint::new(new_row, 0), Bias::Right),
            ),
            Bias::Right,
        )
    }
}
