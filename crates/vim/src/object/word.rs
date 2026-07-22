use super::*;

pub(super) fn in_word(
    map: &DisplaySnapshot,
    relative_to: DisplayPoint,
    ignore_punctuation: bool,
    times: usize,
) -> Option<Range<DisplayPoint>> {
    // Use motion::right so that we consider the character under the cursor when looking for the start
    let classifier = map
        .buffer_snapshot()
        .char_classifier_at(relative_to.to_point(map))
        .ignore_punctuation(ignore_punctuation);
    let start = movement::find_preceding_boundary_display_point(
        map,
        right(map, relative_to, 1),
        movement::FindRange::SingleLine,
        &mut |left, right| classifier.kind(left) != classifier.kind(right),
    );

    let mut end = movement::find_boundary(
        map,
        relative_to,
        FindRange::SingleLine,
        &mut |left, right| classifier.kind(left) != classifier.kind(right),
    );

    let mut is_boundary = |left: char, right: char| classifier.kind(left) != classifier.kind(right);

    for _ in 1..times {
        let kind_at_end = map
            .buffer_chars_at(end.to_offset(map, Bias::Right))
            .next()
            .map(|(c, _)| classifier.kind(c));

        // Skip whitespace but not punctuation (punctuation is its own word unit).
        let next_end = if kind_at_end == Some(CharKind::Whitespace) {
            let after_whitespace =
                movement::find_boundary(map, end, FindRange::MultiLine, &mut is_boundary);
            movement::find_boundary(
                map,
                after_whitespace,
                FindRange::MultiLine,
                &mut is_boundary,
            )
        } else {
            movement::find_boundary(map, end, FindRange::MultiLine, &mut is_boundary)
        };
        if next_end == end {
            break;
        }
        end = next_end;
    }

    Some(start..end)
}

pub(super) fn in_subword(
    map: &DisplaySnapshot,
    relative_to: DisplayPoint,
    ignore_punctuation: bool,
) -> Option<Range<DisplayPoint>> {
    let offset = relative_to.to_offset(map, Bias::Left);
    // Use motion::right so that we consider the character under the cursor when looking for the start
    let classifier = map
        .buffer_snapshot()
        .char_classifier_at(relative_to.to_point(map))
        .ignore_punctuation(ignore_punctuation);
    let in_subword = map
        .buffer_chars_at(offset)
        .next()
        .map(|(c, _)| {
            let is_separator = "._-".contains(c);
            !classifier.is_whitespace(c) && !is_separator
        })
        .unwrap_or(false);

    let start = if in_subword {
        movement::find_preceding_boundary_display_point(
            map,
            right(map, relative_to, 1),
            movement::FindRange::SingleLine,
            &mut |left, right| {
                let is_word_start = classifier.kind(left) != classifier.kind(right);
                is_word_start || is_subword_start(left, right, "._-")
            },
        )
    } else {
        movement::find_boundary(
            map,
            relative_to,
            FindRange::SingleLine,
            &mut |left, right| {
                let is_word_start = classifier.kind(left) != classifier.kind(right);
                is_word_start || is_subword_start(left, right, "._-")
            },
        )
    };

    let end = movement::find_boundary(
        map,
        relative_to,
        FindRange::SingleLine,
        &mut |left, right| {
            let is_word_end = classifier.kind(left) != classifier.kind(right);
            is_word_end || is_subword_end(left, right, "._-")
        },
    );

    Some(start..end)
}

pub(super) fn around_word(
    map: &DisplaySnapshot,
    relative_to: DisplayPoint,
    ignore_punctuation: bool,
    times: usize,
) -> Option<Range<DisplayPoint>> {
    let offset = relative_to.to_offset(map, Bias::Left);
    let classifier = map
        .buffer_snapshot()
        .char_classifier_at(offset)
        .ignore_punctuation(ignore_punctuation);
    let in_word = map
        .buffer_chars_at(offset)
        .next()
        .map(|(c, _)| !classifier.is_whitespace(c))
        .unwrap_or(false);

    if in_word {
        around_containing_word(map, relative_to, ignore_punctuation, times)
    } else {
        around_next_word(map, relative_to, ignore_punctuation, times)
    }
}

