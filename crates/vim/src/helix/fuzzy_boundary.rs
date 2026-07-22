use super::*;

pub enum FuzzyBoundary {
    Sentence,
    Paragraph,
}

impl ImmediateBoundary {
    fn is_inner_start(&self, left: char, right: char, classifier: CharClassifier) -> bool {
        match self {
            Self::Word { ignore_punctuation } => {
                let classifier = classifier.ignore_punctuation(*ignore_punctuation);
                is_word_start(left, right, &classifier)
                    || (is_buffer_start(left) && classifier.kind(right) != CharKind::Whitespace)
            }
            Self::Subword { ignore_punctuation } => {
                let classifier = classifier.ignore_punctuation(*ignore_punctuation);
                movement::is_subword_start(left, right, &classifier)
                    || (is_buffer_start(left) && classifier.kind(right) != CharKind::Whitespace)
            }
            Self::AngleBrackets => left == '<',
            Self::BackQuotes => left == '`',
            Self::CurlyBrackets => left == '{',
            Self::DoubleQuotes => left == '"',
            Self::Parentheses => left == '(',
            Self::SingleQuotes => left == '\'',
            Self::SquareBrackets => left == '[',
            Self::VerticalBars => left == '|',
        }
    }
    fn is_inner_end(&self, left: char, right: char, classifier: CharClassifier) -> bool {
        match self {
            Self::Word { ignore_punctuation } => {
                let classifier = classifier.ignore_punctuation(*ignore_punctuation);
                is_word_end(left, right, &classifier)
                    || (is_buffer_end(right) && classifier.kind(left) != CharKind::Whitespace)
            }
            Self::Subword { ignore_punctuation } => {
                let classifier = classifier.ignore_punctuation(*ignore_punctuation);
                movement::is_subword_start(left, right, &classifier)
                    || (is_buffer_end(right) && classifier.kind(left) != CharKind::Whitespace)
            }
            Self::AngleBrackets => right == '>',
            Self::BackQuotes => right == '`',
            Self::CurlyBrackets => right == '}',
            Self::DoubleQuotes => right == '"',
            Self::Parentheses => right == ')',
            Self::SingleQuotes => right == '\'',
            Self::SquareBrackets => right == ']',
            Self::VerticalBars => right == '|',
        }
    }
    fn is_outer_start(&self, left: char, right: char, classifier: CharClassifier) -> bool {
        match self {
            word @ Self::Word { .. } => word.is_inner_end(left, right, classifier) || left == '\n',
            subword @ Self::Subword { .. } => {
                subword.is_inner_end(left, right, classifier) || left == '\n'
            }
            Self::AngleBrackets => right == '<',
            Self::BackQuotes => right == '`',
            Self::CurlyBrackets => right == '{',
            Self::DoubleQuotes => right == '"',
            Self::Parentheses => right == '(',
            Self::SingleQuotes => right == '\'',
            Self::SquareBrackets => right == '[',
            Self::VerticalBars => right == '|',
        }
    }
    fn is_outer_end(&self, left: char, right: char, classifier: CharClassifier) -> bool {
        match self {
            word @ Self::Word { .. } => {
                word.is_inner_start(left, right, classifier) || right == '\n'
            }
            subword @ Self::Subword { .. } => {
                subword.is_inner_start(left, right, classifier) || right == '\n'
            }
            Self::AngleBrackets => left == '>',
            Self::BackQuotes => left == '`',
            Self::CurlyBrackets => left == '}',
            Self::DoubleQuotes => left == '"',
            Self::Parentheses => left == ')',
            Self::SingleQuotes => left == '\'',
            Self::SquareBrackets => left == ']',
            Self::VerticalBars => left == '|',
        }
    }
}

impl BoundedObject for ImmediateBoundary {
    fn next_start(&self, map: &DisplaySnapshot, from: Offset, outer: bool) -> Option<Offset> {
        try_find_boundary(map, from, &|left, right| {
            let classifier = map.buffer_snapshot().char_classifier_at(from.0);
            if outer {
                self.is_outer_start(left, right, classifier)
            } else {
                self.is_inner_start(left, right, classifier)
            }
        })
    }
    fn next_end(&self, map: &DisplaySnapshot, from: Offset, outer: bool) -> Option<Offset> {
        try_find_boundary(map, from, &|left, right| {
            let classifier = map.buffer_snapshot().char_classifier_at(from.0);
            if outer {
                self.is_outer_end(left, right, classifier)
            } else {
                self.is_inner_end(left, right, classifier)
            }
        })
    }
    fn previous_start(&self, map: &DisplaySnapshot, from: Offset, outer: bool) -> Option<Offset> {
        try_find_preceding_boundary(map, from, &|left, right| {
            let classifier = map.buffer_snapshot().char_classifier_at(from.0);
            if outer {
                self.is_outer_start(left, right, classifier)
            } else {
                self.is_inner_start(left, right, classifier)
            }
        })
    }
    fn previous_end(&self, map: &DisplaySnapshot, from: Offset, outer: bool) -> Option<Offset> {
        try_find_preceding_boundary(map, from, &|left, right| {
            let classifier = map.buffer_snapshot().char_classifier_at(from.0);
            if outer {
                self.is_outer_end(left, right, classifier)
            } else {
                self.is_inner_end(left, right, classifier)
            }
        })
    }
    fn inner_range_can_be_zero_width(&self) -> bool {
        match self {
            Self::Subword { .. } | Self::Word { .. } => false,
            _ => true,
        }
    }
    fn surround_on_both_sides(&self) -> bool {
        match self {
            Self::Subword { .. } | Self::Word { .. } => false,
            _ => true,
        }
    }
    fn ambiguous_outer(&self) -> bool {
        match self {
            Self::BackQuotes
            | Self::DoubleQuotes
            | Self::SingleQuotes
            | Self::VerticalBars
            | Self::Subword { .. }
            | Self::Word { .. } => true,
            _ => false,
        }
    }
}

