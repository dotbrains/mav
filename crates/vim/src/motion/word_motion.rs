use super::*;

pub(crate) fn next_word_start(
    map: &DisplaySnapshot,
    mut point: DisplayPoint,
    ignore_punctuation: bool,
    times: usize,
) -> DisplayPoint {
    let classifier = map
        .buffer_snapshot()
        .char_classifier_at(point.to_point(map))
        .ignore_punctuation(ignore_punctuation);
    for _ in 0..times {
        let mut crossed_newline = false;
        let new_point =
            movement::find_boundary(map, point, FindRange::MultiLine, &mut |left, right| {
                let left_kind = classifier.kind(left);
                let right_kind = classifier.kind(right);
                let at_newline = right == '\n';

                let found = (left_kind != right_kind && right_kind != CharKind::Whitespace)
                    || at_newline && crossed_newline
                    || at_newline && left == '\n'; // Prevents skipping repeated empty lines

                crossed_newline |= at_newline;
                found
            });
        if point == new_point {
            break;
        }
        point = new_point;
    }
    point
}

fn next_end_impl(
    map: &DisplaySnapshot,
    mut point: DisplayPoint,
    times: usize,
    allow_cross_newline: bool,
    always_advance: bool,
    is_boundary: &mut dyn FnMut(char, char) -> bool,
) -> DisplayPoint {
    for _ in 0..times {
        let mut need_next_char = false;
        let new_point = if always_advance {
            next_char(map, point, allow_cross_newline)
        } else {
            point
        };
        let new_point = movement::find_boundary_exclusive(
            map,
            new_point,
            FindRange::MultiLine,
            &mut |left, right| {
                let at_newline = right == '\n';

                if !allow_cross_newline && at_newline {
                    need_next_char = true;
                    return true;
                }

                is_boundary(left, right)
            },
        );
        let new_point = if need_next_char {
            next_char(map, new_point, true)
        } else {
            new_point
        };
        let new_point = map.clip_point(new_point, Bias::Left);
        if point == new_point {
            break;
        }
        point = new_point;
    }
    point
}

pub(crate) fn next_word_end(
    map: &DisplaySnapshot,
    point: DisplayPoint,
    ignore_punctuation: bool,
    times: usize,
    allow_cross_newline: bool,
    always_advance: bool,
) -> DisplayPoint {
    let classifier = map
        .buffer_snapshot()
        .char_classifier_at(point.to_point(map))
        .ignore_punctuation(ignore_punctuation);

    next_end_impl(
        map,
        point,
        times,
        allow_cross_newline,
        always_advance,
        &mut |left, right| {
            let left_kind = classifier.kind(left);
            let right_kind = classifier.kind(right);
            left_kind != right_kind && left_kind != CharKind::Whitespace
        },
    )
}

pub(crate) fn next_subword_end(
    map: &DisplaySnapshot,
    point: DisplayPoint,
    ignore_punctuation: bool,
    times: usize,
    allow_cross_newline: bool,
) -> DisplayPoint {
    let classifier = map
        .buffer_snapshot()
        .char_classifier_at(point.to_point(map))
        .ignore_punctuation(ignore_punctuation);

    next_end_impl(
        map,
        point,
        times,
        allow_cross_newline,
        true,
        &mut |left, right| {
            let left_kind = classifier.kind(left);
            let right_kind = classifier.kind(right);
            let is_stopping_punct = |c: char| ".$=\"'{}[]()<>".contains(c);
            let found_subword_end = is_subword_end(left, right, "$_-");
            let is_word_end = (left_kind != right_kind)
                && (!left.is_ascii_punctuation() || is_stopping_punct(left));

            !left.is_whitespace() && (is_word_end || found_subword_end)
        },
    )
}

fn previous_word_start(
    map: &DisplaySnapshot,
    mut point: DisplayPoint,
    ignore_punctuation: bool,
    times: usize,
) -> DisplayPoint {
    let classifier = map
        .buffer_snapshot()
        .char_classifier_at(point.to_point(map))
        .ignore_punctuation(ignore_punctuation);
    for _ in 0..times {
        // This works even though find_preceding_boundary is called for every character in the line containing
        // cursor because the newline is checked only once.
        let new_point = movement::find_preceding_boundary_display_point(
            map,
            point,
            FindRange::MultiLine,
            &mut |left, right| {
                let left_kind = classifier.kind(left);
                let right_kind = classifier.kind(right);

                (left_kind != right_kind && !right.is_whitespace()) || left == '\n'
            },
        );
        if point == new_point {
            break;
        }
        point = new_point;
    }
    point
}

fn previous_word_end(
    map: &DisplaySnapshot,
    point: DisplayPoint,
    ignore_punctuation: bool,
    times: usize,
) -> DisplayPoint {
    let classifier = map
        .buffer_snapshot()
        .char_classifier_at(point.to_point(map))
        .ignore_punctuation(ignore_punctuation);
    let mut point = point.to_point(map);

    if point.column < map.buffer_snapshot().line_len(MultiBufferRow(point.row))
        && let Some(ch) = map.buffer_snapshot().chars_at(point).next()
    {
        point.column += ch.len_utf8() as u32;
    }
    for _ in 0..times {
        let new_point = movement::find_preceding_boundary_point(
            &map.buffer_snapshot(),
            point,
            FindRange::MultiLine,
            &mut |left, right| {
                let left_kind = classifier.kind(left);
                let right_kind = classifier.kind(right);
                match (left_kind, right_kind) {
                    (CharKind::Punctuation, CharKind::Whitespace)
                    | (CharKind::Punctuation, CharKind::Word)
                    | (CharKind::Word, CharKind::Whitespace)
                    | (CharKind::Word, CharKind::Punctuation) => true,
                    (CharKind::Whitespace, CharKind::Whitespace) => left == '\n' && right == '\n',
                    _ => false,
                }
            },
        );
        if new_point == point {
            break;
        }
        point = new_point;
    }
    movement::saturating_left(map, point.to_display_point(map))
}

