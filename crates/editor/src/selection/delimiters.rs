use super::*;

impl Editor {
    pub fn select_inside_delimiters(
        &mut self,
        _: &SelectInsideDelimiters,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_delimiters_impl(false, window, cx);
    }

    pub fn select_around_delimiters(
        &mut self,
        _: &SelectAroundDelimiters,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_delimiters_impl(true, window, cx);
    }

    fn select_delimiters_impl(
        &mut self,
        include_brackets: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_selections(Default::default(), window, cx, |s| {
            s.move_offsets_with(&mut |snapshot, selection| {
                let Some(enclosing_bracket_ranges) =
                    snapshot.enclosing_bracket_ranges(selection.start..selection.end)
                else {
                    return;
                };

                let mut best = None;
                let mut best_length = usize::MAX;

                for (open, close) in enclosing_bracket_ranges {
                    let range = if include_brackets {
                        open.start..close.end
                    } else {
                        open.end..close.start
                    };

                    // Skip any bracket pair that is already covered by the
                    // selection so repeated uses of delimiters selection only
                    // evere expands outwards to the next pair.
                    if (selection.start..selection.end).contains_inclusive(&range) {
                        continue;
                    }

                    let length = close.end - open.start;
                    if length < best_length {
                        best_length = length;
                        best = Some(range);
                    }
                }

                if let Some(range) = best {
                    selection.set_head_tail(range.end, range.start, SelectionGoal::None);
                }
            })
        });
    }

    pub fn move_to_enclosing_bracket(
        &mut self,
        _: &MoveToEnclosingBracket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_selections(Default::default(), window, cx, |s| {
            s.move_offsets_with(&mut |snapshot, selection| {
                let Some(enclosing_bracket_ranges) =
                    snapshot.enclosing_bracket_ranges(selection.start..selection.end)
                else {
                    return;
                };

                let mut best_length = usize::MAX;
                let mut best_inside = false;
                let mut best_in_bracket_range = false;
                let mut best_destination = None;
                for (open, close) in enclosing_bracket_ranges {
                    let close = close.to_inclusive();
                    let length = *close.end() - open.start;
                    let inside = selection.start >= open.end && selection.end <= *close.start();
                    let in_bracket_range = open.to_inclusive().contains(&selection.head())
                        || close.contains(&selection.head());

                    // If best is next to a bracket and current isn't, skip
                    if !in_bracket_range && best_in_bracket_range {
                        continue;
                    }

                    // Prefer smaller lengths unless best is inside and current isn't
                    if length > best_length && (best_inside || !inside) {
                        continue;
                    }

                    best_length = length;
                    best_inside = inside;
                    best_in_bracket_range = in_bracket_range;
                    best_destination = Some(
                        if close.contains(&selection.start) && close.contains(&selection.end) {
                            if inside { open.end } else { open.start }
                        } else if inside {
                            *close.start()
                        } else {
                            *close.end()
                        },
                    );
                }

                if let Some(destination) = best_destination {
                    selection.collapse_to(destination, SelectionGoal::None);
                }
            })
        });
    }
}
