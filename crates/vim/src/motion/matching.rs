use super::*;

fn matching_tag(map: &DisplaySnapshot, head: DisplayPoint) -> Option<DisplayPoint> {
    let inner = crate::object::surrounding_html_tag(map, head, head..head, false)?;
    let outer = crate::object::surrounding_html_tag(map, head, head..head, true)?;

    if head > outer.start && head < inner.start {
        let mut offset = inner.end.to_offset(map, Bias::Left);
        for c in map.buffer_snapshot().chars_at(offset) {
            if c == '/' || c == '\n' || c == '>' {
                return Some(offset.to_display_point(map));
            }
            offset += c.len_utf8();
        }
    } else {
        let mut offset = outer.start.to_offset(map, Bias::Left);
        for c in map.buffer_snapshot().chars_at(offset) {
            offset += c.len_utf8();
            if c == '<' || c == '\n' {
                return Some(offset.to_display_point(map));
            }
        }
    }

    None
}

const BRACKET_PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

fn get_bracket_pair(ch: char) -> Option<(char, char, bool)> {
    for (open, close) in BRACKET_PAIRS {
        if ch == open {
            return Some((open, close, true));
        }
        if ch == close {
            return Some((open, close, false));
        }
    }
    None
}

fn find_matching_bracket_text_based(
    map: &DisplaySnapshot,
    offset: MultiBufferOffset,
    line_range: Range<MultiBufferOffset>,
) -> Option<MultiBufferOffset> {
    let bracket_info = map
        .buffer_chars_at(offset)
        .take_while(|(_, char_offset)| *char_offset < line_range.end)
        .find_map(|(ch, char_offset)| get_bracket_pair(ch).map(|info| (info, char_offset)));

    if bracket_info.is_none() {
        return find_matching_c_preprocessor_directive(map, line_range, offset);
    }

    let (open, close, is_opening) = bracket_info?.0;
    let bracket_offset = bracket_info?.1;

    let mut depth = 0i32;
    if is_opening {
        for (ch, char_offset) in map.buffer_chars_at(bracket_offset) {
            if ch == open {
                depth += 1;
            } else if ch == close {
                depth -= 1;
                if depth == 0 {
                    return Some(char_offset);
                }
            }
        }
    } else {
        for (ch, char_offset) in map.reverse_buffer_chars_at(bracket_offset + close.len_utf8()) {
            if ch == close {
                depth += 1;
            } else if ch == open {
                depth -= 1;
                if depth == 0 {
                    return Some(char_offset);
                }
            }
        }
    }

    None
}

