use super::*;

impl MultiBufferSnapshot {
    pub fn line_indents(
        &self,
        start_row: MultiBufferRow,
        buffer_filter: impl Fn(&BufferSnapshot) -> bool,
    ) -> impl Iterator<Item = (MultiBufferRow, LineIndent, &BufferSnapshot)> {
        let max_point = self.max_point();
        let mut cursor = self.cursor::<Point, Point>();
        cursor.seek(&Point::new(start_row.0, 0));
        iter::from_fn(move || {
            let mut region = cursor.region()?;
            while !buffer_filter(&region.excerpt.buffer_snapshot(self)) {
                cursor.next();
                region = cursor.region()?;
            }
            let region = cursor.region()?;
            let overshoot = start_row.0.saturating_sub(region.range.start.row);
            let buffer_start_row =
                (region.buffer_range.start.row + overshoot).min(region.buffer_range.end.row);

            let buffer_end_row = if region.is_main_buffer
                && (region.has_trailing_newline || region.range.end == max_point)
            {
                region.buffer_range.end.row
            } else {
                region.buffer_range.end.row.saturating_sub(1)
            };

            let line_indents = region
                .buffer
                .line_indents_in_row_range(buffer_start_row..buffer_end_row);
            let region_buffer_row = region.buffer_range.start.row;
            let region_row = region.range.start.row;
            let region_buffer = region.excerpt.buffer_snapshot(self);
            cursor.next();
            Some(line_indents.map(move |(buffer_row, indent)| {
                let row = region_row + (buffer_row - region_buffer_row);
                (MultiBufferRow(row), indent, region_buffer)
            }))
        })
        .flatten()
    }

    pub fn reversed_line_indents(
        &self,
        end_row: MultiBufferRow,
        buffer_filter: impl Fn(&BufferSnapshot) -> bool,
    ) -> impl Iterator<Item = (MultiBufferRow, LineIndent, &BufferSnapshot)> {
        let max_point = self.max_point();
        let mut cursor = self.cursor::<Point, Point>();
        cursor.seek(&Point::new(end_row.0, 0));
        iter::from_fn(move || {
            let mut region = cursor.region()?;
            while !buffer_filter(&region.excerpt.buffer_snapshot(self)) {
                cursor.prev();
                region = cursor.region()?;
            }
            let region = cursor.region()?;

            let buffer_start_row = region.buffer_range.start.row;
            let buffer_end_row = if region.is_main_buffer
                && (region.has_trailing_newline || region.range.end == max_point)
            {
                region.buffer_range.end.row + 1
            } else {
                region.buffer_range.end.row
            };

            let overshoot = end_row.0 - region.range.start.row;
            let buffer_end_row =
                (region.buffer_range.start.row + overshoot + 1).min(buffer_end_row);

            let line_indents = region
                .buffer
                .reversed_line_indents_in_row_range(buffer_start_row..buffer_end_row);
            let region_buffer_row = region.buffer_range.start.row;
            let region_row = region.range.start.row;
            let region_buffer = region.excerpt.buffer_snapshot(self);
            cursor.prev();
            Some(line_indents.map(move |(buffer_row, indent)| {
                let row = region_row + (buffer_row - region_buffer_row);
                (MultiBufferRow(row), indent, region_buffer)
            }))
        })
        .flatten()
    }

    pub async fn enclosing_indent(
        &self,
        mut target_row: MultiBufferRow,
    ) -> Option<(Range<MultiBufferRow>, LineIndent)> {
        let max_row = MultiBufferRow(self.max_point().row);
        if target_row >= max_row {
            return None;
        }

        let mut target_indent = self.line_indent_for_row(target_row);

        // If the current row is at the start of an indented block, we want to return this
        // block as the enclosing indent.
        if !target_indent.is_line_empty() && target_row < max_row {
            let next_line_indent = self.line_indent_for_row(MultiBufferRow(target_row.0 + 1));
            if !next_line_indent.is_line_empty()
                && target_indent.raw_len() < next_line_indent.raw_len()
            {
                target_indent = next_line_indent;
                target_row.0 += 1;
            }
        }

        const SEARCH_ROW_LIMIT: u32 = 25000;
        const SEARCH_WHITESPACE_ROW_LIMIT: u32 = 2500;
        const YIELD_INTERVAL: u32 = 100;

        let mut accessed_row_counter = 0;

        // If there is a blank line at the current row, search for the next non indented lines
        if target_indent.is_line_empty() {
            let start = MultiBufferRow(target_row.0.saturating_sub(SEARCH_WHITESPACE_ROW_LIMIT));
            let end =
                MultiBufferRow((max_row.0 + 1).min(target_row.0 + SEARCH_WHITESPACE_ROW_LIMIT));

            let mut non_empty_line_above = None;
            for (row, indent, _) in self.reversed_line_indents(target_row, |_| true) {
                if row < start {
                    break;
                }
                accessed_row_counter += 1;
                if accessed_row_counter == YIELD_INTERVAL {
                    accessed_row_counter = 0;
                    yield_now().await;
                }
                if !indent.is_line_empty() {
                    non_empty_line_above = Some((row, indent));
                    break;
                }
            }

            let mut non_empty_line_below = None;
            for (row, indent, _) in self.line_indents(target_row, |_| true) {
                if row > end {
                    break;
                }
                accessed_row_counter += 1;
                if accessed_row_counter == YIELD_INTERVAL {
                    accessed_row_counter = 0;
                    yield_now().await;
                }
                if !indent.is_line_empty() {
                    non_empty_line_below = Some((row, indent));
                    break;
                }
            }

            let (row, indent) = match (non_empty_line_above, non_empty_line_below) {
                (Some((above_row, above_indent)), Some((below_row, below_indent))) => {
                    if above_indent.raw_len() >= below_indent.raw_len() {
                        (above_row, above_indent)
                    } else {
                        (below_row, below_indent)
                    }
                }
                (Some(above), None) => above,
                (None, Some(below)) => below,
                _ => return None,
            };

            target_indent = indent;
            target_row = row;
        }

        let start = MultiBufferRow(target_row.0.saturating_sub(SEARCH_ROW_LIMIT));
        let end = MultiBufferRow((max_row.0 + 1).min(target_row.0 + SEARCH_ROW_LIMIT));

        let mut start_indent = None;
        for (row, indent, _) in self.reversed_line_indents(target_row, |_| true) {
            if row < start {
                break;
            }
            accessed_row_counter += 1;
            if accessed_row_counter == YIELD_INTERVAL {
                accessed_row_counter = 0;
                yield_now().await;
            }
            if !indent.is_line_empty() && indent.raw_len() < target_indent.raw_len() {
                start_indent = Some((row, indent));
                break;
            }
        }
        let (start_row, start_indent_size) = start_indent?;

        let mut end_indent = (end, None);
        for (row, indent, _) in self.line_indents(target_row, |_| true) {
            if row > end {
                break;
            }
            accessed_row_counter += 1;
            if accessed_row_counter == YIELD_INTERVAL {
                accessed_row_counter = 0;
                yield_now().await;
            }
            if !indent.is_line_empty() && indent.raw_len() < target_indent.raw_len() {
                end_indent = (MultiBufferRow(row.0.saturating_sub(1)), Some(indent));
                break;
            }
        }
        let (end_row, end_indent_size) = end_indent;

        let indent = if let Some(end_indent_size) = end_indent_size {
            if start_indent_size.raw_len() > end_indent_size.raw_len() {
                start_indent_size
            } else {
                end_indent_size
            }
        } else {
            start_indent_size
        };

        Some((start_row..end_row, indent))
    }

