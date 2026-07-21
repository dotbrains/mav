use super::*;

impl BufferSnapshot {
    /// Returns the outline for the buffer.
    ///
    /// This method allows passing an optional [`SyntaxTheme`] to
    /// syntax-highlight the returned symbols.
    pub fn outline(&self, theme: Option<&SyntaxTheme>) -> Outline<Anchor> {
        Outline::new(self.outline_items_containing(0..self.len(), true, theme))
    }

    /// Returns all the symbols that contain the given position.
    ///
    /// This method allows passing an optional [`SyntaxTheme`] to
    /// syntax-highlight the returned symbols.
    pub fn symbols_containing<T: ToOffset>(
        &self,
        position: T,
        theme: Option<&SyntaxTheme>,
    ) -> Vec<OutlineItem<Anchor>> {
        let position = position.to_offset(self);
        let start = self.clip_offset(position.saturating_sub(1), Bias::Left);
        let end = self.clip_offset(position + 1, Bias::Right);
        let mut items = self.outline_items_containing(start..end, false, theme);
        let mut prev_depth = None;
        items.retain(|item| {
            let result = prev_depth.is_none_or(|prev_depth| item.depth > prev_depth);
            prev_depth = Some(item.depth);
            result
        });
        items
    }

    pub fn outline_ranges_containing<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> impl Iterator<Item = Range<Point>> + '_ {
        let range = range.to_offset(self);
        let mut matches = self.syntax.matches(range.clone(), &self.text, |grammar| {
            grammar.outline_config.as_ref().map(|c| &c.query)
        });
        let configs = matches
            .grammars()
            .iter()
            .map(|g| g.outline_config.as_ref().unwrap())
            .collect::<Vec<_>>();

