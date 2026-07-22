use super::*;

impl Vim {
    pub(crate) fn search_motion(&mut self, m: Motion, window: &mut Window, cx: &mut Context<Self>) {
        if let Motion::MavSearchResult {
            prior_selections, ..
        } = &m
        {
            match self.mode {
                Mode::Visual | Mode::VisualLine | Mode::VisualBlock => {
                    if !prior_selections.is_empty() {
                        self.update_editor(cx, |_, editor, cx| {
                            editor.change_selections(Default::default(), window, cx, |s| {
                                s.select_ranges(prior_selections.iter().cloned())
                            })
                        });
                    }
                }
                Mode::Normal | Mode::Replace | Mode::Insert => {
                    if self.active_operator().is_none() {
                        return;
                    }
                }
                Mode::HelixNormal | Mode::HelixSelect => {}
            }
        }

        self.motion(m, window, cx)
    }

    pub(crate) fn motion(&mut self, motion: Motion, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Operator::FindForward { .. })
        | Some(Operator::Sneak { .. })
        | Some(Operator::SneakBackward { .. })
        | Some(Operator::FindBackward { .. }) = self.active_operator()
        {
            self.pop_operator(window, cx);
        }

        let count = Vim::take_count(cx);
        let forced_motion = Vim::take_forced_motion(cx);
        let active_operator = self.active_operator();
        let mut waiting_operator: Option<Operator> = None;
        match self.mode {
            Mode::Normal | Mode::Replace | Mode::Insert => {
                if active_operator == Some(Operator::AddSurrounds { target: None }) {
                    waiting_operator = Some(Operator::AddSurrounds {
                        target: Some(SurroundsType::Motion(motion)),
                    });
                } else {
                    self.normal_motion(motion, active_operator, count, forced_motion, window, cx)
                }
            }
            Mode::Visual | Mode::VisualLine | Mode::VisualBlock => {
                self.visual_motion(motion, count, window, cx)
            }

            Mode::HelixNormal => self.helix_normal_motion(motion, count, window, cx),
            Mode::HelixSelect => self.helix_select_motion(motion, count, window, cx),
        }
        self.clear_operator(window, cx);
        if let Some(operator) = waiting_operator {
            self.push_operator(operator, window, cx);
            Vim::globals(cx).pre_count = count
        }
    }
}

// Motion handling is specified here:
// https://github.com/vim/vim/blob/master/runtime/doc/motion.txt
