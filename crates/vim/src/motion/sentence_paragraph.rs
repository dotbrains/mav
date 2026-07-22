use super::*;

pub(crate) fn sentence_backwards(
    map: &DisplaySnapshot,
    point: DisplayPoint,
    mut times: usize,
) -> DisplayPoint {
    let mut start = point.to_point(map).to_offset(&map.buffer_snapshot());
    let mut chars = map.reverse_buffer_chars_at(start).peekable();

    let mut was_newline = map
        .buffer_chars_at(start)
        .next()
        .is_some_and(|(c, _)| c == '\n');

    while let Some((ch, offset)) = chars.next() {
        let start_of_next_sentence = if was_newline && ch == '\n' {
            Some(offset + ch.len_utf8())
        } else if ch == '\n' && chars.peek().is_some_and(|(c, _)| *c == '\n') {
            Some(next_non_blank(map, offset + ch.len_utf8()))
        } else if ch == '.' || ch == '?' || ch == '!' {
            start_of_next_sentence(map, offset + ch.len_utf8())
        } else {
            None
        };

        if let Some(start_of_next_sentence) = start_of_next_sentence {
            if start_of_next_sentence < start {
                times = times.saturating_sub(1);
            }
            if times == 0 || offset.0 == 0 {
                return map.clip_point(
                    start_of_next_sentence
                        .to_offset(&map.buffer_snapshot())
                        .to_display_point(map),
                    Bias::Left,
                );
            }
        }
        if was_newline {
            start = offset;
        }
        was_newline = ch == '\n';
    }

    DisplayPoint::zero()
}

pub(crate) fn sentence_forwards(
    map: &DisplaySnapshot,
    point: DisplayPoint,
    mut times: usize,
) -> DisplayPoint {
    let start = point.to_point(map).to_offset(&map.buffer_snapshot());
    let mut chars = map.buffer_chars_at(start).peekable();

    let mut was_newline = map
        .reverse_buffer_chars_at(start)
        .next()
        .is_some_and(|(c, _)| c == '\n')
        && chars.peek().is_some_and(|(c, _)| *c == '\n');

    while let Some((ch, offset)) = chars.next() {
        if was_newline && ch == '\n' {
            continue;
        }
        let start_of_next_sentence = if was_newline {
            Some(next_non_blank(map, offset))
        } else if ch == '\n' && chars.peek().is_some_and(|(c, _)| *c == '\n') {
            Some(next_non_blank(map, offset + ch.len_utf8()))
        } else if ch == '.' || ch == '?' || ch == '!' {
            start_of_next_sentence(map, offset + ch.len_utf8())
        } else {
            None
        };

        if let Some(start_of_next_sentence) = start_of_next_sentence {
            times = times.saturating_sub(1);
            if times == 0 {
                return map.clip_point(
                    start_of_next_sentence
                        .to_offset(&map.buffer_snapshot())
                        .to_display_point(map),
                    Bias::Right,
                );
            }
        }

        was_newline = ch == '\n' && chars.peek().is_some_and(|(c, _)| *c == '\n');
    }

    map.max_point()
}

/// Returns a position of the start of the current paragraph for vim motions,
/// where a paragraph is defined as a run of non-empty lines. Lines containing
/// only whitespace are not considered empty and do not act as paragraph
/// boundaries.
pub(crate) fn start_of_paragraph(
    map: &DisplaySnapshot,
    display_point: DisplayPoint,
    mut count: usize,
) -> DisplayPoint {
    let point = display_point.to_point(map);
    if point.row == 0 {
        return DisplayPoint::zero();
    }

    let mut found_non_empty_line = false;
    for row in (0..point.row + 1).rev() {
        let empty = map.buffer_snapshot().line_len(MultiBufferRow(row)) == 0;
        if found_non_empty_line && empty {
            if count <= 1 {
                return Point::new(row, 0).to_display_point(map);
            }
            count -= 1;
            found_non_empty_line = false;
        }

        found_non_empty_line |= !empty;
    }

    DisplayPoint::zero()
}

/// Returns a position of the end of the current paragraph for vim motions,
/// where a paragraph is defined as a run of non-empty lines. Lines containing
/// only whitespace are not considered empty and do not act as paragraph
/// boundaries.
pub(crate) fn end_of_paragraph(
    map: &DisplaySnapshot,
    display_point: DisplayPoint,
    mut count: usize,
) -> DisplayPoint {
    let point = display_point.to_point(map);
    if point.row == map.buffer_snapshot().max_row().0 {
        return map.max_point();
    }

    let mut found_non_empty_line = false;
    for row in point.row..=map.buffer_snapshot().max_row().0 {
        let empty = map.buffer_snapshot().line_len(MultiBufferRow(row)) == 0;
        if found_non_empty_line && empty {
            if count <= 1 {
                return Point::new(row, 0).to_display_point(map);
            }
            count -= 1;
            found_non_empty_line = false;
        }

        found_non_empty_line |= !empty;
    }

    map.max_point()
}

fn next_non_blank(map: &DisplaySnapshot, start: MultiBufferOffset) -> MultiBufferOffset {
    for (c, o) in map.buffer_chars_at(start) {
        if c == '\n' || !c.is_whitespace() {
            return o;
        }
    }

    map.buffer_snapshot().len()
}

// given the offset after a ., !, or ? find the start of the next sentence.
// if this is not a sentence boundary, returns None.
fn start_of_next_sentence(
    map: &DisplaySnapshot,
    end_of_sentence: MultiBufferOffset,
) -> Option<MultiBufferOffset> {
    let chars = map.buffer_chars_at(end_of_sentence);
    let mut seen_space = false;

    for (char, offset) in chars {
        if !seen_space && (char == ')' || char == ']' || char == '"' || char == '\'') {
            continue;
        }

        if char == '\n' && seen_space {
            return Some(offset);
        } else if char.is_whitespace() {
            seen_space = true;
        } else if seen_space {
            return Some(offset);
        } else {
            return None;
        }
    }

    Some(map.buffer_snapshot().len())
}
