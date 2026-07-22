use super::*;

fn entire_file(map: &DisplaySnapshot) -> Option<Range<DisplayPoint>> {
    Some(DisplayPoint::zero()..map.max_point())
}

fn text_object(
    map: &DisplaySnapshot,
    relative_to: DisplayPoint,
    target: TextObject,
) -> Option<Range<DisplayPoint>> {
    let snapshot = &map.buffer_snapshot();
    let offset = relative_to.to_offset(map, Bias::Left);

    let results =
        snapshot.map_excerpt_ranges(offset..offset, |buffer, _excerpt_range, buffer_range| {
            let buffer_offset = buffer_range.start;

            let mut matches: Vec<Range<usize>> = buffer
                .text_object_ranges(buffer_offset..buffer_offset, TreeSitterOptions::default())
                .filter_map(|(r, m)| if m == target { Some(r) } else { None })
                .collect();
            matches.sort_by_key(|r| r.end - r.start);
            if let Some(buffer_range) = matches.first() {
                return vec![(
                    BufferOffset(buffer_range.start)..BufferOffset(buffer_range.end),
                    (),
                )];
            }

            let Some(around) = target.around() else {
                return vec![];
            };
            let mut matches: Vec<Range<usize>> = buffer
                .text_object_ranges(buffer_offset..buffer_offset, TreeSitterOptions::default())
                .filter_map(|(r, m)| if m == around { Some(r) } else { None })
                .collect();
            matches.sort_by_key(|r| r.end - r.start);
            let Some(around_range) = matches.first() else {
                return vec![];
            };

            let mut matches: Vec<Range<usize>> = buffer
                .text_object_ranges(around_range.clone(), TreeSitterOptions::default())
                .filter_map(|(r, m)| if m == target { Some(r) } else { None })
                .collect();
            matches.sort_by_key(|r| r.start);
            if let Some(buffer_range) = matches.first()
                && !buffer_range.is_empty()
            {
                return vec![(
                    BufferOffset(buffer_range.start)..BufferOffset(buffer_range.end),
                    (),
                )];
            }
            vec![(
                BufferOffset(around_range.start)..BufferOffset(around_range.end),
                (),
            )]
        })?;

    let (range, ()) = results.into_iter().next()?;
    Some(range.start.to_display_point(map)..range.end.to_display_point(map))
}