/// Checks if there's a subword boundary start between `left` and `right` characters.
/// This detects transitions like `_b` (separator to non-separator) or `aB` (lowercase to uppercase).
pub(crate) fn is_subword_start(left: char, right: char, separators: &str) -> bool {
    let is_separator = |c: char| separators.contains(c);
    (is_separator(left) && !is_separator(right)) || (left.is_lowercase() && right.is_uppercase())
}

/// Checks if there's a subword boundary end between `left` and `right` characters.
/// This detects transitions like `a_` (non-separator to separator) or `aB` (lowercase to uppercase).
pub(crate) fn is_subword_end(left: char, right: char, separators: &str) -> bool {
    let is_separator = |c: char| separators.contains(c);
    (!is_separator(left) && is_separator(right)) || (left.is_lowercase() && right.is_uppercase())
}

fn next_subword_start(
    map: &DisplaySnapshot,
    mut point: DisplayPoint,
    ignore_punctuation: bool,
    times: usize,
) -> DisplayPoint {
    let classifier = map
        .buffer_snapshot()
        .char_classifier_at(point.to_point(map))
        .ignore_punctuation(ignore_punctuation);
    for _ in 0..times {
        let mut crossed_newline = false;
        let new_point =
            movement::find_boundary(map, point, FindRange::MultiLine, &mut |left, right| {
                let left_kind = classifier.kind(left);
                let right_kind = classifier.kind(right);
                let at_newline = right == '\n';
                let is_stopping_punct = |c: char| "$=\"'{}[]()<>".contains(c);
                let found_subword_start = is_subword_start(left, right, ".$_-");
                let is_word_start = (left_kind != right_kind)
                    && (!right.is_ascii_punctuation() || is_stopping_punct(right));

                let found = (!right.is_whitespace() && (is_word_start || found_subword_start))
                    || at_newline && crossed_newline
                    || right == '\n' && left == '\n'; // Prevents skipping repeated empty lines

                crossed_newline |= at_newline;
                found
            });
        if point == new_point {
            break;
        }
        point = new_point;
    }
    point
}

fn previous_subword_start(
    map: &DisplaySnapshot,
    mut point: DisplayPoint,
    ignore_punctuation: bool,
    times: usize,
) -> DisplayPoint {
    let classifier = map
        .buffer_snapshot()
        .char_classifier_at(point.to_point(map))
        .ignore_punctuation(ignore_punctuation);
    for _ in 0..times {
        let mut crossed_newline = false;
        // This works even though find_preceding_boundary is called for every character in the line containing
        // cursor because the newline is checked only once.
        let new_point = movement::find_preceding_boundary_display_point(
            map,
            point,
            FindRange::MultiLine,
            &mut |left, right| {
                let left_kind = classifier.kind(left);
                let right_kind = classifier.kind(right);
                let at_newline = right == '\n';

                let is_stopping_punct = |c: char| ".$=\"'{}[]()<>".contains(c);
                let is_word_start = (left_kind != right_kind)
                    && (is_stopping_punct(right) || !right.is_ascii_punctuation());
                let found_subword_start = is_subword_start(left, right, ".$_-");

                let found = (!right.is_whitespace() && (is_word_start || found_subword_start))
                    || at_newline && crossed_newline
                    || at_newline && left == '\n'; // Prevents skipping repeated empty lines

                crossed_newline |= at_newline;

                found
            },
        );
        if point == new_point {
            break;
        }
        point = new_point;
    }
    point
}

fn previous_subword_end(
    map: &DisplaySnapshot,
    point: DisplayPoint,
    ignore_punctuation: bool,
    times: usize,
) -> DisplayPoint {
    let classifier = map
        .buffer_snapshot()
        .char_classifier_at(point.to_point(map))
        .ignore_punctuation(ignore_punctuation);
    let mut point = point.to_point(map);

    if point.column < map.buffer_snapshot().line_len(MultiBufferRow(point.row))
        && let Some(ch) = map.buffer_snapshot().chars_at(point).next()
    {
        point.column += ch.len_utf8() as u32;
    }
    for _ in 0..times {
        let new_point = movement::find_preceding_boundary_point(
            &map.buffer_snapshot(),
            point,
            FindRange::MultiLine,
            &mut |left, right| {
                let left_kind = classifier.kind(left);
                let right_kind = classifier.kind(right);

                let is_stopping_punct = |c: char| ".$;=\"'{}[]()<>".contains(c);
                let found_subword_end = is_subword_end(left, right, "$_-");

                if found_subword_end {
                    return true;
                }

                match (left_kind, right_kind) {
                    (CharKind::Word, CharKind::Whitespace)
                    | (CharKind::Word, CharKind::Punctuation) => true,
                    (CharKind::Punctuation, _) if is_stopping_punct(left) => true,
                    (CharKind::Whitespace, CharKind::Whitespace) => left == '\n' && right == '\n',
                    _ => false,
                }
            },
        );
        if new_point == point {
            break;
        }
        point = new_point;
    }
    movement::saturating_left(map, point.to_display_point(map))
}
