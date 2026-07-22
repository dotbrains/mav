use super::*;

impl BufferSnapshot {
    /// Returns [`IndentSize`] for a given line that respects user settings and
    /// language preferences.
    pub fn indent_size_for_line(&self, row: u32) -> IndentSize {
        indent_size_for_line(self, row)
    }

    /// Returns [`IndentSize`] for a given position that respects user settings
    /// and language preferences.
    pub fn language_indent_size_at<T: ToOffset>(&self, position: T, cx: &App) -> IndentSize {
        let settings = self.settings_at(position, cx);
        if settings.hard_tabs {
            IndentSize::tab()
        } else {
            IndentSize::spaces(settings.tab_size.get())
        }
    }

    /// Retrieve the suggested indent size for all of the given rows. The unit of indentation
    /// is passed in as `single_indent_size`.
    pub fn suggested_indents(
        &self,
        rows: impl Iterator<Item = u32>,
        single_indent_size: IndentSize,
    ) -> BTreeMap<u32, IndentSize> {
        let mut result = BTreeMap::new();

        for row_range in contiguous_ranges(rows, 10) {
            let suggestions = match self.suggest_autoindents(row_range.clone()) {
                Some(suggestions) => suggestions,
                _ => break,
            };

            for (row, suggestion) in row_range.zip(suggestions) {
                let indent_size = if let Some(suggestion) = suggestion {
                    result
                        .get(&suggestion.basis_row)
                        .copied()
                        .unwrap_or_else(|| self.indent_size_for_line(suggestion.basis_row))
                        .with_delta(suggestion.delta, single_indent_size)
                } else {
                    self.indent_size_for_line(row)
                };

                result.insert(row, indent_size);
            }
        }

        result
    }

    pub(super) fn suggest_autoindents(
        &self,
        row_range: Range<u32>,
    ) -> Option<impl Iterator<Item = Option<IndentSuggestion>> + '_> {
        let config = &self.language.as_ref()?.config;
        let prev_non_blank_row = self.prev_non_blank_row(row_range.start);

        #[derive(Debug, Clone)]
        struct StartPosition {
            start: Point,
            suffix: SharedString,
            language: Arc<Language>,
        }

        // Find the suggested indentation ranges based on the syntax tree.
        let start = Point::new(prev_non_blank_row.unwrap_or(row_range.start), 0);
        let end = Point::new(row_range.end, 0);
        let range = (start..end).to_offset(&self.text);
        let mut matches = self.syntax.matches_with_options(
            range.clone(),
            &self.text,
            TreeSitterOptions {
                max_bytes_to_query: Some(MAX_BYTES_TO_QUERY),
                max_start_depth: None,
            },
            |grammar| Some(&grammar.indents_config.as_ref()?.query),
        );
        let indent_configs = matches
            .grammars()
            .iter()
            .map(|grammar| grammar.indents_config.as_ref().unwrap())
            .collect::<Vec<_>>();

        let mut indent_ranges = Vec::<Range<Point>>::new();
        let mut start_positions = Vec::<StartPosition>::new();
        let mut outdent_positions = Vec::<Point>::new();
        while let Some(mat) = matches.peek() {
            let mut start: Option<Point> = None;
            let mut end: Option<Point> = None;

            let config = indent_configs[mat.grammar_index];
            for capture in mat.captures {
                if capture.index == config.indent_capture_ix {
                    start.get_or_insert(Point::from_ts_point(capture.node.start_position()));
                    end.get_or_insert(Point::from_ts_point(capture.node.end_position()));
                } else if Some(capture.index) == config.start_capture_ix {
                    start = Some(Point::from_ts_point(capture.node.end_position()));
                } else if Some(capture.index) == config.end_capture_ix {
                    end = Some(Point::from_ts_point(capture.node.start_position()));
                } else if Some(capture.index) == config.outdent_capture_ix {
                    outdent_positions.push(Point::from_ts_point(capture.node.start_position()));
                } else if let Some(suffix) = config.suffixed_start_captures.get(&capture.index) {
                    start_positions.push(StartPosition {
                        start: Point::from_ts_point(capture.node.start_position()),
                        suffix: suffix.clone(),
                        language: mat.language.clone(),
                    });
                }
            }

            matches.advance();
            if let Some((start, end)) = start.zip(end) {
                if start.row == end.row {
                    continue;
                }
                let range = start..end;
                match indent_ranges.binary_search_by_key(&range.start, |r| r.start) {
                    Err(ix) => indent_ranges.insert(ix, range),
                    Ok(ix) => {
                        let prev_range = &mut indent_ranges[ix];
                        prev_range.end = prev_range.end.max(range.end);
                    }
                }
            }
        }

        let mut error_ranges = Vec::<Range<Point>>::new();
        let mut matches = self
            .syntax
            .matches(range, &self.text, |grammar| grammar.error_query.as_ref());
        while let Some(mat) = matches.peek() {
            let node = mat.captures[0].node;
            let start = Point::from_ts_point(node.start_position());
            let end = Point::from_ts_point(node.end_position());
            let range = start..end;
            let ix = match error_ranges.binary_search_by_key(&range.start, |r| r.start) {
                Ok(ix) | Err(ix) => ix,
            };
            let mut end_ix = ix;
            while let Some(existing_range) = error_ranges.get(end_ix) {
                if existing_range.end < end {
                    end_ix += 1;
                } else {
                    break;
                }
            }
            error_ranges.splice(ix..end_ix, [range]);
            matches.advance();
        }

        outdent_positions.sort();
        for outdent_position in outdent_positions {
            // find the innermost indent range containing this outdent_position
            // set its end to the outdent position
            if let Some(range_to_truncate) = indent_ranges
                .iter_mut()
                .rfind(|indent_range| indent_range.contains(&outdent_position))
            {
                range_to_truncate.end = outdent_position;
            }
        }

