use super::*;

impl Motion {
    pub fn move_point(
        &self,
        map: &DisplaySnapshot,
        point: DisplayPoint,
        goal: SelectionGoal,
        maybe_times: Option<usize>,
        text_layout_details: &TextLayoutDetails,
    ) -> Option<(DisplayPoint, SelectionGoal)> {
        let times = maybe_times.unwrap_or(1);
        use Motion::*;
        let infallible = self.infallible();
        let (new_point, goal) = match self {
            Left => (left(map, point, times), SelectionGoal::None),
            WrappingLeft => (wrapping_left(map, point, times), SelectionGoal::None),
            Down {
                display_lines: false,
            } => up_down_buffer_rows(map, point, goal, times as isize, text_layout_details),
            Down {
                display_lines: true,
            } => down_display(map, point, goal, times, text_layout_details),
            Up {
                display_lines: false,
            } => up_down_buffer_rows(map, point, goal, 0 - times as isize, text_layout_details),
            Up {
                display_lines: true,
            } => up_display(map, point, goal, times, text_layout_details),
            Right => (right(map, point, times), SelectionGoal::None),
            WrappingRight => (wrapping_right(map, point, times), SelectionGoal::None),
            NextWordStart { ignore_punctuation } => (
                next_word_start(map, point, *ignore_punctuation, times),
                SelectionGoal::None,
            ),
            NextWordEnd { ignore_punctuation } => (
                next_word_end(map, point, *ignore_punctuation, times, true, true),
                SelectionGoal::None,
            ),
            PreviousWordStart { ignore_punctuation } => (
                previous_word_start(map, point, *ignore_punctuation, times),
                SelectionGoal::None,
            ),
            PreviousWordEnd { ignore_punctuation } => (
                previous_word_end(map, point, *ignore_punctuation, times),
                SelectionGoal::None,
            ),
            NextSubwordStart { ignore_punctuation } => (
                next_subword_start(map, point, *ignore_punctuation, times),
                SelectionGoal::None,
            ),
            NextSubwordEnd { ignore_punctuation } => (
                next_subword_end(map, point, *ignore_punctuation, times, true),
                SelectionGoal::None,
            ),
            PreviousSubwordStart { ignore_punctuation } => (
                previous_subword_start(map, point, *ignore_punctuation, times),
                SelectionGoal::None,
            ),
            PreviousSubwordEnd { ignore_punctuation } => (
                previous_subword_end(map, point, *ignore_punctuation, times),
                SelectionGoal::None,
            ),
            FirstNonWhitespace { display_lines } => (
                first_non_whitespace(map, *display_lines, point),
                SelectionGoal::None,
            ),
            StartOfLine { display_lines } => (
                start_of_line(map, *display_lines, point),
                SelectionGoal::None,
            ),
            MiddleOfLine { display_lines } => (
                middle_of_line(map, *display_lines, point, maybe_times),
                SelectionGoal::None,
            ),
            EndOfLine { display_lines } => (
                end_of_line(map, *display_lines, point, times),
                SelectionGoal::HorizontalPosition(f64::INFINITY),
            ),
            SentenceBackward => (sentence_backwards(map, point, times), SelectionGoal::None),
            SentenceForward => (sentence_forwards(map, point, times), SelectionGoal::None),
            StartOfParagraph => (start_of_paragraph(map, point, times), SelectionGoal::None),
            EndOfParagraph => (
                map.clip_at_line_end(end_of_paragraph(map, point, times)),
                SelectionGoal::None,
            ),
            CurrentLine => (next_line_end(map, point, times), SelectionGoal::None),
            StartOfDocument => (
                start_of_document(map, point, maybe_times),
                SelectionGoal::None,
            ),
            EndOfDocument => (
                end_of_document(map, point, maybe_times),
                SelectionGoal::None,
            ),
            Matching { match_quotes } => (matching(map, point, *match_quotes), SelectionGoal::None),
            GoToPercentage => (go_to_percentage(map, point, times), SelectionGoal::None),
            UnmatchedForward { char } => (
                unmatched_forward(map, point, *char, times),
                SelectionGoal::None,
            ),
            UnmatchedBackward { char } => (
                unmatched_backward(map, point, *char, times),
                SelectionGoal::None,
            ),
            // t f
            FindForward {
                before,
                char,
                mode,
                smartcase,
            } => {
                return find_forward(map, point, *before, *char, times, *mode, *smartcase)
                    .map(|new_point| (new_point, SelectionGoal::None));
            }
            // T F
            FindBackward {
                after,
                char,
                mode,
                smartcase,
            } => (
                find_backward(map, point, *after, *char, times, *mode, *smartcase),
                SelectionGoal::None,
            ),
            Sneak {
                first_char,
                second_char,
                smartcase,
            } => {
                return sneak(map, point, *first_char, *second_char, times, *smartcase)
                    .map(|new_point| (new_point, SelectionGoal::None));
            }
            SneakBackward {
                first_char,
                second_char,
                smartcase,
            } => {
                return sneak_backward(map, point, *first_char, *second_char, times, *smartcase)
                    .map(|new_point| (new_point, SelectionGoal::None));
            }
            // ; -- repeat the last find done with t, f, T, F
            RepeatFind { last_find } => match **last_find {
                Motion::FindForward {
                    before,
                    char,
                    mode,
                    smartcase,
                } => {
                    let mut new_point =
                        find_forward(map, point, before, char, times, mode, smartcase);
                    if new_point == Some(point) {
                        new_point =
                            find_forward(map, point, before, char, times + 1, mode, smartcase);
                    }

                    return new_point.map(|new_point| (new_point, SelectionGoal::None));
                }

                Motion::FindBackward {
                    after,
                    char,
                    mode,
                    smartcase,
                } => {
                    let mut new_point =
                        find_backward(map, point, after, char, times, mode, smartcase);
                    if new_point == point {
                        new_point =
                            find_backward(map, point, after, char, times + 1, mode, smartcase);
                    }

                    (new_point, SelectionGoal::None)
                }
                Motion::Sneak {
                    first_char,
                    second_char,
                    smartcase,
                } => {
                    let mut new_point =
                        sneak(map, point, first_char, second_char, times, smartcase);
                    if new_point == Some(point) {
                        new_point =
                            sneak(map, point, first_char, second_char, times + 1, smartcase);
                    }

                    return new_point.map(|new_point| (new_point, SelectionGoal::None));
                }

                Motion::SneakBackward {
                    first_char,
                    second_char,
                    smartcase,
                } => {
                    let mut new_point =
                        sneak_backward(map, point, first_char, second_char, times, smartcase);
                    if new_point == Some(point) {
                        new_point = sneak_backward(
                            map,
                            point,
                            first_char,
                            second_char,
                            times + 1,
                            smartcase,
                        );
                    }

                    return new_point.map(|new_point| (new_point, SelectionGoal::None));
                }
                _ => return None,
            },
            // , -- repeat the last find done with t, f, T, F, s, S, in opposite direction
            RepeatFindReversed { last_find } => match **last_find {
                Motion::FindForward {
                    before,
                    char,
                    mode,
                    smartcase,
                } => {
                    let mut new_point =
                        find_backward(map, point, before, char, times, mode, smartcase);
                    if new_point == point {
                        new_point =
                            find_backward(map, point, before, char, times + 1, mode, smartcase);
                    }

                    (new_point, SelectionGoal::None)
                }

                Motion::FindBackward {
                    after,
                    char,
                    mode,
                    smartcase,
                } => {
                    let mut new_point =
                        find_forward(map, point, after, char, times, mode, smartcase);
                    if new_point == Some(point) {
                        new_point =
                            find_forward(map, point, after, char, times + 1, mode, smartcase);
                    }

                    return new_point.map(|new_point| (new_point, SelectionGoal::None));
                }

                Motion::Sneak {
                    first_char,
                    second_char,
                    smartcase,
                } => {
                    let mut new_point =
                        sneak_backward(map, point, first_char, second_char, times, smartcase);
                    if new_point == Some(point) {
                        new_point = sneak_backward(
                            map,
                            point,
                            first_char,
                            second_char,
                            times + 1,
                            smartcase,
                        );
                    }

                    return new_point.map(|new_point| (new_point, SelectionGoal::None));
                }

                Motion::SneakBackward {
                    first_char,
                    second_char,
                    smartcase,
                } => {
                    let mut new_point =
                        sneak(map, point, first_char, second_char, times, smartcase);
                    if new_point == Some(point) {
                        new_point =
                            sneak(map, point, first_char, second_char, times + 1, smartcase);
                    }

                    return new_point.map(|new_point| (new_point, SelectionGoal::None));
                }
                _ => return None,
            },
            NextLineStart => (next_line_start(map, point, times), SelectionGoal::None),
            PreviousLineStart => (previous_line_start(map, point, times), SelectionGoal::None),
            StartOfLineDownward => (next_line_start(map, point, times - 1), SelectionGoal::None),
            EndOfLineDownward => (last_non_whitespace(map, point, times), SelectionGoal::None),
            GoToColumn => (go_to_column(map, point, times), SelectionGoal::None),
            WindowTop => window_top(map, point, text_layout_details, times - 1),
            WindowMiddle => window_middle(map, point, text_layout_details),
            WindowBottom => window_bottom(map, point, text_layout_details, times - 1),
            Jump { line, anchor } => mark::jump_motion(map, *anchor, *line),
            MavSearchResult { new_selections, .. } => {
                // There will be only one selection, as
                // Search::SelectNextMatch selects a single match.
                if let Some(new_selection) = new_selections.first() {
                    (
                        new_selection.start.to_display_point(map),
                        SelectionGoal::None,
                    )
                } else {
                    return None;
                }
            }
            NextSectionStart => (
                section_motion(map, point, times, Direction::Next, true),
                SelectionGoal::None,
            ),
            NextSectionEnd => (
                section_motion(map, point, times, Direction::Next, false),
                SelectionGoal::None,
            ),
            PreviousSectionStart => (
                section_motion(map, point, times, Direction::Prev, true),
                SelectionGoal::None,
            ),
            PreviousSectionEnd => (
                section_motion(map, point, times, Direction::Prev, false),
                SelectionGoal::None,
            ),

            NextMethodStart => (
                method_motion(map, point, times, Direction::Next, true),
                SelectionGoal::None,
            ),
            NextMethodEnd => (
                method_motion(map, point, times, Direction::Next, false),
                SelectionGoal::None,
            ),
            PreviousMethodStart => (
                method_motion(map, point, times, Direction::Prev, true),
                SelectionGoal::None,
            ),
            PreviousMethodEnd => (
                method_motion(map, point, times, Direction::Prev, false),
                SelectionGoal::None,
            ),
            NextComment => (
                comment_motion(map, point, times, Direction::Next),
                SelectionGoal::None,
            ),
            PreviousComment => (
                comment_motion(map, point, times, Direction::Prev),
                SelectionGoal::None,
            ),
            PreviousLesserIndent => (
                indent_motion(map, point, times, Direction::Prev, IndentType::Lesser),
                SelectionGoal::None,
            ),
            PreviousGreaterIndent => (
                indent_motion(map, point, times, Direction::Prev, IndentType::Greater),
                SelectionGoal::None,
            ),
            PreviousSameIndent => (
                indent_motion(map, point, times, Direction::Prev, IndentType::Same),
                SelectionGoal::None,
            ),
            NextLesserIndent => (
                indent_motion(map, point, times, Direction::Next, IndentType::Lesser),
                SelectionGoal::None,
            ),
            NextGreaterIndent => (
                indent_motion(map, point, times, Direction::Next, IndentType::Greater),
                SelectionGoal::None,
            ),
            NextSameIndent => (
                indent_motion(map, point, times, Direction::Next, IndentType::Same),
                SelectionGoal::None,
            ),
        };
        (new_point != point || infallible).then_some((new_point, goal))
    }
}
