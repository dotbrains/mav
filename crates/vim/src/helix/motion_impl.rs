use super::*;

impl Vim {
    pub fn helix_normal_motion(
        &mut self,
        motion: Motion,
        times: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.helix_move_cursor(motion, times, window, cx);
    }

    pub fn helix_select_motion(
        &mut self,
        motion: Motion,
        times: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_editor(cx, |_, editor, cx| {
            let text_layout_details = editor.text_layout_details(window, cx);
            editor.change_selections(Default::default(), window, cx, |s| {
                if let Motion::MavSearchResult { new_selections, .. } = &motion {
                    s.select_anchor_ranges(new_selections.clone());
                    return;
                };

                s.move_with(&mut |map, selection| {
                    let was_reversed = selection.reversed;
                    let mut current_head = selection.head();

                    // our motions assume the current character is after the cursor,
                    // but in (forward) visual mode the current character is just
                    // before the end of the selection.

                    // If the file ends with a newline (which is common) we don't do this.
                    // so that if you go to the end of such a file you can use "up" to go
                    // to the previous line and have it work somewhat as expected.
                    if !selection.reversed
                        && !selection.is_empty()
                        && !(selection.end.column() == 0 && selection.end == map.max_point())
                    {
                        current_head = movement::left(map, selection.end)
                    }

                    let (new_head, goal) = match motion {
                        Motion::StartOfDocument => {
                            (start_of_document(map, times), SelectionGoal::None)
                        }
                        Motion::EndOfDocument => (end_of_document(map), SelectionGoal::None),
                        // EndOfLine positions after the last character, but in
                        // helix visual mode we want the selection to end ON the
                        // last character. Adjust left here so the subsequent
                        // right-expansion (below) includes the last char without
                        // spilling into the newline.
                        Motion::EndOfLine { .. } => {
                            let (point, goal) = motion
                                .move_point(
                                    map,
                                    current_head,
                                    selection.goal,
                                    times,
                                    &text_layout_details,
                                )
                                .unwrap_or((current_head, selection.goal));
                            (movement::saturating_left(map, point), goal)
                        }
                        // Going to next word start is special cased
                        // since Vim differs from Helix in that motion
                        // Vim: `w` goes to the first character of a word
                        // Helix: `w` goes to the character before a word
                        Motion::NextWordStart { ignore_punctuation } => {
                            let mut head = movement::right(map, current_head);
                            let classifier =
                                map.buffer_snapshot().char_classifier_at(head.to_point(map));
                            for _ in 0..times.unwrap_or(1) {
                                let (_, new_head) =
                                    movement::find_boundary_trail(map, head, &mut |left, right| {
                                        Self::is_boundary_right(ignore_punctuation)(
                                            left,
                                            right,
                                            &classifier,
                                        )
                                    });
                                head = new_head;
                            }
                            head = movement::left(map, head);
                            (head, SelectionGoal::None)
                        }
                        _ => motion
                            .move_point(
                                map,
                                current_head,
                                selection.goal,
                                times,
                                &text_layout_details,
                            )
                            .unwrap_or((current_head, selection.goal)),
                    };

                    selection.set_head(new_head, goal);

                    // ensure the current character is included in the selection.
                    if !selection.reversed {
                        let next_point = movement::right(map, selection.end);

                        if !(next_point.column() == 0 && next_point == map.max_point()) {
                            selection.end = next_point;
                        }
                    }

                    // vim always ensures the anchor character stays selected.
                    // if our selection has reversed, we need to move the opposite end
                    // to ensure the anchor is still selected.
                    if was_reversed && !selection.reversed {
                        selection.start = movement::left(map, selection.start);
                    } else if !was_reversed && selection.reversed {
                        selection.end = movement::right(map, selection.end);
                    }
                })
            });
        });
    }