impl FuzzyBoundary {
    /// When between two chars that form an easy-to-find identifier boundary,
    /// what's the way to get to the actual start of the object, if any
    fn is_near_potential_inner_start<'a>(
        &self,
        left: char,
        right: char,
        classifier: &CharClassifier,
    ) -> Option<Box<dyn Fn(Offset, &'a DisplaySnapshot) -> Option<Offset>>> {
        if is_buffer_start(left) {
            return Some(Box::new(|identifier, _| Some(identifier)));
        }
        match self {
            Self::Paragraph => {
                if left != '\n' || right != '\n' {
                    return None;
                }
                Some(Box::new(|identifier, map| {
                    try_find_boundary(map, identifier, &|left, right| {
                        left == '\n' && right != '\n'
                    })
                }))
            }
            Self::Sentence => {
                if let Some(find_paragraph_start) =
                    Self::Paragraph.is_near_potential_inner_start(left, right, classifier)
                {
                    return Some(find_paragraph_start);
                } else if !is_sentence_end(left, right, classifier) {
                    return None;
                }
                Some(Box::new(|identifier, map| {
                    let word = ImmediateBoundary::Word {
                        ignore_punctuation: false,
                    };
                    word.next_start(map, identifier, false)
                }))
            }
        }
    }
    /// When between two chars that form an easy-to-find identifier boundary,
    /// what's the way to get to the actual end of the object, if any
    fn is_near_potential_inner_end<'a>(
        &self,
        left: char,
        right: char,
        classifier: &CharClassifier,
    ) -> Option<Box<dyn Fn(Offset, &'a DisplaySnapshot) -> Option<Offset>>> {
        if is_buffer_end(right) {
            return Some(Box::new(|identifier, _| Some(identifier)));
        }
        match self {
            Self::Paragraph => {
                if left != '\n' || right != '\n' {
                    return None;
                }
                Some(Box::new(|identifier, map| {
                    try_find_preceding_boundary(map, identifier, &|left, right| {
                        left != '\n' && right == '\n'
                    })
                }))
            }
            Self::Sentence => {
                if let Some(find_paragraph_end) =
                    Self::Paragraph.is_near_potential_inner_end(left, right, classifier)
                {
                    return Some(find_paragraph_end);
                } else if !is_sentence_end(left, right, classifier) {
                    return None;
                }
                Some(Box::new(|identifier, _| Some(identifier)))
            }
        }
    }
    /// When between two chars that form an easy-to-find identifier boundary,
    /// what's the way to get to the actual end of the object, if any
    fn is_near_potential_outer_start<'a>(
        &self,
        left: char,
        right: char,
        classifier: &CharClassifier,
    ) -> Option<Box<dyn Fn(Offset, &'a DisplaySnapshot) -> Option<Offset>>> {
        match self {
            paragraph @ Self::Paragraph => {
                paragraph.is_near_potential_inner_end(left, right, classifier)
            }
            sentence @ Self::Sentence => {
                sentence.is_near_potential_inner_end(left, right, classifier)
            }
        }
    }
    /// When between two chars that form an easy-to-find identifier boundary,
    /// what's the way to get to the actual end of the object, if any
    fn is_near_potential_outer_end<'a>(
        &self,
        left: char,
        right: char,
        classifier: &CharClassifier,
    ) -> Option<Box<dyn Fn(Offset, &'a DisplaySnapshot) -> Option<Offset>>> {
        match self {
            paragraph @ Self::Paragraph => {
                paragraph.is_near_potential_inner_start(left, right, classifier)
            }
            sentence @ Self::Sentence => {
                sentence.is_near_potential_inner_start(left, right, classifier)
            }
        }
    }

    // The boundary can be on the other side of `from` than the identifier, so the search needs to go both ways.
    // Also, the distance (and direction) between identifier and boundary could vary, so a few ones need to be
    // compared, even if one boundary was already found on the right side of `from`.
    fn to_boundary(
        &self,
        map: &DisplaySnapshot,
        from: Offset,
        outer: bool,
        backward: bool,
        boundary_kind: Boundary,
    ) -> Option<Offset> {
        let generate_boundary_data = |left, right, point: Offset| {
            let classifier = map.buffer_snapshot().char_classifier_at(from.0);
            let reach_boundary = if outer && boundary_kind == Boundary::Start {
                self.is_near_potential_outer_start(left, right, &classifier)
            } else if !outer && boundary_kind == Boundary::Start {
                self.is_near_potential_inner_start(left, right, &classifier)
            } else if outer && boundary_kind == Boundary::End {
                self.is_near_potential_outer_end(left, right, &classifier)
            } else {
                self.is_near_potential_inner_end(left, right, &classifier)
            };

            reach_boundary.map(|reach_start| (point, reach_start))
        };

        let forwards = try_find_boundary_data(map, from, generate_boundary_data);
        let backwards = try_find_preceding_boundary_data(map, from, generate_boundary_data);
        let boundaries = [forwards, backwards]
            .into_iter()
            .flatten()
            .filter_map(|(identifier, reach_boundary)| reach_boundary(identifier, map))
            .filter(|boundary| match boundary.cmp(&from) {
                Ordering::Equal => true,
                Ordering::Less => backward,
                Ordering::Greater => !backward,
            });
        if backward {
            boundaries.max_by_key(|boundary| *boundary)
        } else {
            boundaries.min_by_key(|boundary| *boundary)
        }
    }
}

