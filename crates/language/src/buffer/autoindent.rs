use super::*;

impl Buffer {
    pub(super) fn request_autoindent(
        &mut self,
        cx: &mut Context<Self>,
        block_budget: Option<Duration>,
    ) {
        if let Some(indent_sizes) = self.compute_autoindents() {
            let indent_sizes = cx.background_spawn(indent_sizes);
            let Some(block_budget) = block_budget else {
                self.pending_autoindent = Some(cx.spawn(async move |this, cx| {
                    let indent_sizes = indent_sizes.await;
                    this.update(cx, |this, cx| {
                        this.apply_autoindents(indent_sizes, cx);
                    })
                    .ok();
                }));
                return;
            };
            match cx
                .foreground_executor()
                .block_with_timeout(block_budget, indent_sizes)
            {
                Ok(indent_sizes) => self.apply_autoindents(indent_sizes, cx),
                Err(indent_sizes) => {
                    self.pending_autoindent = Some(cx.spawn(async move |this, cx| {
                        let indent_sizes = indent_sizes.await;
                        this.update(cx, |this, cx| {
                            this.apply_autoindents(indent_sizes, cx);
                        })
                        .ok();
                    }));
                }
            }
        } else {
            self.autoindent_requests.clear();
            for tx in self.wait_for_autoindent_txs.drain(..) {
                tx.send(()).ok();
            }
        }
    }

    fn compute_autoindents(
        &self,
    ) -> Option<impl Future<Output = BTreeMap<u32, IndentSize>> + use<>> {
        let max_rows_between_yields = 100;
        let snapshot = self.snapshot();
        if snapshot.syntax.is_empty() || self.autoindent_requests.is_empty() {
            return None;
        }

        let autoindent_requests = self.autoindent_requests.clone();
        Some(async move {
            let mut indent_sizes = BTreeMap::<u32, (IndentSize, bool)>::new();
            for request in autoindent_requests {
                // Resolve each edited range to its row in the current buffer and in the
                // buffer before this batch of edits.
                let mut row_ranges = Vec::new();
                let mut old_to_new_rows = BTreeMap::new();
                let mut language_indent_sizes_by_new_row = Vec::new();
                for entry in &request.entries {
                    let position = entry.range.start;
                    let new_row = position.to_point(&snapshot).row;
                    let new_end_row = entry.range.end.to_point(&snapshot).row + 1;
                    language_indent_sizes_by_new_row.push((new_row, entry.indent_size));

                    if let Some(old_row) = entry.old_row {
                        old_to_new_rows.insert(old_row, new_row);
                    }
                    row_ranges.push((new_row..new_end_row, entry.original_indent_column));
                }

                // Build a map containing the suggested indentation for each of the edited lines
                // with respect to the state of the buffer before these edits. This map is keyed
                // by the rows for these lines in the current state of the buffer.
                let mut old_suggestions = BTreeMap::<u32, (IndentSize, bool)>::default();
                let old_edited_ranges =
                    contiguous_ranges(old_to_new_rows.keys().copied(), max_rows_between_yields);
                let mut language_indent_sizes = language_indent_sizes_by_new_row.iter().peekable();
                let mut language_indent_size = IndentSize::default();
                for old_edited_range in old_edited_ranges {
                    let suggestions = request
                        .before_edit
                        .suggest_autoindents(old_edited_range.clone())
                        .into_iter()
                        .flatten();
                    for (old_row, suggestion) in old_edited_range.zip(suggestions) {
                        if let Some(suggestion) = suggestion {
                            let new_row = *old_to_new_rows.get(&old_row).unwrap();

                            // Find the indent size based on the language for this row.
                            while let Some((row, size)) = language_indent_sizes.peek() {
                                if *row > new_row {
                                    break;
                                }
                                language_indent_size = *size;
                                language_indent_sizes.next();
                            }

                            let suggested_indent = old_to_new_rows
                                .get(&suggestion.basis_row)
                                .and_then(|from_row| {
                                    Some(old_suggestions.get(from_row).copied()?.0)
                                })
                                .unwrap_or_else(|| {
                                    request
                                        .before_edit
                                        .indent_size_for_line(suggestion.basis_row)
                                })
                                .with_delta(suggestion.delta, language_indent_size);
                            old_suggestions
                                .insert(new_row, (suggested_indent, suggestion.within_error));
                        }
                    }
                    yield_now().await;
                }

                // Compute new suggestions for each line, but only include them in the result
                // if they differ from the old suggestion for that line.
                let mut language_indent_sizes = language_indent_sizes_by_new_row.iter().peekable();
                let mut language_indent_size = IndentSize::default();
                for (row_range, original_indent_column) in row_ranges {
                    let new_edited_row_range = if request.is_block_mode {
                        row_range.start..row_range.start + 1
                    } else {
                        row_range.clone()
                    };

                    let suggestions = snapshot
                        .suggest_autoindents(new_edited_row_range.clone())
                        .into_iter()
                        .flatten();
                    for (new_row, suggestion) in new_edited_row_range.zip(suggestions) {
                        if let Some(suggestion) = suggestion {
                            // Find the indent size based on the language for this row.
                            while let Some((row, size)) = language_indent_sizes.peek() {
                                if *row > new_row {
                                    break;
                                }
                                language_indent_size = *size;
                                language_indent_sizes.next();
                            }

                            let suggested_indent = indent_sizes
                                .get(&suggestion.basis_row)
                                .copied()
                                .map(|e| e.0)
                                .unwrap_or_else(|| {
                                    snapshot.indent_size_for_line(suggestion.basis_row)
                                })
                                .with_delta(suggestion.delta, language_indent_size);

                            if old_suggestions.get(&new_row).is_none_or(
                                |(old_indentation, was_within_error)| {
                                    suggested_indent != *old_indentation
                                        && (!suggestion.within_error || *was_within_error)
                                },
                            ) {
                                indent_sizes.insert(
                                    new_row,
                                    (suggested_indent, request.ignore_empty_lines),
                                );
                            }
                        }
                    }

                    if let (true, Some(original_indent_column)) =
                        (request.is_block_mode, original_indent_column)
                    {
                        let new_indent =
                            if let Some((indent, _)) = indent_sizes.get(&row_range.start) {
                                *indent
                            } else {
                                snapshot.indent_size_for_line(row_range.start)
                            };
                        let delta = new_indent.len as i64 - original_indent_column as i64;
                        if delta != 0 {
                            for row in row_range.skip(1) {
                                indent_sizes.entry(row).or_insert_with(|| {
                                    let mut size = snapshot.indent_size_for_line(row);
                                    if size.kind == new_indent.kind {
                                        match delta.cmp(&0) {
                                            Ordering::Greater => size.len += delta as u32,
                                            Ordering::Less => {
                                                size.len = size.len.saturating_sub(-delta as u32)
                                            }
                                            Ordering::Equal => {}
                                        }
                                    }
                                    (size, request.ignore_empty_lines)
                                });
                            }
                        }
                    }

                    yield_now().await;
                }
            }

            indent_sizes
                .into_iter()
                .filter_map(|(row, (indent, ignore_empty_lines))| {
                    if ignore_empty_lines && snapshot.line_len(row) == 0 {
                        None
                    } else {
                        Some((row, indent))
                    }
                })
                .collect()
        })
    }

