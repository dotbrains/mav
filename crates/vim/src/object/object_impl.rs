use super::*;

impl Object {
    pub fn is_multiline(self) -> bool {
        match self {
            Object::Word { .. }
            | Object::Subword { .. }
            | Object::Quotes
            | Object::BackQuotes
            | Object::AnyQuotes
            | Object::MiniQuotes
            | Object::VerticalBars
            | Object::DoubleQuotes => false,
            Object::Sentence
            | Object::Paragraph
            | Object::AnyBrackets
            | Object::MiniBrackets
            | Object::Parentheses
            | Object::Tag
            | Object::AngleBrackets
            | Object::CurlyBrackets
            | Object::SquareBrackets
            | Object::Argument
            | Object::Method
            | Object::Class
            | Object::EntireFile
            | Object::Comment
            | Object::IndentObj { .. } => true,
        }
    }

    pub fn always_expands_both_ways(self) -> bool {
        match self {
            Object::Word { .. }
            | Object::Subword { .. }
            | Object::Sentence
            | Object::Paragraph
            | Object::Argument
            | Object::IndentObj { .. } => false,
            Object::Quotes
            | Object::BackQuotes
            | Object::AnyQuotes
            | Object::MiniQuotes
            | Object::DoubleQuotes
            | Object::VerticalBars
            | Object::AnyBrackets
            | Object::MiniBrackets
            | Object::Parentheses
            | Object::SquareBrackets
            | Object::Tag
            | Object::Method
            | Object::Class
            | Object::Comment
            | Object::EntireFile
            | Object::CurlyBrackets
            | Object::AngleBrackets => true,
        }
    }

    pub fn target_visual_mode(self, current_mode: Mode, around: bool) -> Mode {
        match self {
            Object::Word { .. }
            | Object::Subword { .. }
            | Object::Sentence
            | Object::Quotes
            | Object::AnyQuotes
            | Object::MiniQuotes
            | Object::BackQuotes
            | Object::DoubleQuotes => {
                if current_mode == Mode::VisualBlock {
                    Mode::VisualBlock
                } else {
                    Mode::Visual
                }
            }
            Object::Parentheses
            | Object::AnyBrackets
            | Object::MiniBrackets
            | Object::SquareBrackets
            | Object::CurlyBrackets
            | Object::AngleBrackets
            | Object::VerticalBars
            | Object::Tag
            | Object::Comment
            | Object::Argument
            | Object::IndentObj { .. } => Mode::Visual,
            Object::Method | Object::Class => {
                if around {
                    Mode::VisualLine
                } else {
                    Mode::Visual
                }
            }
            Object::Paragraph | Object::EntireFile => Mode::VisualLine,
        }
    }

