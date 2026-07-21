use super::*;

impl BufferSnapshot {
    pub fn text_object_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
        options: TreeSitterOptions,
    ) -> impl Iterator<Item = (Range<usize>, TextObject)> + '_ {
        let range =
            range.start.to_previous_offset(self)..self.len().min(range.end.to_next_offset(self));

        let mut matches =
            self.syntax
                .matches_with_options(range.clone(), &self.text, options, |grammar| {
                    grammar.text_object_config.as_ref().map(|c| &c.query)
                });

        let configs = matches
            .grammars()
            .iter()
            .map(|grammar| grammar.text_object_config.as_ref())
            .collect::<Vec<_>>();

        let mut captures = Vec::<(Range<usize>, TextObject)>::new();

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
                        .text_objects_by_capture_ix
                        .binary_search_by_key(&capture.index, |e| e.0)
                        .ok()
                    else {
                        continue;
                    };
                    let text_object = config.text_objects_by_capture_ix[ix].1;
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

    /// Returns enclosing bracket ranges containing the given range
    pub fn enclosing_bracket_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> impl Iterator<Item = BracketMatch<usize>> + '_ {
        let range = range.start.to_offset(self)..range.end.to_offset(self);

        let result: Vec<_> = self.bracket_ranges(range.clone()).collect();
        let max_depth = result
            .iter()
            .map(|mat| mat.syntax_layer_depth)
            .max()
            .unwrap_or(0);
        result.into_iter().filter(move |pair| {
            pair.open_range.start <= range.start
                && pair.close_range.end >= range.end
                && pair.syntax_layer_depth == max_depth
        })
    }

    /// Returns the smallest enclosing bracket ranges containing the given range or None if no brackets contain range
    ///
    /// Can optionally pass a range_filter to filter the ranges of brackets to consider
    pub fn innermost_enclosing_bracket_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
        range_filter: Option<&dyn Fn(Range<usize>, Range<usize>) -> bool>,
    ) -> Option<(Range<usize>, Range<usize>)> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);

        // Get the ranges of the innermost pair of brackets.
        let mut result: Option<(Range<usize>, Range<usize>)> = None;

        for pair in self.enclosing_bracket_ranges(range) {
            if let Some(range_filter) = range_filter
                && !range_filter(pair.open_range.clone(), pair.close_range.clone())
            {
                continue;
            }

            let len = pair.close_range.end - pair.open_range.start;

            if let Some((existing_open, existing_close)) = &result {
                let existing_len = existing_close.end - existing_open.start;
                if len > existing_len {
                    continue;
                }
            }

            result = Some((pair.open_range, pair.close_range));
        }

        result
    }

    /// Returns anchor ranges for any matches of the redaction query.
    /// The buffer can be associated with multiple languages, and the redaction query associated with each
    /// will be run on the relevant section of the buffer.
    pub fn redacted_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> impl Iterator<Item = Range<usize>> + '_ {
        let offset_range = range.start.to_offset(self)..range.end.to_offset(self);
        let mut syntax_matches = self.syntax.matches(offset_range, self, |grammar| {
            grammar
                .redactions_config
                .as_ref()
                .map(|config| &config.query)
        });

        let configs = syntax_matches
            .grammars()
            .iter()
            .map(|grammar| grammar.redactions_config.as_ref())
            .collect::<Vec<_>>();

        iter::from_fn(move || {
            let redacted_range = syntax_matches
                .peek()
                .and_then(|mat| {
                    configs[mat.grammar_index].and_then(|config| {
                        mat.captures
                            .iter()
                            .find(|capture| capture.index == config.redaction_capture_ix)
                    })
                })
                .map(|mat| mat.node.byte_range());
            syntax_matches.advance();
            redacted_range
        })
    }

    pub fn injections_intersecting_range<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> impl Iterator<Item = (Range<usize>, &Arc<Language>)> + '_ {
        let offset_range = range.start.to_offset(self)..range.end.to_offset(self);

        let mut syntax_matches = self.syntax.matches(offset_range, self, |grammar| {
            grammar
                .injection_config
                .as_ref()
                .map(|config| &config.query)
        });

        let configs = syntax_matches
            .grammars()
            .iter()
            .map(|grammar| grammar.injection_config.as_ref())
            .collect::<Vec<_>>();

        iter::from_fn(move || {
            let ranges = syntax_matches.peek().and_then(|mat| {
                let config = &configs[mat.grammar_index]?;
                let content_capture_range = mat.captures.iter().find_map(|capture| {
                    if capture.index == config.content_capture_ix {
                        Some(capture.node.byte_range())
                    } else {
                        None
                    }
                })?;
                let language = self.language_at(content_capture_range.start)?;
                Some((content_capture_range, language))
            });
            syntax_matches.advance();
            ranges
        })
    }
}