    fn apply_autoindents(
        &mut self,
        indent_sizes: BTreeMap<u32, IndentSize>,
        cx: &mut Context<Self>,
    ) {
        self.autoindent_requests.clear();
        for tx in self.wait_for_autoindent_txs.drain(..) {
            tx.send(()).ok();
        }

        let edits: Vec<_> = indent_sizes
            .into_iter()
            .filter_map(|(row, indent_size)| {
                let current_size = indent_size_for_line(self, row);
                Self::edit_for_indent_size_adjustment(row, current_size, indent_size)
            })
            .collect();

        let preserve_preview = self.preserve_preview();
        self.edit(edits, None, cx);
        if preserve_preview {
            self.refresh_preview();
        }
    }

    /// Create a minimal edit that will cause the given row to be indented
    /// with the given size. After applying this edit, the length of the line
    /// will always be at least `new_size.len`.
    pub fn edit_for_indent_size_adjustment(
        row: u32,
        current_size: IndentSize,
        new_size: IndentSize,
    ) -> Option<(Range<Point>, String)> {
        if new_size.kind == current_size.kind {
            match new_size.len.cmp(&current_size.len) {
                Ordering::Greater => {
                    let point = Point::new(row, 0);
                    Some((
                        point..point,
                        iter::repeat(new_size.char())
                            .take((new_size.len - current_size.len) as usize)
                            .collect::<String>(),
                    ))
                }

                Ordering::Less => Some((
                    Point::new(row, 0)..Point::new(row, current_size.len - new_size.len),
                    String::new(),
                )),

                Ordering::Equal => None,
            }
        } else {
            Some((
                Point::new(row, 0)..Point::new(row, current_size.len),
                iter::repeat(new_size.char())
                    .take(new_size.len as usize)
                    .collect::<String>(),
            ))
        }
    }
}
