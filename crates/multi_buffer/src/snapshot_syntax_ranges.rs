use super::*;

impl MultiBufferSnapshot {
    /// Returns the smallest enclosing bracket ranges containing the given range or
    /// None if no brackets contain range or the range is not contained in a single
    /// excerpt
    ///
    /// Can optionally pass a range_filter to filter the ranges of brackets to consider
    #[ztracing::instrument(skip_all)]
    pub fn innermost_enclosing_bracket_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
        range_filter: Option<
            &dyn Fn(&BufferSnapshot, Range<BufferOffset>, Range<BufferOffset>) -> bool,
        >,
    ) -> Option<(Range<MultiBufferOffset>, Range<MultiBufferOffset>)> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let results =
            self.map_excerpt_ranges(range, |buffer, excerpt_range, input_buffer_range| {
                let filter = |open: Range<usize>, close: Range<usize>| -> bool {
                    excerpt_range.context.start.0 <= open.start
                        && close.end <= excerpt_range.context.end.0
                        && range_filter.is_none_or(|filter| {
                            filter(
                                buffer,
                                BufferOffset(open.start)..BufferOffset(close.end),
                                BufferOffset(close.start)..BufferOffset(close.end),
                            )
                        })
                };
                let Some((open, close)) =
                    buffer.innermost_enclosing_bracket_ranges(input_buffer_range, Some(&filter))
                else {
                    return Vec::new();
                };
                vec![
                    (BufferOffset(open.start)..BufferOffset(open.end), ()),
                    (BufferOffset(close.start)..BufferOffset(close.end), ()),
                ]
            })?;
        let [(open, _), (close, _)] = results.try_into().ok()?;
        Some((open, close))
    }

    /// Returns enclosing bracket ranges containing the given range or returns None if the range is
    /// not contained in a single excerpt
    pub fn enclosing_bracket_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> Option<impl Iterator<Item = (Range<MultiBufferOffset>, Range<MultiBufferOffset>)>> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let results =
            self.map_excerpt_ranges(range, |buffer, excerpt_range, input_buffer_range| {
                buffer
                    .enclosing_bracket_ranges(input_buffer_range)
                    .filter(|pair| {
                        excerpt_range.context.start.0 <= pair.open_range.start
                            && pair.close_range.end <= excerpt_range.context.end.0
                    })
                    .flat_map(|pair| {
                        [
                            (
                                BufferOffset(pair.open_range.start)
                                    ..BufferOffset(pair.open_range.end),
                                (),
                            ),
                            (
                                BufferOffset(pair.close_range.start)
                                    ..BufferOffset(pair.close_range.end),
                                (),
                            ),
                        ]
                    })
                    .collect()
            })?;
        Some(results.into_iter().map(|(range, _)| range).tuples())
    }

    /// Returns enclosing bracket ranges containing the given range or returns None if the range is
    /// not contained in a single excerpt
    pub fn text_object_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
        options: TreeSitterOptions,
    ) -> impl Iterator<Item = (Range<MultiBufferOffset>, TextObject)> + '_ {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        self.map_excerpt_ranges(range, |buffer, excerpt_range, input_buffer_range| {
            buffer
                .text_object_ranges(input_buffer_range, options)
                .filter(|(range, _)| {
                    excerpt_range.context.start.0 <= range.start
                        && range.end <= excerpt_range.context.end.0
                })
                .map(|(range, text_object)| {
                    (
                        BufferOffset(range.start)..BufferOffset(range.end),
                        text_object,
                    )
                })
                .collect()
        })
        .into_iter()
        .flatten()
    }

    pub fn bracket_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> Option<impl Iterator<Item = (Range<MultiBufferOffset>, Range<MultiBufferOffset>)>> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let results =
            self.map_excerpt_ranges(range, |buffer, excerpt_range, input_buffer_range| {
                buffer
                    .bracket_ranges(input_buffer_range)
                    .filter(|pair| {
                        excerpt_range.context.start.0 <= pair.open_range.start
                            && pair.close_range.end <= excerpt_range.context.end.0
                    })
                    .flat_map(|pair| {
                        [
                            (
                                BufferOffset(pair.open_range.start)
                                    ..BufferOffset(pair.open_range.end),
                                (),
                            ),
                            (
                                BufferOffset(pair.close_range.start)
                                    ..BufferOffset(pair.close_range.end),
                                (),
                            ),
                        ]
                    })
                    .collect()
            })?;
        Some(results.into_iter().map(|(range, _)| range).tuples())
    }

    pub fn redacted_ranges<'a, T: ToOffset>(
        &'a self,
        range: Range<T>,
        redaction_enabled: impl Fn(Option<&Arc<dyn File>>) -> bool + 'a,
    ) -> impl Iterator<Item = Range<MultiBufferOffset>> + 'a {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        self.lift_buffer_metadata(range, move |buffer, range| {
            if redaction_enabled(buffer.file()) {
                Some(buffer.redacted_ranges(range).map(|range| (range, ())))
            } else {
                None
            }
        })
        .map(|(range, _, _)| range)
    }

    pub fn runnable_ranges(
        &self,
        range: Range<Anchor>,
    ) -> impl Iterator<Item = (Range<Anchor>, language::RunnableRange)> + '_ {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        self.lift_buffer_metadata(range, move |buffer, range| {
            Some(
                buffer
                    .runnable_ranges(range.clone())
                    .filter(move |runnable| {
                        runnable.run_range.start >= range.start
                            && runnable.run_range.end < range.end
                    })
                    .map(|runnable| (runnable.run_range.clone(), runnable)),
            )
        })
        .map(|(run_range, runnable, _)| {
            (
                self.anchor_after(run_range.start)..self.anchor_before(run_range.end),
                runnable,
            )
        })
    }
}