fn argument(
    map: &DisplaySnapshot,
    relative_to: DisplayPoint,
    around: bool,
) -> Option<Range<DisplayPoint>> {
    let snapshot = &map.buffer_snapshot();
    let offset = relative_to.to_offset(map, Bias::Left);

    fn comma_delimited_range_at(
        buffer: &BufferSnapshot,
        mut offset: BufferOffset,
        include_comma: bool,
    ) -> Option<Range<BufferOffset>> {
        offset += buffer
            .chars_at(offset)
            .take_while(|c| c.is_whitespace())
            .map(char::len_utf8)
            .sum::<usize>();

        let bracket_filter = |open: Range<usize>, close: Range<usize>| {
            if open.end == close.start {
                return false;
            }

            if open.start == offset.0 || close.end == offset.0 {
                return false;
            }

            matches!(
                buffer.chars_at(open.start).next(),
                Some('(' | '[' | '{' | '<' | '|')
            )
        };

        let (open_bracket, close_bracket) =
            buffer.innermost_enclosing_bracket_ranges(offset..offset, Some(&bracket_filter))?;

        let inner_bracket_range = BufferOffset(open_bracket.end)..BufferOffset(close_bracket.start);

        let layer = buffer.syntax_layer_at(offset)?;
        let node = layer.node();
        let mut cursor = node.walk();

        let mut parent_covers_bracket_range = false;
        loop {
            let node = cursor.node();
            let range = node.byte_range();
            let covers_bracket_range =
                range.start == open_bracket.start && range.end == close_bracket.end;
            if parent_covers_bracket_range && !covers_bracket_range {
                break;
            }
            parent_covers_bracket_range = covers_bracket_range;

            cursor.goto_first_child_for_byte(offset.0)?;
        }

        let mut argument_node = cursor.node();

        if argument_node.byte_range() == open_bracket {
            if !cursor.goto_next_sibling() {
                return Some(inner_bracket_range);
            }
            argument_node = cursor.node();
        }
        while argument_node.byte_range() == close_bracket || argument_node.kind() == "," {
            if !cursor.goto_previous_sibling() {
                return Some(inner_bracket_range);
            }
            argument_node = cursor.node();
            if argument_node.byte_range() == open_bracket {
                return Some(inner_bracket_range);
            }
        }

        let mut start = argument_node.start_byte();
        let mut end = argument_node.end_byte();

        let mut needs_surrounding_comma = include_comma;

        while cursor.goto_previous_sibling() {
            let prev = cursor.node();

            if prev.start_byte() < open_bracket.end {
                start = open_bracket.end;
                break;
            } else if prev.kind() == "," {
                if needs_surrounding_comma {
                    start = prev.start_byte();
                    needs_surrounding_comma = false;
                }
                break;
            } else if prev.start_byte() < start {
                start = prev.start_byte();
            }
        }

        while cursor.goto_next_sibling() {
            let next = cursor.node();

            if next.end_byte() > close_bracket.start {
                end = close_bracket.start;
                break;
            } else if next.kind() == "," {
                if needs_surrounding_comma {
                    if let Some(next_arg) = next.next_sibling() {
                        end = next_arg.start_byte();
                    } else {
                        end = next.end_byte();
                    }
                }
                break;
            } else if next.end_byte() > end {
                end = next.end_byte();
            }
        }

        Some(BufferOffset(start)..BufferOffset(end))
    }

    let results =
        snapshot.map_excerpt_ranges(offset..offset, |buffer, _excerpt_range, buffer_range| {
            let buffer_offset = buffer_range.start;
            match comma_delimited_range_at(buffer, buffer_offset, around) {
                Some(result) => vec![(result, ())],
                None => vec![],
            }
        })?;

    let (range, ()) = results.into_iter().next()?;
    Some(range.start.to_display_point(map)..range.end.to_display_point(map))
}

fn indent(
    map: &DisplaySnapshot,
    relative_to: DisplayPoint,
    around: bool,
    include_below: bool,
) -> Option<Range<DisplayPoint>> {
    let point = relative_to.to_point(map);
    let row = point.row;

    let desired_indent = map.line_indent_for_buffer_row(MultiBufferRow(row));

    // Loop backwards until we find a non-blank line with less indent
    let mut start_row = row;
    for prev_row in (0..row).rev() {
        let indent = map.line_indent_for_buffer_row(MultiBufferRow(prev_row));
        if indent.is_line_empty() {
            continue;
        }
        if indent.spaces < desired_indent.spaces || indent.tabs < desired_indent.tabs {
            if around {
                // When around is true, include the first line with less indent
                start_row = prev_row;
            }
            break;
        }
        start_row = prev_row;
    }

    // Loop forwards until we find a non-blank line with less indent
    let mut end_row = row;
    let max_rows = map.buffer_snapshot().max_row().0;
    for next_row in (row + 1)..=max_rows {
        let indent = map.line_indent_for_buffer_row(MultiBufferRow(next_row));
        if indent.is_line_empty() {
            continue;
        }
        if indent.spaces < desired_indent.spaces || indent.tabs < desired_indent.tabs {
            if around && include_below {
                // When around is true and including below, include this line
                end_row = next_row;
            }
            break;
        }
        end_row = next_row;
    }

    let end_len = map.buffer_snapshot().line_len(MultiBufferRow(end_row));
    let start = map.point_to_display_point(Point::new(start_row, 0), Bias::Right);
    let end = map.point_to_display_point(Point::new(end_row, end_len), Bias::Left);
    Some(start..end)
}

