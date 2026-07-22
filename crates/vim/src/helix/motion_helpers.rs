use super::*;

impl Vim {
    /// Updates all selections based on where the cursors are.
    pub(super) fn helix_new_selections(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: &mut dyn FnMut(
            // the start of the cursor
            DisplayPoint,
            &DisplaySnapshot,
        ) -> Option<(DisplayPoint, DisplayPoint)>,
    ) {
        self.update_editor(cx, |_, editor, cx| {
            editor.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |map, selection| {
                    let cursor_start = if selection.reversed || selection.is_empty() {
                        selection.head()
                    } else {
                        movement::left(map, selection.head())
                    };
                    let Some((head, tail)) = change(cursor_start, map) else {
                        return;
                    };

                    selection.set_head_tail(head, tail, SelectionGoal::None);
                });
            });
        });
    }

    pub(super) fn helix_find_range_forward(
        &mut self,
        times: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
        is_boundary: &mut dyn FnMut(char, char, &CharClassifier) -> bool,
    ) {
        let times = times.unwrap_or(1);
        self.helix_new_selections(window, cx, &mut |cursor, map| {
            let mut head = movement::right(map, cursor);
            let mut tail = cursor;
            let classifier = map.buffer_snapshot().char_classifier_at(head.to_point(map));
            if head == map.max_point() {
                return None;
            }
            for _ in 0..times {
                let (maybe_next_tail, next_head) =
                    movement::find_boundary_trail(map, head, &mut |left, right| {
                        is_boundary(left, right, &classifier)
                    });

                if next_head == head && maybe_next_tail.unwrap_or(next_head) == tail {
                    break;
                }

                head = next_head;
                if let Some(next_tail) = maybe_next_tail {
                    tail = next_tail;
                }
            }
            Some((head, tail))
        });
    }

    pub(super) fn helix_find_range_backward(
        &mut self,
        times: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
        is_boundary: &mut dyn FnMut(char, char, &CharClassifier) -> bool,
    ) {
        let times = times.unwrap_or(1);
        self.helix_new_selections(window, cx, &mut |cursor, map| {
            let mut head = cursor;
            // The original cursor was one character wide,
            // but the search starts from the left side of it,
            // so to include that space the selection must end one character to the right.
            let mut tail = movement::right(map, cursor);
            let classifier = map.buffer_snapshot().char_classifier_at(head.to_point(map));
            if head == DisplayPoint::zero() {
                return None;
            }
            for _ in 0..times {
                let (maybe_next_tail, next_head) =
                    movement::find_preceding_boundary_trail(map, head, &mut |left, right| {
                        is_boundary(left, right, &classifier)
                    });

                if next_head == head && maybe_next_tail.unwrap_or(next_head) == tail {
                    break;
                }

                head = next_head;
                if let Some(next_tail) = maybe_next_tail {
                    tail = next_tail;
                }
            }
            Some((head, tail))
        });
    }

    pub fn helix_move_and_collapse(
        &mut self,
        motion: Motion,
        times: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

                    let (point, goal) = motion
                        .move_point(map, cursor, selection.goal, times, &text_layout_details)
                        .unwrap_or((cursor, goal));

                    selection.collapse_to(point, goal)
                })
            });
        });
    }

    pub(super) fn is_boundary_right(
        ignore_punctuation: bool,
    ) -> impl FnMut(char, char, &CharClassifier) -> bool {
        move |left, right, classifier| {
            let left_kind = classifier.kind_with(left, ignore_punctuation);
            let right_kind = classifier.kind_with(right, ignore_punctuation);
            let at_newline = (left == '\n') ^ (right == '\n');

            (left_kind != right_kind && right_kind != CharKind::Whitespace) || at_newline
        }
    }

    pub(super) fn is_boundary_left(
        ignore_punctuation: bool,
    ) -> impl FnMut(char, char, &CharClassifier) -> bool {
        move |left, right, classifier| {
            let left_kind = classifier.kind_with(left, ignore_punctuation);
            let right_kind = classifier.kind_with(right, ignore_punctuation);
            let at_newline = (left == '\n') ^ (right == '\n');

            (left_kind != right_kind && left_kind != CharKind::Whitespace) || at_newline
        }
    }

    /// When `reversed` is true (used with `helix_find_range_backward`), the
    /// `left` and `right` characters are yielded in reverse text order, so the
    /// camelCase transition check must be flipped accordingly.
    pub(super) fn subword_boundary_start(
        ignore_punctuation: bool,
        reversed: bool,
    ) -> impl FnMut(char, char, &CharClassifier) -> bool {
        move |left, right, classifier| {
            let left_kind = classifier.kind_with(left, ignore_punctuation);
            let right_kind = classifier.kind_with(right, ignore_punctuation);
            let at_newline = (left == '\n') ^ (right == '\n');
            let is_separator = |c: char| "_$=".contains(c);

            let is_word = left_kind != right_kind && right_kind != CharKind::Whitespace;
            let is_subword = (is_separator(left) && !is_separator(right))
                || if reversed {
                    right.is_lowercase() && left.is_uppercase()
                } else {
                    left.is_lowercase() && right.is_uppercase()
                };

            is_word || (is_subword && !right.is_whitespace()) || at_newline
        }
    }

    /// When `reversed` is true (used with `helix_find_range_backward`), the
    /// `left` and `right` characters are yielded in reverse text order, so the
    /// camelCase transition check must be flipped accordingly.
    pub(super) fn subword_boundary_end(
        ignore_punctuation: bool,
        reversed: bool,
    ) -> impl FnMut(char, char, &CharClassifier) -> bool {
        move |left, right, classifier| {
            let left_kind = classifier.kind_with(left, ignore_punctuation);
            let right_kind = classifier.kind_with(right, ignore_punctuation);
            let at_newline = (left == '\n') ^ (right == '\n');
            let is_separator = |c: char| "_$=".contains(c);

            let is_word = left_kind != right_kind && left_kind != CharKind::Whitespace;
            let is_subword = (!is_separator(left) && is_separator(right))
                || if reversed {
                    right.is_lowercase() && left.is_uppercase()
                } else {
                    left.is_lowercase() && right.is_uppercase()
                };

            is_word || (is_subword && !left.is_whitespace()) || at_newline
        }
    }
}
