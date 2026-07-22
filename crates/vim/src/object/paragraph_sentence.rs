use super::*;

pub(super) fn sentence(
    map: &DisplaySnapshot,
    relative_to: DisplayPoint,
    around: bool,
) -> Option<Range<DisplayPoint>> {
    let mut start = None;
    let relative_offset = relative_to.to_offset(map, Bias::Left);
    let mut previous_end = relative_offset;

    let mut chars = map.buffer_chars_at(previous_end).peekable();

    // Search backwards for the previous sentence end or current sentence start. Include the character under relative_to
    for (char, offset) in chars
        .peek()
        .cloned()
        .into_iter()
        .chain(map.reverse_buffer_chars_at(previous_end))
    {
        if is_sentence_end(map, offset) {
            break;
        }

        if is_possible_sentence_start(char) {
            start = Some(offset);
        }

        previous_end = offset;
    }

    // Search forward for the end of the current sentence or if we are between sentences, the start of the next one
    let mut end = relative_offset;
    for (char, offset) in chars {
        if start.is_none() && is_possible_sentence_start(char) {
            if around {
                start = Some(offset);
                continue;
            } else {
                end = offset;
                break;
            }
        }

        if char != '\n' {
            end = offset + char.len_utf8();
        }

        if is_sentence_end(map, end) {
            break;
        }
    }

    let mut range = start.unwrap_or(previous_end).to_display_point(map)..end.to_display_point(map);
    if around {
        range = expand_to_include_whitespace(map, range, false);
    }

    Some(range)
}

fn is_possible_sentence_start(character: char) -> bool {
    !character.is_whitespace() && character != '.'
}

const SENTENCE_END_PUNCTUATION: &[char] = &['.', '!', '?'];
const SENTENCE_END_FILLERS: &[char] = &[')', ']', '"', '\''];
const SENTENCE_END_WHITESPACE: &[char] = &[' ', '\t', '\n'];
fn is_sentence_end(map: &DisplaySnapshot, offset: MultiBufferOffset) -> bool {
    let mut next_chars = map.buffer_chars_at(offset).peekable();
    if let Some((char, _)) = next_chars.next() {
        // We are at a double newline. This position is a sentence end.
        if char == '\n' && next_chars.peek().map(|(c, _)| c == &'\n').unwrap_or(false) {
            return true;
        }

        // The next text is not a valid whitespace. This is not a sentence end
        if !SENTENCE_END_WHITESPACE.contains(&char) {
            return false;
        }
    }

    for (char, _) in map.reverse_buffer_chars_at(offset) {
        if SENTENCE_END_PUNCTUATION.contains(&char) {
            return true;
        }

        if !SENTENCE_END_FILLERS.contains(&char) {
            return false;
        }
    }

    false
}

/// Expands the passed range to include whitespace on one side or the other in a line. Attempts to add the
/// whitespace to the end first and falls back to the start if there was none.
pub fn expand_to_include_whitespace(
    map: &DisplaySnapshot,
    range: Range<DisplayPoint>,
    stop_at_newline: bool,
) -> Range<DisplayPoint> {
    let mut range = range.start.to_offset(map, Bias::Left)..range.end.to_offset(map, Bias::Right);
    let mut whitespace_included = false;

    let chars = map.buffer_chars_at(range.end).peekable();
    for (char, offset) in chars {
        if char == '\n' && stop_at_newline {
            break;
        }

        if char.is_whitespace() {
            if char != '\n' || !stop_at_newline {
                range.end = offset + char.len_utf8();
                whitespace_included = true;
            }
        } else {
            // Found non whitespace. Quit out.
            break;
        }
    }

    if !whitespace_included {
        for (char, point) in map.reverse_buffer_chars_at(range.start) {
            if char == '\n' && stop_at_newline {
                break;
            }

            if !char.is_whitespace() {
                break;
            }

            range.start = point;
        }
    }

    range.start.to_display_point(map)..range.end.to_display_point(map)
}