pub fn surrounding_markers(
    map: &DisplaySnapshot,
    relative_to: DisplayPoint,
    around: bool,
    search_across_lines: bool,
    open_marker: char,
    close_marker: char,
) -> Option<Range<DisplayPoint>> {
    let point = relative_to.to_offset(map, Bias::Left);

    let mut matched_closes = 0;
    let mut opening = None;

    let mut before_ch = match movement::chars_before(map, point).next() {
        Some((ch, _)) => ch,
        _ => '\0',
    };
    if let Some((ch, range)) = movement::chars_after(map, point).next()
        && ch == open_marker
        && before_ch != '\\'
    {
        if open_marker == close_marker {
            let mut total = 0;
            for ((ch, _), (before_ch, _)) in movement::chars_before(map, point).tuple_windows() {
                if ch == '\n' {
                    break;
                }
                if ch == open_marker && before_ch != '\\' {
                    total += 1;
                }
            }
            if total % 2 == 0 {
                opening = Some(range)
            }
        } else {
            opening = Some(range)
        }
    }

    if opening.is_none() {
        let mut chars_before = movement::chars_before(map, point).peekable();
        while let Some((ch, range)) = chars_before.next() {
            if ch == '\n' && !search_across_lines {
                break;
            }

            if let Some((before_ch, _)) = chars_before.peek()
                && *before_ch == '\\'
            {
                continue;
            }

            if ch == open_marker {
                if matched_closes == 0 {
                    opening = Some(range);
                    break;
                }
                matched_closes -= 1;
            } else if ch == close_marker {
                matched_closes += 1
            }
        }
    }
    if opening.is_none() {
        for (ch, range) in movement::chars_after(map, point) {
            if before_ch != '\\' {
                if ch == open_marker {
                    opening = Some(range);
                    break;
                } else if ch == close_marker {
                    break;
                }
            }

            before_ch = ch;
        }
    }

    let mut opening = opening?;

    let mut matched_opens = 0;
    let mut closing = None;
    before_ch = match movement::chars_before(map, opening.end).next() {
        Some((ch, _)) => ch,
        _ => '\0',
    };
    for (ch, range) in movement::chars_after(map, opening.end) {
        if ch == '\n' && !search_across_lines {
            break;
        }

        if before_ch != '\\' {
            if ch == close_marker {
                if matched_opens == 0 {
                    closing = Some(range);
                    break;
                }
                matched_opens -= 1;
            } else if ch == open_marker {
                matched_opens += 1;
            }
        }

        before_ch = ch;
    }

    let mut closing = closing?;

    if around && !search_across_lines {
        let mut found = false;

        for (ch, range) in movement::chars_after(map, closing.end) {
            if ch.is_whitespace() && ch != '\n' {
                found = true;
                closing.end = range.end;
            } else {
                break;
            }
        }

        if !found {
            for (ch, range) in movement::chars_before(map, opening.start) {
                if ch.is_whitespace() && ch != '\n' {
                    opening.start = range.start
                } else {
                    break;
                }
            }
        }
    }

    // Adjust selection to remove leading and trailing whitespace for multiline inner brackets
    if !around && open_marker != close_marker {
        let start_point = opening.end.to_display_point(map);
        let end_point = closing.start.to_display_point(map);
        let start_offset = start_point.to_offset(map, Bias::Left);
        let end_offset = end_point.to_offset(map, Bias::Left);

        if start_point.row() != end_point.row()
            && map
                .buffer_chars_at(start_offset)
                .take_while(|(_, offset)| offset < &end_offset)
                .any(|(ch, _)| !ch.is_whitespace())
        {
            let mut first_non_ws = None;
            let mut last_non_ws = None;
            for (ch, offset) in map.buffer_chars_at(start_offset) {
                if !ch.is_whitespace() {
                    first_non_ws = Some(offset);
                    break;
                }
            }
            for (ch, offset) in map.reverse_buffer_chars_at(end_offset) {
                if !ch.is_whitespace() {
                    last_non_ws = Some(offset + ch.len_utf8());
                    break;
                }
            }
            if let Some(start) = first_non_ws {
                opening.end = start;
            }
            if let Some(end) = last_non_ws {
                closing.start = end;
            }
        }
    }

    let result = if around {
        opening.start..closing.end
    } else {
        opening.end..closing.start
    };

    Some(
        map.clip_point(result.start.to_display_point(map), Bias::Left)
            ..map.clip_point(result.end.to_display_point(map), Bias::Right),
    )
}