pub(super) fn around_subword(
    map: &DisplaySnapshot,
    relative_to: DisplayPoint,
    ignore_punctuation: bool,
) -> Option<Range<DisplayPoint>> {
    // Use motion::right so that we consider the character under the cursor when looking for the start
    let classifier = map
        .buffer_snapshot()
        .char_classifier_at(relative_to.to_point(map))
        .ignore_punctuation(ignore_punctuation);
    let start = movement::find_preceding_boundary_display_point(
        map,
        right(map, relative_to, 1),
        movement::FindRange::SingleLine,
        &mut |left, right| {
            let is_separator = |c: char| "._-".contains(c);
            let is_word_start =
                classifier.kind(left) != classifier.kind(right) && !is_separator(left);
            is_word_start || is_subword_start(left, right, "._-")
        },
    );

    let end = movement::find_boundary(
        map,
        relative_to,
        FindRange::SingleLine,
        &mut |left, right| {
            let is_separator = |c: char| "._-".contains(c);
            let is_word_end =
                classifier.kind(left) != classifier.kind(right) && !is_separator(right);
            is_word_end || is_subword_end(left, right, "._-")
        },
    );

    Some(start..end).map(|range| expand_to_include_whitespace(map, range, true))
}

fn around_containing_word(
    map: &DisplaySnapshot,
    relative_to: DisplayPoint,
    ignore_punctuation: bool,
    times: usize,
) -> Option<Range<DisplayPoint>> {
    in_word(map, relative_to, ignore_punctuation, times).map(|range| {
        let spans_multiple_lines = range.start.row() != range.end.row();
        let stop_at_newline = !spans_multiple_lines;

        let line_start = DisplayPoint::new(range.start.row(), 0);
        let is_first_word = map
            .buffer_chars_at(line_start.to_offset(map, Bias::Left))
            .take_while(|(ch, offset)| {
                offset < &range.start.to_offset(map, Bias::Left) && ch.is_whitespace()
            })
            .count()
            > 0;

        if is_first_word {
            // For first word on line, trim indentation
            let mut expanded = expand_to_include_whitespace(map, range.clone(), stop_at_newline);
            expanded.start = range.start;
            expanded
        } else {
            expand_to_include_whitespace(map, range, stop_at_newline)
        }
    })
}

fn around_next_word(
    map: &DisplaySnapshot,
    relative_to: DisplayPoint,
    ignore_punctuation: bool,
    times: usize,
) -> Option<Range<DisplayPoint>> {
    let classifier = map
        .buffer_snapshot()
        .char_classifier_at(relative_to.to_point(map))
        .ignore_punctuation(ignore_punctuation);
    let start = movement::find_preceding_boundary_display_point(
        map,
        right(map, relative_to, 1),
        FindRange::SingleLine,
        &mut |left, right| classifier.kind(left) != classifier.kind(right),
    );

    let mut word_found = false;
    let mut end = movement::find_boundary(
        map,
        relative_to,
        FindRange::MultiLine,
        &mut |left, right| {
            let left_kind = classifier.kind(left);
            let right_kind = classifier.kind(right);

            let found = (word_found && left_kind != right_kind) || right == '\n' && left == '\n';

            if right_kind != CharKind::Whitespace {
                word_found = true;
            }

            found
        },
    );

    for _ in 1..times {
        let next_end =
            movement::find_boundary(map, end, FindRange::MultiLine, &mut |left, right| {
                let left_kind = classifier.kind(left);
                let right_kind = classifier.kind(right);

                let in_word_unit = left_kind != CharKind::Whitespace;
                (in_word_unit && left_kind != right_kind) || right == '\n' && left == '\n'
            });
        if next_end == end {
            break;
        }
        end = next_end;
    }

    Some(start..end)
}