    pub fn indent_guides_in_range<T: ToPoint>(
        &self,
        range: Range<T>,
        ignore_disabled_for_language: bool,
        cx: &App,
    ) -> impl Iterator<Item = IndentGuide> {
        let range = range.start.to_point(self)..range.end.to_point(self);
        let start_row = MultiBufferRow(range.start.row);
        let end_row = MultiBufferRow(range.end.row);

        let mut row_indents = self.line_indents(start_row, |buffer| {
            let settings = LanguageSettings::for_buffer_snapshot(buffer, None, cx);
            settings.indent_guides.enabled || ignore_disabled_for_language
        });

        let mut result = Vec::new();
        let mut indent_stack = SmallVec::<[IndentGuide; 8]>::new();

        let mut prev_settings = None;
        while let Some((first_row, mut line_indent, buffer)) = row_indents.next() {
            if first_row > end_row {
                break;
            }
            let current_depth = indent_stack.len() as u32;

            // Avoid retrieving the language settings repeatedly for every buffer row.
            if let Some((prev_buffer_id, _)) = &prev_settings
                && prev_buffer_id != &buffer.remote_id()
            {
                prev_settings.take();
            }
            let settings = &prev_settings
                .get_or_insert_with(|| {
                    (
                        buffer.remote_id(),
                        LanguageSettings::for_buffer_snapshot(buffer, None, cx),
                    )
                })
                .1;
            let tab_size = settings.tab_size.get();

            // When encountering empty, continue until found useful line indent
            // then add to the indent stack with the depth found
            let mut found_indent = false;
            let mut last_row = first_row;
            if line_indent.is_line_blank() {
                while !found_indent {
                    let Some((target_row, new_line_indent, _)) = row_indents.next() else {
                        break;
                    };
                    const TRAILING_ROW_SEARCH_LIMIT: u32 = 25;
                    if target_row > MultiBufferRow(end_row.0 + TRAILING_ROW_SEARCH_LIMIT) {
                        break;
                    }

                    if new_line_indent.is_line_blank() {
                        continue;
                    }
                    last_row = target_row.min(end_row);
                    line_indent = new_line_indent;
                    found_indent = true;
                    break;
                }
            } else {
                found_indent = true
            }

            let depth = if found_indent {
                line_indent.len(tab_size) / tab_size
            } else {
                0
            };

            match depth.cmp(&current_depth) {
                cmp::Ordering::Less => {
                    for _ in 0..(current_depth - depth) {
                        let mut indent = indent_stack.pop().unwrap();
                        if last_row != first_row {
                            // In this case, we landed on an empty row, had to seek forward,
                            // and discovered that the indent we where on is ending.
                            // This means that the last display row must
                            // be on line that ends this indent range, so we
                            // should display the range up to the first non-empty line
                            indent.end_row = MultiBufferRow(first_row.0.saturating_sub(1));
                        }

                        result.push(indent)
                    }
                }
                cmp::Ordering::Greater => {
                    for next_depth in current_depth..depth {
                        indent_stack.push(IndentGuide {
                            buffer_id: buffer.remote_id(),
                            start_row: first_row,
                            end_row: last_row,
                            depth: next_depth,
                            tab_size,
                            settings: settings.indent_guides.clone(),
                        });
                    }
                }
                _ => {}
            }

            for indent in indent_stack.iter_mut() {
                indent.end_row = last_row;
            }
        }

        result.extend(indent_stack);
        result.into_iter()
    }
}
