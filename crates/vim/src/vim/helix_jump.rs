use super::*;

impl Vim {
    pub(super) fn clear_helix_jump_ui(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.update_editor(cx, move |_, editor, cx| {
            editor.clear_navigation_overlays(HELIX_JUMP_OVERLAY_KEY, cx);
        });
    }

    pub(super) fn apply_helix_jump_ui(
        &mut self,
        overlays: Vec<NavigationTargetOverlay>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.clear_helix_jump_ui(window, cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.set_navigation_overlays(HELIX_JUMP_OVERLAY_KEY, overlays, cx);
        })
        .is_some()
    }

    pub(super) fn handle_helix_jump_input(
        &mut self,
        operator: Operator,
        input_char: char,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Operator::HelixJump {
            behaviour,
            first_char,
            labels,
        } = operator
        else {
            return;
        };

        let input = input_char.to_ascii_lowercase();
        self.pop_operator(window, cx);

        if let Some(first) = first_char {
            let first = first.to_ascii_lowercase();
            if let Some(candidate) = labels.into_iter().find(|label| {
                label.label[0].eq_ignore_ascii_case(&first)
                    && label.label[1].eq_ignore_ascii_case(&input)
            }) {
                self.finish_helix_jump(candidate, behaviour, window, cx);
            } else {
                self.clear_helix_jump_ui(window, cx);
            }
        } else {
            if !labels
                .iter()
                .any(|label| label.label[0].eq_ignore_ascii_case(&input))
            {
                self.clear_helix_jump_ui(window, cx);
                return;
            }

            self.push_operator(
                Operator::HelixJump {
                    behaviour,
                    first_char: Some(input),
                    labels,
                },
                window,
                cx,
            );
        }
    }

    fn finish_helix_jump(
        &mut self,
        candidate: HelixJumpLabel,
        behaviour: HelixJumpBehaviour,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_editor(cx, |_, editor, cx| match behaviour {
            HelixJumpBehaviour::Move => {
                editor.change_selections(Default::default(), window, cx, |s| {
                    s.select_anchor_ranges([candidate.range.clone()])
                });
            }
            HelixJumpBehaviour::MoveToWordStart => {
                editor.change_selections(Default::default(), window, cx, |s| {
                    // Vim users expect jump labels to behave like motions, leaving
                    // normal mode at the label instead of selecting the word.
                    s.select_anchor_ranges([candidate.range.start..candidate.range.start])
                });
            }
            HelixJumpBehaviour::ExtendToWordStart => {
                editor.change_selections(Default::default(), window, cx, |s| {
                    s.move_with(&mut |map, selection| {
                        let word_start = candidate.range.start.to_display_point(map);
                        let tail = selection.tail();

                        if word_start >= tail {
                            selection
                                .set_head(motion::right(map, word_start, 1), SelectionGoal::None);
                        } else {
                            selection.set_head_tail(word_start, selection.end, SelectionGoal::None);
                        }
                    });
                });
            }
            HelixJumpBehaviour::Extend => {
                editor.change_selections(Default::default(), window, cx, |s| {
                    s.move_with(&mut |map, selection| {
                        let word_start = candidate.range.start.to_display_point(map);
                        let word_end = candidate.range.end.to_display_point(map);
                        let tail = selection.tail();

                        if word_start >= tail {
                            // Jumping forward: extend head to end of target word
                            selection.set_head(word_end, SelectionGoal::None);
                        } else {
                            // Jumping backward: extend backward while keeping current extent
                            // Use current end as tail to preserve the selection
                            selection.set_head_tail(word_start, selection.end, SelectionGoal::None);
                        }
                    });
                });
            }
        });
        self.clear_helix_jump_ui(window, cx);
    }

    fn active_operator(&self) -> Option<Operator> {
        self.operator_stack.last().cloned()
    }
}
