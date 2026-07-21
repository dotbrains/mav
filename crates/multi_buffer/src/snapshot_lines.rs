use super::*;

impl MultiBufferSnapshot {
    pub fn suggested_indents(
        &self,
        rows: impl IntoIterator<Item = u32>,
        cx: &App,
    ) -> BTreeMap<MultiBufferRow, IndentSize> {
        let mut result = BTreeMap::new();
        self.suggested_indents_callback(
            rows,
            &mut |row, indent| {
                result.insert(row, indent);
                ControlFlow::Continue(())
            },
            cx,
        );
        result
    }

    // move this to be a generator once those are a thing
    pub fn suggested_indents_callback(
        &self,
        rows: impl IntoIterator<Item = u32>,
        cb: &mut dyn FnMut(MultiBufferRow, IndentSize) -> ControlFlow<()>,
        cx: &App,
    ) {
        let mut rows_for_excerpt = Vec::new();
        let mut cursor = self.cursor::<Point, Point>();
        let mut rows = rows.into_iter().peekable();
        let mut prev_row = u32::MAX;
        let mut prev_language_indent_size = IndentSize::default();

        while let Some(row) = rows.next() {
            cursor.seek(&Point::new(row, 0));
            let Some(region) = cursor.region() else {
                continue;
            };

            // Retrieve the language and indent size once for each disjoint region being indented.
            let single_indent_size = if row.saturating_sub(1) == prev_row {
                prev_language_indent_size
            } else {
                region
                    .buffer
                    .language_indent_size_at(Point::new(row, 0), cx)
            };
            prev_language_indent_size = single_indent_size;
            prev_row = row;

            let start_buffer_row = region.buffer_range.start.row;
            let start_multibuffer_row = region.range.start.row;
            let end_multibuffer_row = region.range.end.row;

            rows_for_excerpt.push(row);
            while let Some(next_row) = rows.peek().copied() {
                if end_multibuffer_row > next_row {
                    rows_for_excerpt.push(next_row);
                    rows.next();
                } else {
                    break;
                }
            }

            let buffer_rows = rows_for_excerpt
                .drain(..)
                .map(|row| start_buffer_row + row - start_multibuffer_row);
            let buffer_indents = region
                .buffer
                .suggested_indents(buffer_rows, single_indent_size);
            for (row, indent) in buffer_indents {
                if cb(
                    MultiBufferRow(start_multibuffer_row + row - start_buffer_row),
                    indent,
                )
                .is_break()
                {
                    return;
                }
            }
        }
    }

    pub fn indent_size_for_line(&self, row: MultiBufferRow) -> IndentSize {
        if let Some((buffer, range)) = self.buffer_line_for_row(row) {
            let mut size = buffer.indent_size_for_line(range.start.row);
            size.len = size
                .len
                .min(range.end.column)
                .saturating_sub(range.start.column);
            size
        } else {
            IndentSize::spaces(0)
        }
    }

    pub fn line_indent_for_row(&self, row: MultiBufferRow) -> LineIndent {
        if let Some((buffer, range)) = self.buffer_line_for_row(row) {
            LineIndent::from_iter(buffer.text_for_range(range).flat_map(|s| s.chars()))
        } else {
            LineIndent::spaces(0)
        }
    }

    pub fn indent_and_comment_for_line(&self, row: MultiBufferRow, cx: &App) -> String {
        let mut indent = self.indent_size_for_line(row).chars().collect::<String>();

        if self.language_settings(cx).extend_comment_on_newline
            && let Some(language_scope) = self.language_scope_at(Point::new(row.0, 0))
        {
            let delimiters = language_scope.line_comment_prefixes();
            for delimiter in delimiters {
                if *self
                    .chars_at(Point::new(row.0, indent.len() as u32))
                    .take(delimiter.chars().count())
                    .collect::<String>()
                    .as_str()
                    == **delimiter
                {
                    indent.push_str(delimiter);
                    break;
                }
            }
        }

        indent
    }