fn find_matching_c_preprocessor_directive(
    map: &DisplaySnapshot,
    line_range: Range<MultiBufferOffset>,
    offset: MultiBufferOffset,
) -> Option<MultiBufferOffset> {
    let line_start = map
        .buffer_chars_at(line_range.start)
        .skip_while(|(c, _)| *c == ' ' || *c == '\t')
        .take_while(|(c, char_offset)| *char_offset < line_range.end && !c.is_whitespace())
        .map(|(c, _)| c)
        .collect::<String>();

    if line_range.start + line_start.len() < offset {
        return None;
    }

    if line_start.starts_with("#if") || line_start.starts_with("#el") {
        let mut depth = 0i32;
        for (ch, char_offset) in map.buffer_chars_at(line_range.end) {
            if ch != '\n' {
                continue;
            }
            let mut line_offset = char_offset + '\n'.len_utf8();

            // Skip leading whitespace
            map.buffer_chars_at(line_offset)
                .take_while(|(c, _)| *c == ' ' || *c == '\t')
                .for_each(|(_, _)| line_offset += 1);

            // Check what directive starts the next line
            let next_line_start = map
                .buffer_chars_at(line_offset)
                .map(|(c, _)| c)
                .take(6)
                .collect::<String>();

            if next_line_start.starts_with("#if") {
                depth += 1;
            } else if next_line_start.starts_with("#endif") {
                if depth > 0 {
                    depth -= 1;
                } else {
                    return Some(line_offset);
                }
            } else if next_line_start.starts_with("#else") || next_line_start.starts_with("#elif") {
                if depth == 0 {
                    return Some(line_offset);
                }
            }
        }
    } else if line_start.starts_with("#endif") {
        let mut depth = 0i32;
        for (ch, char_offset) in
            map.reverse_buffer_chars_at(line_range.start.saturating_sub_usize(1))
        {
            let mut line_offset = if char_offset == MultiBufferOffset(0) {
                MultiBufferOffset(0)
            } else if ch != '\n' {
                continue;
            } else {
                char_offset + '\n'.len_utf8()
            };

            // Skip leading whitespace
            map.buffer_chars_at(line_offset)
                .take_while(|(c, _)| *c == ' ' || *c == '\t')
                .for_each(|(_, _)| line_offset += 1);

            // Check what directive starts this line
            let line_start = map
                .buffer_chars_at(line_offset)
                .skip_while(|(c, _)| *c == ' ' || *c == '\t')
                .map(|(c, _)| c)
                .take(6)
                .collect::<String>();

            if line_start.starts_with("\n\n") {
                // empty line
                continue;
            } else if line_start.starts_with("#endif") {
                depth += 1;
            } else if line_start.starts_with("#if") {
                if depth > 0 {
                    depth -= 1;
                } else {
                    return Some(line_offset);
                }
            }
        }
    }
    None
}

fn comment_delimiter_pair(
    map: &DisplaySnapshot,
    offset: MultiBufferOffset,
) -> Option<(Range<MultiBufferOffset>, Range<MultiBufferOffset>)> {
    let snapshot = map.buffer_snapshot();
    snapshot
        .text_object_ranges(offset..offset, TreeSitterOptions::default())
        .find_map(|(range, obj)| {
            if !matches!(obj, TextObject::InsideComment | TextObject::AroundComment)
                || !range.contains(&offset)
            {
                return None;
            }

            let mut chars = snapshot.chars_at(range.start);
            if (Some('/'), Some('*')) != (chars.next(), chars.next()) {
                return None;
            }

            let open_range = range.start..range.start + 2usize;
            let close_range = range.end - 2..range.end;
            Some((open_range, close_range))
        })
}