    pub fn helix_move_cursor(
        &mut self,
        motion: Motion,
        times: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match motion {
            Motion::NextWordStart { ignore_punctuation } => {
                let mut is_boundary = Self::is_boundary_right(ignore_punctuation);
                self.helix_find_range_forward(times, window, cx, &mut is_boundary)
            }
            Motion::NextWordEnd { ignore_punctuation } => {
                let mut is_boundary = Self::is_boundary_left(ignore_punctuation);
                self.helix_find_range_forward(times, window, cx, &mut is_boundary)
            }
            Motion::PreviousWordStart { ignore_punctuation } => {
                let mut is_boundary = Self::is_boundary_left(ignore_punctuation);
                self.helix_find_range_backward(times, window, cx, &mut is_boundary)
            }
            Motion::PreviousWordEnd { ignore_punctuation } => {
                let mut is_boundary = Self::is_boundary_right(ignore_punctuation);
                self.helix_find_range_backward(times, window, cx, &mut is_boundary)
            }
            // The subword motions implementation is based off of the same
            // commands present in Helix itself, namely:
            //
            // * `move_next_sub_word_start`
            // * `move_next_sub_word_end`
            // * `move_prev_sub_word_start`
            // * `move_prev_sub_word_end`
            Motion::NextSubwordStart { ignore_punctuation } => {
                let mut is_boundary = Self::subword_boundary_start(ignore_punctuation, false);
                self.helix_find_range_forward(times, window, cx, &mut is_boundary)
            }
            Motion::NextSubwordEnd { ignore_punctuation } => {
                let mut is_boundary = Self::subword_boundary_end(ignore_punctuation, false);
                self.helix_find_range_forward(times, window, cx, &mut is_boundary)
            }
            Motion::PreviousSubwordStart { ignore_punctuation } => {
                let mut is_boundary = Self::subword_boundary_end(ignore_punctuation, true);
                self.helix_find_range_backward(times, window, cx, &mut is_boundary)
            }
            Motion::PreviousSubwordEnd { ignore_punctuation } => {
                let mut is_boundary = Self::subword_boundary_start(ignore_punctuation, true);
                self.helix_find_range_backward(times, window, cx, &mut is_boundary)
            }
            Motion::StartOfDocument => {
                self.update_editor(cx, |_, editor, cx| {
                    editor.change_selections(Default::default(), window, cx, |s| {
                        s.move_with(&mut |map, selection| {
                            selection
                                .collapse_to(start_of_document(map, times), SelectionGoal::None)
                        })
                    });
                });
            }
            Motion::EndOfDocument => {
                self.update_editor(cx, |_, editor, cx| {
                    editor.change_selections(Default::default(), window, cx, |s| {
                        s.move_with(&mut |map, selection| {
                            selection.collapse_to(end_of_document(map), SelectionGoal::None)
                        })
                    });
                });
            }
            Motion::EndOfLine { .. } => {
                // In Helix mode, EndOfLine should position cursor ON the last character,
                // not after it. We therefore need special handling for it.
                self.update_editor(cx, |_, editor, cx| {
                    let text_layout_details = editor.text_layout_details(window, cx);
                    editor.change_selections(Default::default(), window, cx, |s| {
                        s.move_with(&mut |map, selection| {
                            let goal = selection.goal;
                            let cursor = if selection.is_empty() || selection.reversed {
                                selection.head()
                            } else {
                                movement::left(map, selection.head())
                            };

                            let (point, _goal) = motion
                                .move_point(map, cursor, goal, times, &text_layout_details)
                                .unwrap_or((cursor, goal));

                            // Move left by one character to position on the last character
                            let adjusted_point = movement::saturating_left(map, point);
                            selection.collapse_to(adjusted_point, SelectionGoal::None)
                        })
                    });
                });
            }
            Motion::FindForward {
                before,
                char,
                mode,
                smartcase,
            } => {
                self.helix_new_selections(window, cx, &mut |cursor, map| {
                    let start = cursor;
                    let mut last_boundary = start;
                    for _ in 0..times.unwrap_or(1) {
                        last_boundary = movement::find_boundary(
                            map,
                            movement::right(map, last_boundary),
                            mode,
                            &mut |left, right| {
                                let current_char = if before { right } else { left };
                                motion::is_character_match(char, current_char, smartcase)
                            },
                        );
                    }
                    Some((last_boundary, start))
                });
            }
            Motion::FindBackward {
                after,
                char,
                mode,
                smartcase,
            } => {
                self.helix_new_selections(window, cx, &mut |cursor, map| {
                    let start = cursor;
                    let mut last_boundary = start;
                    for _ in 0..times.unwrap_or(1) {
                        last_boundary = movement::find_preceding_boundary_display_point(
                            map,
                            last_boundary,
                            mode,
                            &mut |left, right| {
                                let current_char = if after { left } else { right };
                                motion::is_character_match(char, current_char, smartcase)
                            },
                        );
                    }
                    // The original cursor was one character wide,
                    // but the search started from the left side of it,
                    // so to include that space the selection must end one character to the right.
                    Some((last_boundary, movement::right(map, start)))
                });
            }
            _ => self.helix_move_and_collapse(motion, times, window, cx),
        }
    }

    pub fn helix_yank(&mut self, _: &HelixYank, window: &mut Window, cx: &mut Context<Self>) {
        self.update_editor(cx, |vim, editor, cx| {
            let has_selection = editor
                .selections
                .all_adjusted(&editor.display_snapshot(cx))
                .iter()
                .any(|selection| !selection.is_empty());

            if !has_selection {
                // If no selection, expand to current character (like 'v' does)
                editor.change_selections(Default::default(), window, cx, |s| {
                    s.move_with(&mut |map, selection| {
                        let head = selection.head();
                        let new_head = movement::saturating_right(map, head);
                        selection.set_tail(head, SelectionGoal::None);
                        selection.set_head(new_head, SelectionGoal::None);
                    });
                });
                vim.yank_selections_content(
                    editor,
                    crate::motion::MotionKind::Exclusive,
                    window,
                    cx,
                );
                editor.change_selections(Default::default(), window, cx, |s| {
                    s.move_with(&mut |_map, selection| {
                        selection.collapse_to(selection.start, SelectionGoal::None);
                    });
                });
            } else {
                // Yank the selection(s)
                vim.yank_selections_content(
                    editor,
                    crate::motion::MotionKind::Exclusive,
                    window,
                    cx,
                );
            }
        });

        // Drop back to normal mode after yanking
        self.switch_mode(Mode::HelixNormal, true, window, cx);
    }
}