    pub fn is_line_whitespace_upto<T>(&self, position: T) -> bool
    where
        T: ToOffset,
    {
        for char in self.reversed_chars_at(position) {
            if !char.is_whitespace() {
                return false;
            }
            if char == '\n' {
                return true;
            }
        }
        true
    }

    pub fn prev_non_blank_row(&self, mut row: MultiBufferRow) -> Option<MultiBufferRow> {
        while row.0 > 0 {
            row.0 -= 1;
            if !self.is_line_blank(row) {
                return Some(row);
            }
        }
        None
    }

    pub fn line_len(&self, row: MultiBufferRow) -> u32 {
        if let Some((_, range)) = self.buffer_line_for_row(row) {
            range.end.column - range.start.column
        } else {
            0
        }
    }

    pub fn line_len_utf16(&self, row: MultiBufferRow) -> u32 {
        self.clip_point_utf16(Unclipped(PointUtf16::new(row.0, u32::MAX)), Bias::Left)
            .column
    }

    pub fn buffer_line_for_row(
        &self,
        row: MultiBufferRow,
    ) -> Option<(&BufferSnapshot, Range<Point>)> {
        let mut cursor = self.cursor::<Point, Point>();
        let point = Point::new(row.0, 0);
        cursor.seek(&point);
        let region = cursor.region()?;
        let overshoot = point.min(region.range.end) - region.range.start;
        let buffer_point = region.buffer_range.start + overshoot;
        if buffer_point.row > region.buffer_range.end.row {
            return None;
        }
        let line_start = Point::new(buffer_point.row, 0).max(region.buffer_range.start);
        let line_end = Point::new(buffer_point.row, region.buffer.line_len(buffer_point.row))
            .min(region.buffer_range.end);
        Some((region.buffer, line_start..line_end))
    }

    pub fn max_point(&self) -> Point {
        self.text_summary().lines
    }

    pub fn max_row(&self) -> MultiBufferRow {
        MultiBufferRow(self.text_summary().lines.row)
    }

    pub fn text_summary(&self) -> MBTextSummary {
        self.diff_transforms.summary().output
    }