pub(super) fn matching(
    map: &DisplaySnapshot,
    display_point: DisplayPoint,
    match_quotes: bool,
) -> DisplayPoint {
    // https://github.com/vim/vim/blob/1d87e11a1ef201b26ed87585fba70182ad0c468a/runtime/doc/motion.txt#L1200
    let display_point = map.clip_at_line_end(display_point);
    let point = display_point.to_point(map);
    let offset = point.to_offset(&map.buffer_snapshot());
    let snapshot = map.buffer_snapshot();

    // Ensure the range is contained by the current line.
    let mut line_end = map.next_line_boundary(point).0;
    let max_point = map.max_point().to_point(map);

    // Only widen to EOF when the cursor is actually at EOF.
    // This avoids expanding a blank current line into start..EOF.
    if line_end == point && point == max_point {
        line_end = max_point;
    }

    let line_range = map.prev_line_boundary(point).0..line_end;
    let line_range = line_range.start.to_offset(&map.buffer_snapshot())
        ..line_range.end.to_offset(&map.buffer_snapshot());

    if let Some(preproc_range) = find_matching_c_preprocessor_directive(map, line_range, offset) {
        return preproc_range.to_display_point(map);
    }

    if let Some((open_range, close_range)) = comment_delimiter_pair(map, offset) {
        if open_range.contains(&offset) {
            return close_range.start.to_display_point(map);
        }

        if close_range.contains(&offset) {
            return open_range.start.to_display_point(map);
        }
    }

    let is_quote_char = |ch: char| matches!(ch, '\'' | '"' | '`');

    // The filter receives buffer-local ranges, not multibuffer offsets.
    let buffer_offset = snapshot
        .point_to_buffer_offset(offset)
        .map(|(_, buffer_offset)| buffer_offset);

    let make_range_filter = |require_on_bracket: bool| {
        move |buffer: &language::BufferSnapshot,
              opening_range: Range<BufferOffset>,
              closing_range: Range<BufferOffset>| {
            if !match_quotes
                && buffer
                    .chars_at(opening_range.start)
                    .next()
                    .is_some_and(is_quote_char)
            {
                return false;
            }

            if require_on_bracket {
                // Attempt to find the smallest enclosing bracket range that also contains
                // the offset, which only happens if the cursor is currently in a bracket.
                buffer_offset.is_some_and(|buffer_offset| {
                    opening_range.contains(&buffer_offset) || closing_range.contains(&buffer_offset)
                })
            } else {
                true
            }
        }
    };

    let bracket_ranges = snapshot
        .innermost_enclosing_bracket_ranges(offset..offset, Some(&make_range_filter(true)))
        .or_else(|| {
            snapshot
                .innermost_enclosing_bracket_ranges(offset..offset, Some(&make_range_filter(false)))
        });

    if let Some((opening_range, closing_range)) = bracket_ranges {
        let mut chars = map.buffer_snapshot().chars_at(offset);
        match chars.next() {
            Some('/') => {}
            _ => {
                if opening_range.contains(&offset) {
                    return closing_range.start.to_display_point(map);
                } else if closing_range.contains(&offset) {
                    return opening_range.start.to_display_point(map);
                }
            }
        }
    }

    let line_range = map.prev_line_boundary(point).0..line_end;
    let visible_line_range =
        line_range.start..Point::new(line_range.end.row, line_range.end.column.saturating_sub(1));
    let line_range = line_range.start.to_offset(&map.buffer_snapshot())
        ..line_range.end.to_offset(&map.buffer_snapshot());
    let ranges = map.buffer_snapshot().bracket_ranges(visible_line_range);
    if let Some(ranges) = ranges {
        let mut closest_pair_destination = None;
        let mut closest_distance = usize::MAX;

        for (open_range, close_range) in ranges {
            if !match_quotes
                && map
                    .buffer_snapshot()
                    .chars_at(open_range.start)
                    .next()
                    .is_some_and(is_quote_char)
            {
                continue;
            }

            if map.buffer_snapshot().chars_at(open_range.start).next() == Some('<') {
                if offset > open_range.start && offset < close_range.start {
                    let mut chars = map.buffer_snapshot().chars_at(close_range.start);
                    if (Some('/'), Some('>')) == (chars.next(), chars.next()) {
                        return display_point;
                    }
                    if let Some(tag) = matching_tag(map, display_point) {
                        return tag;
                    }
                } else if close_range.contains(&offset) {
                    return open_range.start.to_display_point(map);
                } else if open_range.contains(&offset) {
                    return (close_range.end - 1).to_display_point(map);
                }
            }

            if (open_range.contains(&offset) || open_range.start >= offset)
                && line_range.contains(&open_range.start)
            {
                let distance = open_range.start.saturating_sub(offset);
                if distance < closest_distance {
                    closest_pair_destination = Some(close_range.start);
                    closest_distance = distance;
                }
            }

            if (close_range.contains(&offset) || close_range.start >= offset)
                && line_range.contains(&close_range.start)
            {
                let distance = close_range.start.saturating_sub(offset);
                if distance < closest_distance {
                    closest_pair_destination = Some(open_range.start);
                    closest_distance = distance;
                }
            }

            continue;
        }

        closest_pair_destination
            .map(|destination| destination.to_display_point(map))
            .unwrap_or_else(|| {
                find_matching_bracket_text_based(map, offset, line_range.clone())
                    .map(|o| o.to_display_point(map))
                    .unwrap_or(display_point)
            })
    } else {
        find_matching_bracket_text_based(map, offset, line_range)
            .map(|o| o.to_display_point(map))
            .unwrap_or(display_point)
    }
}

