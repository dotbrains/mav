use super::rendered_line::RenderedLine;
use super::*;

pub(super) struct RenderedMarkdown {
    element: AnyElement,
    text: RenderedText,
}

#[derive(Clone)]
pub(super) struct RenderedText {
    lines: Rc<[RenderedLine]>,
    links: Rc<[RenderedLink]>,
    footnote_refs: Rc<[RenderedFootnoteRef]>,
}

struct WrappedLineSegment {
    start: usize,
    end: usize,
    row_top: Pixels,
    layout: Arc<WrappedLineLayout>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct RenderedLink {
    pub(super) source_range: Range<usize>,
    pub(super) destination_url: SharedString,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct RenderedFootnoteRef {
    pub(super) source_range: Range<usize>,
    pub(super) label: SharedString,
}

impl RenderedText {
    pub(super) fn bounds_for_source_range(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        self.bounds_for_sorted_source_ranges([(0, range)])
            .into_iter()
            .map(|(_, bounds)| bounds)
            .collect()
    }

    pub(super) fn bounds_for_sorted_source_ranges(
        &self,
        ranges: impl IntoIterator<Item = (usize, Range<usize>)>,
    ) -> Vec<(usize, Bounds<Pixels>)> {
        let ranges = ranges.into_iter().collect::<Vec<_>>();
        let mut all_bounds = Vec::new();
        let mut first_possible_range_ix = 0;

        for line in self.lines.iter() {
            let line_source_start = line.source_mappings.first().unwrap().source_index;
            while ranges
                .get(first_possible_range_ix)
                .is_some_and(|(_, range)| range.end <= line_source_start)
            {
                first_possible_range_ix += 1;
            }

            let Some((_, first_possible_range)) = ranges.get(first_possible_range_ix) else {
                break;
            };
            if first_possible_range.start >= line.source_end {
                continue;
            }

            let wrapped_line_segments = Self::wrapped_line_segments(line);
            if wrapped_line_segments.is_empty() {
                continue;
            }

            let mut range_ix = first_possible_range_ix;
            while let Some((highlight_ix, range)) = ranges.get(range_ix) {
                if range.start >= line.source_end {
                    break;
                }
                Self::push_bounds_for_line_source_range(
                    &mut all_bounds,
                    *highlight_ix,
                    line,
                    &wrapped_line_segments,
                    range.start.max(line_source_start)..range.end.min(line.source_end),
                );
                range_ix += 1;
            }
        }

        all_bounds
    }

    fn wrapped_line_segments(line: &RenderedLine) -> SmallVec<[WrappedLineSegment; 1]> {
        let layout = &line.layout;
        let line_height = layout.line_height();
        let mut row_top = layout.bounds().top();
        let mut wrapped_line_start = 0;
        let mut segments = SmallVec::new();

        for wrapped_line in layout.line_layouts() {
            let wrapped_line_end = wrapped_line_start + wrapped_line.len();
            let wrapped_line_height = wrapped_line.size(line_height).height;
            segments.push(WrappedLineSegment {
                start: wrapped_line_start,
                end: wrapped_line_end,
                row_top,
                layout: wrapped_line,
            });
            row_top += wrapped_line_height;
            wrapped_line_start = wrapped_line_end + 1;
        }

        segments
    }

    fn push_bounds_for_line_source_range(
        all_bounds: &mut Vec<(usize, Bounds<Pixels>)>,
        highlight_ix: usize,
        line: &RenderedLine,
        wrapped_line_segments: &[WrappedLineSegment],
        range: Range<usize>,
    ) {
        if range.start >= range.end {
            return;
        }

        let layout = &line.layout;
        let line_bounds = layout.bounds();
        let line_height = layout.line_height();

        let rendered_start = line.rendered_index_for_source_index(range.start);
        let rendered_end = line.rendered_index_for_source_index(range.end);

        for wrapped_line_segment in wrapped_line_segments {
            if wrapped_line_segment.start >= rendered_end {
                break;
            }
            if wrapped_line_segment.end <= rendered_start {
                continue;
            }

            let wrapped_line = &wrapped_line_segment.layout;
            let unwrapped_layout = &wrapped_line.unwrapped_layout;
            let wrapped_line_start = wrapped_line_segment.start;
            let wrapped_line_end = wrapped_line_segment.end;
            let mut row_top = wrapped_line_segment.row_top;

            let row_ends = wrapped_line
                .wrap_boundaries()
                .iter()
                .map(|wrap_boundary| {
                    let glyph =
                        &unwrapped_layout.runs[wrap_boundary.run_ix].glyphs[wrap_boundary.glyph_ix];
                    (wrapped_line_start + glyph.index, glyph.position.x)
                })
                .chain([(wrapped_line_end, unwrapped_layout.width)]);

            let mut row_start = wrapped_line_start;
            let mut row_start_x = Pixels::ZERO;

            for (row_end, row_end_x) in row_ends {
                let selection_start = rendered_start.max(row_start);
                let selection_end = rendered_end.min(row_end);

                if selection_start < selection_end {
                    let alignment_offset = line.alignment_offset_for_segment(
                        line_bounds.size.width,
                        row_start_x,
                        row_end_x,
                    );
                    let x_for_index = |index| {
                        line_bounds.left()
                            + alignment_offset
                            + unwrapped_layout.x_for_index(index - wrapped_line_start)
                            - row_start_x
                    };
                    all_bounds.push((
                        highlight_ix,
                        Bounds::from_corners(
                            point(x_for_index(selection_start), row_top),
                            point(x_for_index(selection_end), row_top + line_height),
                        ),
                    ));
                }

                row_start = row_end;
                row_start_x = row_end_x;
                row_top += line_height;
            }
        }
    }

    pub(super) fn source_index_for_position(
        &self,
        position: Point<Pixels>,
    ) -> Result<usize, usize> {
        let mut lines = self.lines.iter().peekable();
        let mut fallback_line: Option<&RenderedLine> = None;

        while let Some(line) = lines.next() {
            let line_bounds = line.layout.bounds();

            // Exact match: position is within bounds (handles overlapping bounds like table columns)
            if line_bounds.contains(&position) {
                return line.source_index_for_position(position);
            }

            // Track fallback for Y-coordinate based matching
            if position.y <= line_bounds.bottom() && fallback_line.is_none() {
                fallback_line = Some(line);
            }

            // Handle gap between lines
            if position.y > line_bounds.bottom() {
                if let Some(next_line) = lines.peek()
                    && position.y < next_line.layout.bounds().top()
                {
                    return Err(line.source_end);
                }
            }
        }

        // Fall back to Y-coordinate matched line
        if let Some(line) = fallback_line {
            return line.source_index_for_position(position);
        }

        Err(self.lines.last().map_or(0, |line| line.source_end))
    }

    pub(super) fn position_for_source_index(
        &self,
        source_index: usize,
    ) -> Option<(Point<Pixels>, Pixels)> {
        for line in self.lines.iter() {
            let line_source_start = line.source_mappings.first().unwrap().source_index;
            if source_index < line_source_start {
                break;
            } else if source_index > line.source_end {
                continue;
            } else {
                let line_height = line.layout.line_height();
                let rendered_index_within_line = line.rendered_index_for_source_index(source_index);
                let position = line.layout.position_for_index(rendered_index_within_line)?;
                return Some((position, line_height));
            }
        }
        None
    }

    pub(super) fn surrounding_word_range(&self, source_index: usize) -> Range<usize> {
        for line in self.lines.iter() {
            if source_index > line.source_end {
                continue;
            }

            let line_rendered_start = line.source_mappings.first().unwrap().rendered_index;
            let rendered_index_in_line =
                line.rendered_index_for_source_index(source_index) - line_rendered_start;
            let text = line.layout.text();

            let scope = line.language.as_ref().map(|l| l.default_scope());
            let classifier = CharClassifier::new(scope);

            let mut prev_chars = text[..rendered_index_in_line].chars().rev().peekable();
            let mut next_chars = text[rendered_index_in_line..].chars().peekable();

            let word_kind = std::cmp::max(
                prev_chars.peek().map(|&c| classifier.kind(c)),
                next_chars.peek().map(|&c| classifier.kind(c)),
            );

            let mut start = rendered_index_in_line;
            for c in prev_chars {
                if Some(classifier.kind(c)) == word_kind {
                    start -= c.len_utf8();
                } else {
                    break;
                }
            }

            let mut end = rendered_index_in_line;
            for c in next_chars {
                if Some(classifier.kind(c)) == word_kind {
                    end += c.len_utf8();
                } else {
                    break;
                }
            }

            return line.source_index_for_rendered_index(line_rendered_start + start)
                ..line.source_index_for_exclusive_rendered_end(line_rendered_start + end);
        }

        source_index..source_index
    }

    pub(super) fn surrounding_line_range(&self, source_index: usize) -> Range<usize> {
        for line in self.lines.iter() {
            if source_index > line.source_end {
                continue;
            }
            let line_source_start = line.source_mappings.first().unwrap().source_index;
            return line_source_start..line.source_end;
        }

        source_index..source_index
    }

    pub(super) fn text_for_range(&self, range: Range<usize>) -> String {
        let mut accumulator = String::new();

        for line in self.lines.iter() {
            if range.start > line.source_end {
                continue;
            }
            let line_source_start = line.source_mappings.first().unwrap().source_index;
            if range.end < line_source_start {
                break;
            }

            let text = line.layout.text();

            let start = if range.start < line_source_start {
                0
            } else {
                line.rendered_index_for_source_index(range.start)
            };
            let end = if range.end > line.source_end {
                line.rendered_index_for_source_index(line.source_end)
            } else {
                line.rendered_index_for_source_index(range.end)
            }
            .min(text.len());

            accumulator.push_str(&text[start..end]);
            accumulator.push('\n');
        }
        // Remove trailing newline
        accumulator.pop();
        accumulator
    }

    pub(super) fn link_for_source_index(&self, source_index: usize) -> Option<&RenderedLink> {
        self.links
            .iter()
            .find(|link| link.source_range.contains(&source_index))
    }

    pub(super) fn footnote_ref_for_source_index(
        &self,
        source_index: usize,
    ) -> Option<&RenderedFootnoteRef> {
        self.footnote_refs
            .iter()
            .find(|fref| fref.source_range.contains(&source_index))
    }
}