    pub fn text_summary_for_range<MBD, O>(&self, range: Range<O>) -> MBD
    where
        MBD: MultiBufferDimension + AddAssign,
        O: ToOffset,
    {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let mut cursor = self
            .diff_transforms
            .cursor::<Dimensions<MultiBufferOffset, ExcerptOffset>>(());
        cursor.seek(&range.start, Bias::Right);

        let Some(first_transform) = cursor.item() else {
            return MBD::from_summary(&MBTextSummary::default());
        };

        let diff_transform_start = cursor.start().0;
        let diff_transform_end = cursor.end().0;
        let diff_start = range.start;
        let start_overshoot = diff_start - diff_transform_start;
        let end_overshoot = std::cmp::min(range.end, diff_transform_end) - diff_transform_start;

        let mut result = match first_transform {
            DiffTransform::BufferContent { .. } => {
                let excerpt_start = cursor.start().1 + start_overshoot;
                let excerpt_end = cursor.start().1 + end_overshoot;
                self.text_summary_for_excerpt_offset_range(excerpt_start..excerpt_end)
            }
            DiffTransform::DeletedHunk {
                buffer_id,
                base_text_byte_range,
                has_trailing_newline,
                ..
            } => {
                let buffer_start = base_text_byte_range.start + start_overshoot;
                let mut buffer_end = base_text_byte_range.start + end_overshoot;
                let Some(base_text) = self.diff_state(*buffer_id).map(|diff| diff.base_text())
                else {
                    panic!("{:?} is in non-existent deleted hunk", range.start)
                };

                let include_trailing_newline =
                    *has_trailing_newline && range.end >= diff_transform_end;
                if include_trailing_newline {
                    buffer_end -= 1;
                }

                let mut summary = base_text
                    .text_summary_for_range::<MBD::TextDimension, _>(buffer_start..buffer_end);

                if include_trailing_newline {
                    summary.add_assign(&<MBD::TextDimension>::from_text_summary(
                        &TextSummary::newline(),
                    ))
                }

                let mut result = MBD::default();
                result.add_text_dim(&summary);
                result
            }
        };
        if range.end < diff_transform_end {
            return result;
        }

        cursor.next();
        result.add_mb_text_summary(
            &cursor
                .summary::<_, OutputDimension<_>>(&range.end, Bias::Right)
                .0,
        );

        let Some(last_transform) = cursor.item() else {
            return result;
        };

        let overshoot = range.end - cursor.start().0;
        let suffix = match last_transform {
            DiffTransform::BufferContent { .. } => {
                let end = cursor.start().1 + overshoot;
                self.text_summary_for_excerpt_offset_range::<MBD>(cursor.start().1..end)
            }
            DiffTransform::DeletedHunk {
                base_text_byte_range,
                buffer_id,
                has_trailing_newline,
                ..
            } => {
                let buffer_end = base_text_byte_range.start + overshoot;
                let Some(base_text) = self.diff_state(*buffer_id).map(|diff| diff.base_text())
                else {
                    panic!("{:?} is in non-existent deleted hunk", range.end)
                };

                let mut suffix = base_text.text_summary_for_range::<MBD::TextDimension, _>(
                    base_text_byte_range.start..buffer_end,
                );
                if *has_trailing_newline && buffer_end == base_text_byte_range.end + 1 {
                    suffix.add_assign(&<MBD::TextDimension>::from_text_summary(
                        &TextSummary::from("\n"),
                    ))
                }

                let mut result = MBD::default();
                result.add_text_dim(&suffix);
                result
            }
        };

        result += suffix;
        result
    }

    pub(super) fn text_summary_for_excerpt_offset_range<MBD>(
        &self,
        mut range: Range<ExcerptOffset>,
    ) -> MBD
    where
        MBD: MultiBufferDimension + AddAssign,
    {
        let mut summary = MBD::default();
        let mut cursor = self.excerpts.cursor::<ExcerptOffset>(());
        cursor.seek(&range.start, Bias::Right);
        if let Some(excerpt) = cursor.item() {
            let buffer_snapshot = excerpt.buffer_snapshot(self);
            let mut end_before_newline = cursor.end();
            if excerpt.has_trailing_newline {
                end_before_newline -= 1;
            }

            let excerpt_start = excerpt.range.context.start.to_offset(&buffer_snapshot);
            let start_in_excerpt = excerpt_start + (range.start - *cursor.start());
            let end_in_excerpt =
                excerpt_start + (cmp::min(end_before_newline, range.end) - *cursor.start());
            summary.add_text_dim(
                &buffer_snapshot.text_summary_for_range::<MBD::TextDimension, _>(
                    start_in_excerpt..end_in_excerpt,
                ),
            );

            if range.end > end_before_newline {
                summary.add_mb_text_summary(&MBTextSummary::from(TextSummary::newline()));
            }

            cursor.next();
        }

        if range.end > *cursor.start() {
            summary += cursor
                .summary::<_, ExcerptDimension<MBD>>(&range.end, Bias::Right)
                .0;
            if let Some(excerpt) = cursor.item() {
                let buffer_snapshot = excerpt.buffer_snapshot(self);
                range.end = cmp::max(*cursor.start(), range.end);

                let excerpt_start = excerpt.range.context.start.to_offset(&buffer_snapshot);
                let end_in_excerpt = excerpt_start + (range.end - *cursor.start());
                summary.add_text_dim(
                    &buffer_snapshot.text_summary_for_range::<MBD::TextDimension, _>(
                        excerpt_start..end_in_excerpt,
                    ),
                );
            }
        }

        summary
    }
}