        start_positions.sort_by_key(|b| b.start);

        // Find the suggested indentation increases and decreased based on regexes.
        let mut regex_outdent_map = HashMap::default();
        let mut last_seen_suffix: HashMap<String, Vec<StartPosition>> = HashMap::default();
        let mut start_positions_iter = start_positions.iter().peekable();

        let mut indent_change_rows = Vec::<(u32, Ordering)>::new();
        self.for_each_line(
            Point::new(prev_non_blank_row.unwrap_or(row_range.start), 0)
                ..Point::new(row_range.end, 0),
            |row, line| {
                let indent_len = self.indent_size_for_line(row).len;
                let row_language = self.language_at(Point::new(row, indent_len)).cloned();
                let row_language_config = row_language
                    .as_ref()
                    .map(|lang| lang.config())
                    .unwrap_or(config);

                if row_language_config
                    .decrease_indent_pattern
                    .as_ref()
                    .is_some_and(|regex| regex.is_match(line))
                {
                    indent_change_rows.push((row, Ordering::Less));
                }
                if row_language_config
                    .increase_indent_pattern
                    .as_ref()
                    .is_some_and(|regex| regex.is_match(line))
                {
                    indent_change_rows.push((row + 1, Ordering::Greater));
                }
                while let Some(pos) = start_positions_iter.peek() {
                    if pos.start.row < row {
                        let pos = start_positions_iter.next().unwrap().clone();
                        last_seen_suffix
                            .entry(pos.suffix.to_string())
                            .or_default()
                            .push(pos);
                    } else {
                        break;
                    }
                }
                for rule in &row_language_config.decrease_indent_patterns {
                    if rule.pattern.as_ref().is_some_and(|r| r.is_match(line)) {
                        let row_start_column = self.indent_size_for_line(row).len;
                        let basis_row = rule
                            .valid_after
                            .iter()
                            .filter_map(|valid_suffix| last_seen_suffix.get(valid_suffix))
                            .flatten()
                            .filter(|pos| {
                                row_language
                                    .as_ref()
                                    .or(self.language.as_ref())
                                    .is_some_and(|lang| Arc::ptr_eq(lang, &pos.language))
                            })
                            .filter(|pos| pos.start.column <= row_start_column)
                            .max_by_key(|pos| pos.start.row);
                        if let Some(outdent_to) = basis_row {
                            regex_outdent_map.insert(row, outdent_to.start.row);
                        }
                        break;
                    }
                }
            },
        );

        let mut indent_changes = indent_change_rows.into_iter().peekable();
        let mut prev_row = if config.auto_indent_using_last_non_empty_line {
            prev_non_blank_row.unwrap_or(0)
        } else {
            row_range.start.saturating_sub(1)
        };

        let mut prev_row_start = Point::new(prev_row, self.indent_size_for_line(prev_row).len);
        Some(row_range.map(move |row| {
            let row_start = Point::new(row, self.indent_size_for_line(row).len);

            let mut indent_from_prev_row = false;
            let mut outdent_from_prev_row = false;
            let mut outdent_to_row = u32::MAX;
            let mut from_regex = false;

            while let Some((indent_row, delta)) = indent_changes.peek() {
                match indent_row.cmp(&row) {
                    Ordering::Equal => match delta {
                        Ordering::Less => {
                            from_regex = true;
                            outdent_from_prev_row = true
                        }
                        Ordering::Greater => {
                            indent_from_prev_row = true;
                            from_regex = true
                        }
                        _ => {}
                    },

                    Ordering::Greater => break,
                    Ordering::Less => {}
                }

                indent_changes.next();
            }

            for range in &indent_ranges {
                if range.start.row >= row {
                    break;
                }
                if range.start.row == prev_row && range.end > row_start {
                    indent_from_prev_row = true;
                }
                if range.end > prev_row_start && range.end <= row_start {
                    outdent_to_row = outdent_to_row.min(range.start.row);
                }
            }

            if let Some(basis_row) = regex_outdent_map.get(&row) {
                indent_from_prev_row = false;
                outdent_to_row = *basis_row;
                from_regex = true;
            }

            let within_error = error_ranges
                .iter()
                .any(|e| e.start.row < row && e.end > row_start);

            let suggestion = if outdent_to_row == prev_row
                || (outdent_from_prev_row && indent_from_prev_row)
            {
                Some(IndentSuggestion {
                    basis_row: prev_row,
                    delta: Ordering::Equal,
                    within_error: within_error && !from_regex,
                })
            } else if indent_from_prev_row {
                Some(IndentSuggestion {
                    basis_row: prev_row,
                    delta: Ordering::Greater,
                    within_error: within_error && !from_regex,
                })
            } else if outdent_to_row < prev_row {
                Some(IndentSuggestion {
                    basis_row: outdent_to_row,
                    delta: Ordering::Equal,
                    within_error: within_error && !from_regex,
                })
            } else if outdent_from_prev_row {
                Some(IndentSuggestion {
                    basis_row: prev_row,
                    delta: Ordering::Less,
                    within_error: within_error && !from_regex,
                })
            } else if config.auto_indent_using_last_non_empty_line || !self.is_line_blank(prev_row)
            {
                Some(IndentSuggestion {
                    basis_row: prev_row,
                    delta: Ordering::Equal,
                    within_error: within_error && !from_regex,
                })
            } else {
                None
            };

            prev_row = row;
            prev_row_start = row_start;
            suggestion
        }))
    }

    fn prev_non_blank_row(&self, mut row: u32) -> Option<u32> {
        while row > 0 {
            row -= 1;
            if !self.is_line_blank(row) {
                return Some(row);
            }
        }
        None
    }
}