    pub fn range(
        self,
        map: &DisplaySnapshot,
        selection: Selection<DisplayPoint>,
        around: bool,
        times: Option<usize>,
    ) -> Option<Range<DisplayPoint>> {
        let relative_to = selection.head();
        match self {
            Object::Word { ignore_punctuation } => {
                let count = times.unwrap_or(1);
                if around {
                    around_word(map, relative_to, ignore_punctuation, count)
                } else {
                    in_word(map, relative_to, ignore_punctuation, count).map(|range| {
                        // For iw with count > 1, vim includes trailing whitespace
                        if count > 1 {
                            let spans_multiple_lines = range.start.row() != range.end.row();
                            expand_to_include_whitespace(map, range, !spans_multiple_lines)
                        } else {
                            range
                        }
                    })
                }
            }
            Object::Subword { ignore_punctuation } => {
                if around {
                    around_subword(map, relative_to, ignore_punctuation)
                } else {
                    in_subword(map, relative_to, ignore_punctuation)
                }
            }
            Object::Sentence => sentence(map, relative_to, around),
            //change others later
            Object::Paragraph => paragraph(map, relative_to, around, times.unwrap_or(1)),
            Object::Quotes => {
                surrounding_markers(map, relative_to, around, self.is_multiline(), '\'', '\'')
            }
            Object::BackQuotes => {
                surrounding_markers(map, relative_to, around, self.is_multiline(), '`', '`')
            }
            Object::AnyQuotes => {
                let cursor_offset = relative_to.to_offset(map, Bias::Left);

                // Find innermost range directly without collecting all ranges
                let mut innermost = None;
                let mut min_size = usize::MAX;

                // First pass: find innermost enclosing range
                for &SurroundPair { open, close } in QUOTE_PAIRS {
                    if let Some(range) = surrounding_markers(
                        map,
                        relative_to,
                        around,
                        self.is_multiline(),
                        open,
                        close,
                    ) {
                        let start_offset = range.start.to_offset(map, Bias::Left);
                        let end_offset = range.end.to_offset(map, Bias::Right);

                        if cursor_offset >= start_offset && cursor_offset <= end_offset {
                            let size = end_offset - start_offset;
                            if size < min_size {
                                min_size = size;
                                innermost = Some(range);
                            }
                        }
                    }
                }

                if let Some(range) = innermost {
                    return Some(range);
                }

                // Fallback: find nearest pair if not inside any quotes
                QUOTE_PAIRS
                    .iter()
                    .flat_map(|&SurroundPair { open, close }| {
                        surrounding_markers(
                            map,
                            relative_to,
                            around,
                            self.is_multiline(),
                            open,
                            close,
                        )
                    })
                    .min_by_key(|range| {
                        let start_offset = range.start.to_offset(map, Bias::Left);
                        let end_offset = range.end.to_offset(map, Bias::Right);
                        if cursor_offset < start_offset {
                            (start_offset - cursor_offset) as isize
                        } else if cursor_offset > end_offset {
                            (cursor_offset - end_offset) as isize
                        } else {
                            0
                        }
                    })
            }
            Object::MiniQuotes => delimiters::find_mini_quotes(map, relative_to, around),
            Object::DoubleQuotes => {
                surrounding_markers(map, relative_to, around, self.is_multiline(), '"', '"')
            }
            Object::VerticalBars => {
                surrounding_markers(map, relative_to, around, self.is_multiline(), '|', '|')
            }
            Object::Parentheses => {
                surrounding_markers(map, relative_to, around, self.is_multiline(), '(', ')')
            }
            Object::Tag => {
                let head = selection.head();
                let range = selection.range();
                surrounding_html_tag(map, head, range, around)
            }
            Object::AnyBrackets => {
                let cursor_offset = relative_to.to_offset(map, Bias::Left);

                // Find innermost enclosing bracket range
                let mut innermost = None;
                let mut min_size = usize::MAX;

                for &SurroundPair { open, close } in BRACKET_PAIRS {
                    if let Some(range) = surrounding_markers(
                        map,
                        relative_to,
                        around,
                        self.is_multiline(),
                        open,
                        close,
                    ) {
                        let start_offset = range.start.to_offset(map, Bias::Left);
                        let end_offset = range.end.to_offset(map, Bias::Right);

                        if cursor_offset >= start_offset && cursor_offset <= end_offset {
                            let size = end_offset - start_offset;
                            if size < min_size {
                                min_size = size;
                                innermost = Some(range);
                            }
                        }
                    }
                }

                if let Some(range) = innermost {
                    return Some(range);
                }

                // Fallback: find nearest bracket pair if not inside any
                BRACKET_PAIRS
                    .iter()
                    .flat_map(|&SurroundPair { open, close }| {
                        surrounding_markers(
                            map,
                            relative_to,
                            around,
                            self.is_multiline(),
                            open,
                            close,
                        )
                    })
                    .min_by_key(|range| {
                        let start_offset = range.start.to_offset(map, Bias::Left);
                        let end_offset = range.end.to_offset(map, Bias::Right);
                        if cursor_offset < start_offset {
                            (start_offset - cursor_offset) as isize
                        } else if cursor_offset > end_offset {
                            (cursor_offset - end_offset) as isize
                        } else {
                            0
                        }
                    })
            }
            Object::MiniBrackets => delimiters::find_mini_brackets(map, relative_to, around),
            Object::SquareBrackets => {
                surrounding_markers(map, relative_to, around, self.is_multiline(), '[', ']')
            }
            Object::CurlyBrackets => {
                surrounding_markers(map, relative_to, around, self.is_multiline(), '{', '}')
            }
            Object::AngleBrackets => {
                surrounding_markers(map, relative_to, around, self.is_multiline(), '<', '>')
            }
            Object::Method => text_object(
                map,
                relative_to,
                if around {
                    TextObject::AroundFunction
                } else {
                    TextObject::InsideFunction
                },
            ),
            Object::Comment => text_object(
                map,
                relative_to,
                if around {
                    TextObject::AroundComment
                } else {
                    TextObject::InsideComment
                },
            ),
            Object::Class => text_object(
                map,
                relative_to,
                if around {
                    TextObject::AroundClass
                } else {
                    TextObject::InsideClass
                },
            ),
            Object::Argument => argument(map, relative_to, around),
            Object::IndentObj { include_below } => indent(map, relative_to, around, include_below),
            Object::EntireFile => entire_file(map),
        }
    }

    pub fn expand_selection(
        self,
        map: &DisplaySnapshot,
        selection: &mut Selection<DisplayPoint>,
        around: bool,
        times: Option<usize>,
    ) -> bool {
        if let Some(range) = self.range(map, selection.clone(), around, times) {
            selection.start = range.start;
            selection.end = range.end;
            true
        } else {
            false
        }
    }
}
