use super::*;

#[derive(Debug, Clone)]
pub(super) struct CandidateRange {
    start: DisplayPoint,
    end: DisplayPoint,
}

#[derive(Debug, Clone)]
pub(super) struct CandidateWithRanges {
    candidate: CandidateRange,
    open_range: Range<MultiBufferOffset>,
    close_range: Range<MultiBufferOffset>,
}

/// Operates on text within or around parentheses `()`.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct Parentheses {
    #[serde(default)]
    pub(super) opening: bool,
}

/// Operates on text within or around square brackets `[]`.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct SquareBrackets {
    #[serde(default)]
    pub(super) opening: bool,
}

/// Operates on text within or around angle brackets `<>`.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct AngleBrackets {
    #[serde(default)]
    pub(super) opening: bool,
}

/// Operates on text within or around curly brackets `{}`.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct CurlyBrackets {
    #[serde(default)]
    pub(super) opening: bool,
}

fn cover_or_next<I: Iterator<Item = (Range<MultiBufferOffset>, Range<MultiBufferOffset>)>>(
    candidates: Option<I>,
    caret: DisplayPoint,
    map: &DisplaySnapshot,
) -> Option<CandidateWithRanges> {
    let caret_offset = caret.to_offset(map, Bias::Left);
    let mut covering = vec![];
    let mut next_ones = vec![];
    let snapshot = map.buffer_snapshot();

    if let Some(ranges) = candidates {
        for (open_range, close_range) in ranges {
            let start_off = open_range.start;
            let end_off = close_range.end;
            let candidate = CandidateWithRanges {
                candidate: CandidateRange {
                    start: start_off.to_display_point(map),
                    end: end_off.to_display_point(map),
                },
                open_range: open_range.clone(),
                close_range: close_range.clone(),
            };

            if open_range
                .start
                .to_offset(snapshot)
                .to_display_point(map)
                .row()
                == caret_offset.to_display_point(map).row()
            {
                if start_off <= caret_offset && caret_offset < end_off {
                    covering.push(candidate);
                } else if start_off >= caret_offset {
                    next_ones.push(candidate);
                }
            }
        }
    }

    // 1) covering -> smallest width
    if !covering.is_empty() {
        return covering.into_iter().min_by_key(|r| {
            r.candidate.end.to_offset(map, Bias::Right)
                - r.candidate.start.to_offset(map, Bias::Left)
        });
    }

    // 2) next -> closest by start
    if !next_ones.is_empty() {
        return next_ones.into_iter().min_by_key(|r| {
            let start = r.candidate.start.to_offset(map, Bias::Left);
            (start.0 as isize - caret_offset.0 as isize).abs()
        });
    }

    None
}

type DelimiterPredicate = dyn Fn(&BufferSnapshot, usize, usize) -> bool;

struct DelimiterRange {
    open: Range<MultiBufferOffset>,
    close: Range<MultiBufferOffset>,
}

impl DelimiterRange {
    fn to_display_range(&self, map: &DisplaySnapshot, around: bool) -> Range<DisplayPoint> {
        if around {
            self.open.start.to_display_point(map)..self.close.end.to_display_point(map)
        } else {
            self.open.end.to_display_point(map)..self.close.start.to_display_point(map)
        }
    }
}

fn find_mini_delimiters(
    map: &DisplaySnapshot,
    display_point: DisplayPoint,
    around: bool,
    is_valid_delimiter: &DelimiterPredicate,
) -> Option<Range<DisplayPoint>> {
    let point = map.clip_at_line_end(display_point).to_point(map);
    let offset = map.buffer_snapshot().point_to_offset(point);

    let line_range = get_line_range(map, point);
    let visible_line_range = get_visible_line_range(&line_range);

    let snapshot = &map.buffer_snapshot();

    let ranges = map
        .buffer_snapshot()
        .bracket_ranges(visible_line_range)
        .map(|ranges| {
            ranges.filter_map(|(open, close)| {
                let (buffer, buffer_open) =
                    snapshot.range_to_buffer_range::<MultiBufferOffset>(open.clone())?;
                let (_, buffer_close) =
                    snapshot.range_to_buffer_range::<MultiBufferOffset>(close.clone())?;

                if is_valid_delimiter(buffer, buffer_open.start, buffer_close.start) {
                    Some((open, close))
                } else {
                    None
                }
            })
        });

    if let Some(candidate) = cover_or_next(ranges, display_point, map) {
        return Some(
            DelimiterRange {
                open: candidate.open_range,
                close: candidate.close_range,
            }
            .to_display_range(map, around),
        );
    }

    let results = snapshot.map_excerpt_ranges(offset..offset, |buffer, _, input_range| {
        let buffer_offset = input_range.start.0;
        let bracket_filter = |open: Range<usize>, close: Range<usize>| {
            is_valid_delimiter(buffer, open.start, close.start)
        };
        let Some((open, close)) = buffer.innermost_enclosing_bracket_ranges(
            buffer_offset..buffer_offset,
            Some(&bracket_filter),
        ) else {
            return vec![];
        };
        vec![
            (BufferOffset(open.start)..BufferOffset(open.end), ()),
            (BufferOffset(close.start)..BufferOffset(close.end), ()),
        ]
    })?;

    if results.len() < 2 {
        return None;
    }

    Some(
        DelimiterRange {
            open: results[0].0.clone(),
            close: results[1].0.clone(),
        }
        .to_display_range(map, around),
    )
}

fn get_line_range(map: &DisplaySnapshot, point: Point) -> Range<Point> {
    let (start, mut end) = (
        map.prev_line_boundary(point).0,
        map.next_line_boundary(point).0,
    );

    if end == point {
        end = map.max_point().to_point(map);
    }

    start..end
}

fn get_visible_line_range(line_range: &Range<Point>) -> Range<Point> {
    let end_column = line_range.end.column.saturating_sub(1);
    line_range.start..Point::new(line_range.end.row, end_column)
}

fn is_quote_delimiter(buffer: &BufferSnapshot, _start: usize, end: usize) -> bool {
    matches!(buffer.chars_at(end).next(), Some('\'' | '"' | '`'))
}

fn is_bracket_delimiter(buffer: &BufferSnapshot, start: usize, _end: usize) -> bool {
    matches!(
        buffer.chars_at(start).next(),
        Some('(' | '[' | '{' | '<' | '|')
    )
}

pub(super) fn find_mini_quotes(
    map: &DisplaySnapshot,
    display_point: DisplayPoint,
    around: bool,
) -> Option<Range<DisplayPoint>> {
    find_mini_delimiters(map, display_point, around, &is_quote_delimiter)
}

pub(super) fn find_mini_brackets(
    map: &DisplaySnapshot,
    display_point: DisplayPoint,
    around: bool,
) -> Option<Range<DisplayPoint>> {
    find_mini_delimiters(map, display_point, around, &is_bracket_delimiter)
}
