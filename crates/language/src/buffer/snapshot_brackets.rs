use super::*;

impl BufferSnapshot {
    /// For each grammar in the language, runs the provided
    /// [`tree_sitter::Query`] against the given range.
    pub fn matches(
        &self,
        range: Range<usize>,
        query: fn(&Grammar) -> Option<&tree_sitter::Query>,
    ) -> SyntaxMapMatches<'_> {
        self.syntax.matches(range, self, query)
    }

    /// Finds all [`RowChunks`] applicable to the given range, then returns all bracket pairs that intersect with those chunks.
    /// Hence, may return more bracket pairs than the range contains.
    ///
    /// Will omit known chunks.
    /// The resulting bracket match collections are not ordered.
    pub fn fetch_bracket_ranges(
        &self,
        range: Range<usize>,
        known_chunks: Option<&HashSet<Range<BufferRow>>>,
    ) -> HashMap<Range<BufferRow>, Vec<BracketMatch<usize>>> {
        let mut all_bracket_matches = HashMap::default();

        for chunk in self
            .tree_sitter_data
            .chunks
            .applicable_chunks(&[range.to_point(self)])
        {
            if known_chunks.is_some_and(|chunks| chunks.contains(&chunk.row_range())) {
                continue;
            }
            let chunk_range = chunk.anchor_range();
            let chunk_range = chunk_range.to_offset(&self);

            if let Some(cached_brackets) =
                &self.tree_sitter_data.brackets_by_chunks.lock()[chunk.id]
            {
                all_bracket_matches.insert(chunk.row_range(), cached_brackets.clone());
                continue;
            }

            let mut all_brackets: Vec<(BracketMatch<usize>, usize, bool)> = Vec::new();
            let mut opens = Vec::new();
            let mut color_pairs = Vec::new();

            let mut matches = self.syntax.matches_with_options(
                chunk_range.clone(),
                &self.text,
                TreeSitterOptions {
                    max_bytes_to_query: Some(MAX_BYTES_TO_QUERY),
                    max_start_depth: None,
                },
                |grammar| grammar.brackets_config.as_ref().map(|c| &c.query),
            );
            let configs = matches
                .grammars()
                .iter()
                .map(|grammar| grammar.brackets_config.as_ref().unwrap())
                .collect::<Vec<_>>();

            // Group matches by open range so we can either trust grammar output
            // or repair it by picking a single closest close per open.
            let mut open_to_close_ranges = BTreeMap::new();
            while let Some(mat) = matches.peek() {
                let mut open = None;
                let mut close = None;
                let syntax_layer_depth = mat.depth;
                let pattern_index = mat.pattern_index;
                let config = configs[mat.grammar_index];
                let pattern = &config.patterns[pattern_index];
                for capture in mat.captures {
                    if capture.index == config.open_capture_ix {
                        open = Some(capture.node.byte_range());
                    } else if capture.index == config.close_capture_ix {
                        close = Some(capture.node.byte_range());
                    }
                }

                matches.advance();

                let Some((open_range, close_range)) = open.zip(close) else {
                    continue;
                };

                let bracket_range = open_range.start..=close_range.end;
                if !bracket_range.overlaps(&chunk_range) {
                    continue;
                }

                open_to_close_ranges
                    .entry((open_range.start, open_range.end, pattern_index))
                    .or_insert_with(BTreeMap::new)
                    .insert(
                        (close_range.start, close_range.end),
                        BracketMatch {
                            open_range: open_range.clone(),
                            close_range: close_range.clone(),
                            syntax_layer_depth,
                            newline_only: pattern.newline_only,
                            color_index: None,
                        },
                    );

                all_brackets.push((
                    BracketMatch {
                        open_range,
                        close_range,
                        syntax_layer_depth,
                        newline_only: pattern.newline_only,
                        color_index: None,
                    },
                    pattern_index,
                    pattern.rainbow_exclude,
                ));
            }

            let has_bogus_matches = open_to_close_ranges
                .iter()
                .any(|(_, end_ranges)| end_ranges.len() > 1);
            if has_bogus_matches {
                // Grammar is producing bogus matches where one open is paired with multiple
                // closes. Build a valid stack by walking through positions in order.
                // For each close, we know the expected open_len from tree-sitter matches.

                // Map each close to its expected open length (for inferring opens)
                let close_to_open_len: HashMap<(usize, usize, usize), usize> = all_brackets
                    .iter()
                    .map(|(bracket_match, pattern_index, _)| {
                        (
                            (
                                bracket_match.close_range.start,
                                bracket_match.close_range.end,
                                *pattern_index,
                            ),
                            bracket_match.open_range.len(),
                        )
                    })
                    .collect();

                // Collect unique opens and closes within this chunk
                let mut unique_opens: HashSet<(usize, usize, usize)> = all_brackets
                    .iter()
                    .map(|(bracket_match, pattern_index, _)| {
                        (
                            bracket_match.open_range.start,
                            bracket_match.open_range.end,
                            *pattern_index,
                        )
                    })
                    .filter(|(start, _, _)| chunk_range.contains(start))
                    .collect();

                let mut unique_closes: Vec<(usize, usize, usize)> = all_brackets
                    .iter()
                    .map(|(bracket_match, pattern_index, _)| {
                        (
                            bracket_match.close_range.start,
                            bracket_match.close_range.end,
                            *pattern_index,
                        )
                    })
                    .filter(|(start, _, _)| chunk_range.contains(start))
                    .collect();
                unique_closes.sort_unstable();
                unique_closes.dedup();

                // Build valid pairs by walking through closes in order
                let mut unique_opens_vec: Vec<_> = unique_opens.iter().copied().collect();
                unique_opens_vec.sort();

                let mut valid_pairs: HashSet<((usize, usize, usize), (usize, usize, usize))> =
                    HashSet::default();
                let mut open_stacks: HashMap<usize, Vec<(usize, usize)>> = HashMap::default();
                let mut open_idx = 0;

                for close in &unique_closes {
                    // Push all opens before this close onto stack
                    while open_idx < unique_opens_vec.len()
                        && unique_opens_vec[open_idx].0 < close.0
                    {
                        let (start, end, pattern_index) = unique_opens_vec[open_idx];
                        open_stacks
                            .entry(pattern_index)
                            .or_default()
                            .push((start, end));
                        open_idx += 1;
                    }

                    // Try to match with most recent open
                    let (close_start, close_end, pattern_index) = *close;
                    if let Some(open) = open_stacks
                        .get_mut(&pattern_index)
                        .and_then(|open_stack| open_stack.pop())
                    {
                        valid_pairs.insert(((open.0, open.1, pattern_index), *close));
                    } else if let Some(&open_len) = close_to_open_len.get(close) {
                        // No open on stack - infer one based on expected open_len
                        if close_start >= open_len {
                            let inferred = (close_start - open_len, close_start, pattern_index);
                            unique_opens.insert(inferred);
                            valid_pairs.insert((inferred, *close));
                            all_brackets.push((
                                BracketMatch {
                                    open_range: inferred.0..inferred.1,
                                    close_range: close_start..close_end,
                                    newline_only: false,
                                    syntax_layer_depth: 0,
                                    color_index: None,
                                },
                                pattern_index,
                                false,
                            ));
                        }
                    }
                }

                all_brackets.retain(|(bracket_match, pattern_index, _)| {
                    let open = (
                        bracket_match.open_range.start,
                        bracket_match.open_range.end,
                        *pattern_index,
                    );
                    let close = (
                        bracket_match.close_range.start,
                        bracket_match.close_range.end,
                        *pattern_index,
                    );
                    valid_pairs.contains(&(open, close))
                });
            }

            let mut all_brackets = all_brackets
                .into_iter()
                .enumerate()
                .map(|(index, (bracket_match, _, rainbow_exclude))| {
                    // Certain languages have "brackets" that are not brackets, e.g. tags. and such
                    // bracket will match the entire tag with all text inside.
                    // For now, avoid highlighting any pair that has more than single char in each bracket.
                    // We need to  colorize `<Element/>` bracket pairs, so cannot make this check stricter.
                    let should_color = !rainbow_exclude
                        && (bracket_match.open_range.len() == 1
                            || bracket_match.close_range.len() == 1);
                    if should_color {
                        opens.push(bracket_match.open_range.clone());
                        color_pairs.push((
                            bracket_match.open_range.clone(),
                            bracket_match.close_range.clone(),
                            index,
                        ));
                    }
                    bracket_match
                })
                .collect::<Vec<_>>();

            opens.sort_by_key(|r| (r.start, r.end));
            opens.dedup_by(|a, b| a.start == b.start && a.end == b.end);
            color_pairs.sort_by_key(|(_, close, _)| close.end);

            let mut open_stack = Vec::new();
            let mut open_index = 0;
            for (open, close, index) in color_pairs {
                while open_index < opens.len() && opens[open_index].start < close.start {
                    open_stack.push(opens[open_index].clone());
                    open_index += 1;
                }

                if open_stack.last() == Some(&open) {
                    let depth_index = open_stack.len() - 1;
                    all_brackets[index].color_index = Some(depth_index);
                    open_stack.pop();
                }
            }

            all_brackets.sort_by_key(|bracket_match| {
                (bracket_match.open_range.start, bracket_match.open_range.end)
            });

            if let empty_slot @ None =
                &mut self.tree_sitter_data.brackets_by_chunks.lock()[chunk.id]
            {
                *empty_slot = Some(all_brackets.clone());
            }
            all_bracket_matches.insert(chunk.row_range(), all_brackets);
        }

        all_bracket_matches
    }

    pub fn all_bracket_ranges(
        &self,
        range: Range<usize>,
    ) -> impl Iterator<Item = BracketMatch<usize>> {
        self.fetch_bracket_ranges(range.clone(), None)
            .into_values()
            .flatten()
            .filter(move |bracket_match| {
                let bracket_range = bracket_match.open_range.start..bracket_match.close_range.end;
                bracket_range.overlaps(&range)
            })
    }

    /// Returns bracket range pairs overlapping or adjacent to `range`
    pub fn bracket_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> impl Iterator<Item = BracketMatch<usize>> + '_ {
        // Find bracket pairs that *inclusively* contain the given range.
        let range = range.start.to_previous_offset(self)..range.end.to_next_offset(self);
        self.all_bracket_ranges(range)
            .filter(|pair| !pair.newline_only)
    }

    pub fn debug_variables_query<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> impl Iterator<Item = (Range<usize>, DebuggerTextObject)> + '_ {
        let range = range.start.to_previous_offset(self)..range.end.to_next_offset(self);

        let mut matches = self.syntax.matches_with_options(
            range.clone(),
            &self.text,
            TreeSitterOptions::default(),
            |grammar| grammar.debug_variables_config.as_ref().map(|c| &c.query),
        );

        let configs = matches
            .grammars()
            .iter()
            .map(|grammar| grammar.debug_variables_config.as_ref())
            .collect::<Vec<_>>();

        let mut captures = Vec::<(Range<usize>, DebuggerTextObject)>::new();

        iter::from_fn(move || {
            loop {
                while let Some(capture) = captures.pop() {
                    if capture.0.overlaps(&range) {
                        return Some(capture);
                    }
                }

                let mat = matches.peek()?;

                let Some(config) = configs[mat.grammar_index].as_ref() else {
                    matches.advance();
                    continue;
                };

                for capture in mat.captures {
                    let Some(ix) = config
                        .objects_by_capture_ix
                        .binary_search_by_key(&capture.index, |e| e.0)
                        .ok()
                    else {
                        continue;
                    };
                    let text_object = config.objects_by_capture_ix[ix].1;
                    let byte_range = capture.node.byte_range();

                    let mut found = false;
                    for (range, existing) in captures.iter_mut() {
                        if existing == &text_object {
                            range.start = range.start.min(byte_range.start);
                            range.end = range.end.max(byte_range.end);
                            found = true;
                            break;
                        }
                    }

                    if !found {
                        captures.push((byte_range, text_object));
                    }
                }

                matches.advance();
            }
        })
    }
}