        std::iter::from_fn(move || {
            while let Some(mat) = matches.peek() {
                let config = &configs[mat.grammar_index];
                let containing_item_node = maybe!({
                    let item_node = mat.captures.iter().find_map(|cap| {
                        if cap.index == config.item_capture_ix {
                            Some(cap.node)
                        } else {
                            None
                        }
                    })?;

                    let item_byte_range = item_node.byte_range();
                    if item_byte_range.end < range.start || item_byte_range.start > range.end {
                        None
                    } else {
                        Some(item_node)
                    }
                });

                let range = containing_item_node.as_ref().map(|item_node| {
                    Point::from_ts_point(item_node.start_position())
                        ..Point::from_ts_point(item_node.end_position())
                });
                matches.advance();
                if range.is_some() {
                    return range;
                }
            }
            None
        })
    }

    pub fn outline_range_containing<T: ToOffset>(&self, range: Range<T>) -> Option<Range<Point>> {
        self.outline_ranges_containing(range).next()
    }

    pub fn outline_items_containing<T: ToOffset>(
        &self,
        range: Range<T>,
        include_extra_context: bool,
        theme: Option<&SyntaxTheme>,
    ) -> Vec<OutlineItem<Anchor>> {
        self.outline_items_containing_internal(
            range,
            include_extra_context,
            theme,
            |this, range| this.anchor_after(range.start)..this.anchor_before(range.end),
        )
    }

    pub fn outline_items_as_points_containing<T: ToOffset>(
        &self,
        range: Range<T>,
        include_extra_context: bool,
        theme: Option<&SyntaxTheme>,
    ) -> Vec<OutlineItem<Point>> {
        self.outline_items_containing_internal(range, include_extra_context, theme, |_, range| {
            range
        })
    }

    pub fn outline_items_as_offsets_containing<T: ToOffset>(
        &self,
        range: Range<T>,
        include_extra_context: bool,
        theme: Option<&SyntaxTheme>,
    ) -> Vec<OutlineItem<usize>> {
        self.outline_items_containing_internal(
            range,
            include_extra_context,
            theme,
            |buffer, range| range.to_offset(buffer),
        )
    }

    fn outline_items_containing_internal<T: ToOffset, U>(
        &self,
        range: Range<T>,
        include_extra_context: bool,
        theme: Option<&SyntaxTheme>,
        range_callback: fn(&Self, Range<Point>) -> Range<U>,
    ) -> Vec<OutlineItem<U>> {
        let range = range.to_offset(self);
        let mut matches = self.syntax.matches(range.clone(), &self.text, |grammar| {
            grammar.outline_config.as_ref().map(|c| &c.query)
        });

        let mut items = Vec::new();
        let mut annotation_row_ranges: Vec<Range<u32>> = Vec::new();
        while let Some(mat) = matches.peek() {
            let config = matches.grammars()[mat.grammar_index]
                .outline_config
                .as_ref()
                .unwrap();
            if let Some(item) =
                self.next_outline_item(config, &mat, &range, include_extra_context, theme)
            {
                items.push(item);
            } else if let Some(capture) = mat
                .captures
                .iter()
                .find(|capture| Some(capture.index) == config.annotation_capture_ix)
            {
                let capture_range = capture.node.start_position()..capture.node.end_position();
                let mut capture_row_range =
                    capture_range.start.row as u32..capture_range.end.row as u32;
                if capture_range.end.row > capture_range.start.row && capture_range.end.column == 0
                {
                    capture_row_range.end -= 1;
                }
                if let Some(last_row_range) = annotation_row_ranges.last_mut() {
                    if last_row_range.end >= capture_row_range.start.saturating_sub(1) {
                        last_row_range.end = capture_row_range.end;
                    } else {
                        annotation_row_ranges.push(capture_row_range);
                    }
                } else {
                    annotation_row_ranges.push(capture_row_range);
                }
            }
            matches.advance();
        }

        items.sort_by_key(|item| (item.range.start, Reverse(item.range.end)));

        // Assign depths based on containment relationships and convert to anchors.
        let mut item_ends_stack = Vec::<Point>::new();
        let mut anchor_items = Vec::new();
        let mut annotation_row_ranges = annotation_row_ranges.into_iter().peekable();
        for item in items {
            while let Some(last_end) = item_ends_stack.last().copied() {
                if last_end < item.range.end {
                    item_ends_stack.pop();
                } else {
                    break;
                }
            }

            let mut annotation_row_range = None;
            while let Some(next_annotation_row_range) = annotation_row_ranges.peek() {
                let row_preceding_item = item.range.start.row.saturating_sub(1);
                if next_annotation_row_range.end < row_preceding_item {
                    annotation_row_ranges.next();
                } else {
                    if next_annotation_row_range.end == row_preceding_item {
                        annotation_row_range = Some(next_annotation_row_range.clone());
                        annotation_row_ranges.next();
                    }
                    break;
                }
            }

            anchor_items.push(OutlineItem {
                depth: item_ends_stack.len(),
                range: range_callback(self, item.range.clone()),
                selection_range: range_callback(self, item.selection_range.clone()),
                source_range_for_text: range_callback(self, item.source_range_for_text.clone()),
                text: item.text,
                highlight_ranges: item.highlight_ranges,
                name_ranges: item.name_ranges,
                body_range: item.body_range.map(|r| range_callback(self, r)),
                annotation_range: annotation_row_range.map(|annotation_range| {
                    let point_range = Point::new(annotation_range.start, 0)
                        ..Point::new(annotation_range.end, self.line_len(annotation_range.end));
                    range_callback(self, point_range)
                }),
            });
            item_ends_stack.push(item.range.end);
        }

        anchor_items
    }

    fn next_outline_item(
        &self,
        config: &OutlineConfig,
        mat: &SyntaxMapMatch,
        range: &Range<usize>,
        include_extra_context: bool,
        theme: Option<&SyntaxTheme>,
    ) -> Option<OutlineItem<Point>> {
        let item_node = mat.captures.iter().find_map(|cap| {
            if cap.index == config.item_capture_ix {
                Some(cap.node)
            } else {
                None
            }
        })?;

        let item_byte_range = item_node.byte_range();
        if item_byte_range.end < range.start || item_byte_range.start > range.end {
            return None;
        }
        let item_point_range = Point::from_ts_point(item_node.start_position())
            ..Point::from_ts_point(item_node.end_position());

        let mut open_point = None;
        let mut close_point = None;

        let mut buffer_ranges = Vec::new();
        let mut add_to_buffer_ranges = |node: tree_sitter::Node, node_is_name| {
            let mut range = node.start_byte()..node.end_byte();
            let start = node.start_position();
            if node.end_position().row > start.row {
                range.end = range.start + self.line_len(start.row as u32) as usize - start.column;
            }

            if !range.is_empty() {
                buffer_ranges.push((range, node_is_name));
            }
        };

        for capture in mat.captures {
            if capture.index == config.name_capture_ix {
                add_to_buffer_ranges(capture.node, true);
            } else if Some(capture.index) == config.context_capture_ix
                || (Some(capture.index) == config.extra_context_capture_ix && include_extra_context)
            {
                add_to_buffer_ranges(capture.node, false);
            } else {
                if Some(capture.index) == config.open_capture_ix {
                    open_point = Some(Point::from_ts_point(capture.node.end_position()));
                } else if Some(capture.index) == config.close_capture_ix {
                    close_point = Some(Point::from_ts_point(capture.node.start_position()));
                }
            }
        }

        if buffer_ranges.is_empty() {
            return None;
        }
        let source_range_for_text =
            buffer_ranges.first().unwrap().0.start..buffer_ranges.last().unwrap().0.end;
        let selection_range = buffer_ranges
            .iter()
            .filter(|(_, node_is_name)| *node_is_name)
            .map(|(buffer_range, _)| buffer_range.clone())
            .reduce(|mut combined_range, next_range| {
                combined_range.end = next_range.end;
                combined_range
            })?;

        let mut text = String::new();
        let mut highlight_ranges = Vec::new();
        let mut name_ranges = Vec::new();
        let mut chunks = self.chunks(
            source_range_for_text.clone(),
            LanguageAwareStyling {
                tree_sitter: true,
                diagnostics: true,
            },
        );
        let mut last_buffer_range_end = 0;
        for (buffer_range, is_name) in buffer_ranges {
            let space_added = !text.is_empty() && buffer_range.start > last_buffer_range_end;
            if space_added {
                text.push(' ');
            }
            let before_append_len = text.len();
            let mut offset = buffer_range.start;
            chunks.seek(buffer_range.clone());
            for mut chunk in chunks.by_ref() {
                if chunk.text.len() > buffer_range.end - offset {
                    chunk.text = &chunk.text[0..(buffer_range.end - offset)];
                    offset = buffer_range.end;
                } else {
                    offset += chunk.text.len();
                }
                let style = chunk
                    .syntax_highlight_id
                    .zip(theme)
                    .and_then(|(highlight, theme)| theme.get(highlight).cloned());

                if let Some(style) = style {
                    let start = text.len();
                    let end = start + chunk.text.len();
                    highlight_ranges.push((start..end, style));
                }
                text.push_str(chunk.text);
                if offset >= buffer_range.end {
                    break;
                }
            }
            if is_name {
                let after_append_len = text.len();
                let start = if space_added && !name_ranges.is_empty() {
                    before_append_len - 1
                } else {
                    before_append_len
                };
                name_ranges.push(start..after_append_len);
            }
            last_buffer_range_end = buffer_range.end;
        }

        Some(OutlineItem {
            depth: 0, // We'll calculate the depth later
            range: item_point_range,
            selection_range: selection_range.to_point(self),
            source_range_for_text: source_range_for_text.to_point(self),
            text: text.into(),
            highlight_ranges,
            name_ranges,
            body_range: open_point.zip(close_point).map(|(start, end)| start..end),
            annotation_range: None,
        })
    }

    pub fn function_body_fold_ranges<T: ToOffset>(
        &self,
        within: Range<T>,
    ) -> impl Iterator<Item = Range<usize>> + '_ {
        self.text_object_ranges(within, TreeSitterOptions::default())
            .filter_map(|(range, obj)| (obj == TextObject::InsideFunction).then_some(range))
    }
}