/// If not `around` (i.e. inner), returns a range that surrounds the paragraph
/// where `relative_to` is in. If `around`, principally returns the range ending
/// at the end of the next paragraph.
///
/// Here, the "paragraph" is defined as a block of non-blank lines or a block of
/// blank lines. If the paragraph ends with a trailing newline (i.e. not with
/// EOF), the returned range ends at the trailing newline of the paragraph (i.e.
/// the trailing newline is not subject to subsequent operations).
///
/// Edge cases:
/// - If `around` and if the current paragraph is the last paragraph of the
///   file and is blank, then the selection results in an error.
/// - If `around` and if the current paragraph is the last paragraph of the
///   file and is not blank, then the returned range starts at the start of the
///   previous paragraph, if it exists.
pub(super) fn paragraph(
    map: &DisplaySnapshot,
    relative_to: DisplayPoint,
    around: bool,
    times: usize,
) -> Option<Range<DisplayPoint>> {
    let mut paragraph_start = start_of_paragraph(map, relative_to);
    let mut paragraph_end = end_of_paragraph(map, relative_to);

    for i in 0..times {
        let paragraph_end_row = paragraph_end.row();
        let paragraph_ends_with_eof = paragraph_end_row == map.max_point().row();
        let point = relative_to.to_point(map);
        let current_line_is_empty = map
            .buffer_snapshot()
            .is_line_blank(MultiBufferRow(point.row));

        if around {
            if paragraph_ends_with_eof {
                if current_line_is_empty {
                    return None;
                }

                let paragraph_start_buffer_point = paragraph_start.to_point(map);
                if paragraph_start_buffer_point.row != 0 {
                    let previous_paragraph_last_line_start =
                        Point::new(paragraph_start_buffer_point.row - 1, 0).to_display_point(map);
                    paragraph_start = start_of_paragraph(map, previous_paragraph_last_line_start);
                }
            } else {
                let paragraph_end_buffer_point = paragraph_end.to_point(map);
                let mut start_row = paragraph_end_buffer_point.row + 1;
                if i > 0 {
                    start_row += 1;
                }
                let next_paragraph_start = Point::new(start_row, 0).to_display_point(map);
                paragraph_end = end_of_paragraph(map, next_paragraph_start);
            }
        }
    }

    let range = paragraph_start..paragraph_end;
    Some(range)
}

/// Returns a position of the start of the current paragraph, where a paragraph
/// is defined as a run of non-blank lines or a run of blank lines.
fn start_of_paragraph(map: &DisplaySnapshot, display_point: DisplayPoint) -> DisplayPoint {
    let point = display_point.to_point(map);
    if point.row == 0 {
        return DisplayPoint::zero();
    }

    let is_current_line_blank = map
        .buffer_snapshot()
        .is_line_blank(MultiBufferRow(point.row));

    for row in (0..point.row).rev() {
        let blank = map.buffer_snapshot().is_line_blank(MultiBufferRow(row));
        if blank != is_current_line_blank {
            return Point::new(row + 1, 0).to_display_point(map);
        }
    }

    DisplayPoint::zero()
}

/// Returns a position of the end of the current paragraph, where a paragraph
/// is defined as a run of non-blank lines or a run of blank lines.
/// The trailing newline is excluded from the paragraph.
fn end_of_paragraph(map: &DisplaySnapshot, display_point: DisplayPoint) -> DisplayPoint {
    let point = display_point.to_point(map);
    if point.row == map.buffer_snapshot().max_row().0 {
        return map.max_point();
    }

    let is_current_line_blank = map
        .buffer_snapshot()
        .is_line_blank(MultiBufferRow(point.row));

    for row in point.row + 1..map.buffer_snapshot().max_row().0 + 1 {
        let blank = map.buffer_snapshot().is_line_blank(MultiBufferRow(row));
        if blank != is_current_line_blank {
            let previous_row = row - 1;
            return Point::new(
                previous_row,
                map.buffer_snapshot().line_len(MultiBufferRow(previous_row)),
            )
            .to_display_point(map);
        }
    }

    map.max_point()
}