#[derive(PartialEq)]
enum Boundary {
    Start,
    End,
}

impl BoundedObject for FuzzyBoundary {
    fn next_start(&self, map: &DisplaySnapshot, from: Offset, outer: bool) -> Option<Offset> {
        self.to_boundary(map, from, outer, false, Boundary::Start)
    }
    fn next_end(&self, map: &DisplaySnapshot, from: Offset, outer: bool) -> Option<Offset> {
        self.to_boundary(map, from, outer, false, Boundary::End)
    }
    fn previous_start(&self, map: &DisplaySnapshot, from: Offset, outer: bool) -> Option<Offset> {
        self.to_boundary(map, from, outer, true, Boundary::Start)
    }
    fn previous_end(&self, map: &DisplaySnapshot, from: Offset, outer: bool) -> Option<Offset> {
        self.to_boundary(map, from, outer, true, Boundary::End)
    }
    fn inner_range_can_be_zero_width(&self) -> bool {
        false
    }
    fn surround_on_both_sides(&self) -> bool {
        false
    }
    fn ambiguous_outer(&self) -> bool {
        false
    }
}

/// Returns the first boundary after or at `from` in text direction.
/// The start and end of the file are the chars `'\0'`.
fn try_find_boundary(
    map: &DisplaySnapshot,
    from: Offset,
    is_boundary: &dyn Fn(char, char) -> bool,
) -> Option<Offset> {
    let boundary = try_find_boundary_data(map, from, |left, right, point| {
        if is_boundary(left, right) {
            Some(point)
        } else {
            None
        }
    })?;
    Some(boundary)
}

/// Returns some information about it (of type `T`) as soon as
/// there is a boundary after or at `from` in text direction
/// The start and end of the file are the chars `'\0'`.
fn try_find_boundary_data<T>(
    map: &DisplaySnapshot,
    mut from: Offset,
    boundary_information: impl Fn(char, char, Offset) -> Option<T>,
) -> Option<T> {
    let mut prev_ch = map
        .buffer_snapshot()
        .reversed_chars_at(from.0)
        .next()
        .unwrap_or('\0');

    for ch in map.buffer_snapshot().chars_at(from.0).chain(['\0']) {
        if let Some(boundary_information) = boundary_information(prev_ch, ch, from) {
            return Some(boundary_information);
        }
        from.0 += ch.len_utf8();
        prev_ch = ch;
    }

    None
}

/// Returns the first boundary after or at `from` in text direction.
/// The start and end of the file are the chars `'\0'`.
fn try_find_preceding_boundary(
    map: &DisplaySnapshot,
    from: Offset,
    is_boundary: &dyn Fn(char, char) -> bool,
) -> Option<Offset> {
    let boundary = try_find_preceding_boundary_data(map, from, |left, right, point| {
        if is_boundary(left, right) {
            Some(point)
        } else {
            None
        }
    })?;
    Some(boundary)
}

/// Returns some information about it (of type `T`) as soon as
/// there is a boundary before or at `from` in opposite text direction
/// The start and end of the file are the chars `'\0'`.
fn try_find_preceding_boundary_data<T>(
    map: &DisplaySnapshot,
    mut from: Offset,
    is_boundary: impl Fn(char, char, Offset) -> Option<T>,
) -> Option<T> {
    let mut prev_ch = map
        .buffer_snapshot()
        .chars_at(from.0)
        .next()
        .unwrap_or('\0');

    for ch in map
        .buffer_snapshot()
        .reversed_chars_at(from.0)
        .chain(['\0'])
    {
        if let Some(boundary_information) = is_boundary(ch, prev_ch, from) {
            return Some(boundary_information);
        }
        from.0.0 = from.0.0.saturating_sub(ch.len_utf8());
        prev_ch = ch;
    }

    None
}
