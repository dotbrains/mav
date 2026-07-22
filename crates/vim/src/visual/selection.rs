use super::*;

impl Vim {
    pub fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        Vim::take_forced_motion(cx);
        let count =
            Vim::take_count(cx).unwrap_or_else(|| if self.mode.is_visual() { 1 } else { 2 });
        self.update_editor(cx, |_, editor, cx| {
            editor.set_clip_at_line_ends(false, cx);
            for _ in 0..count {
                if editor
                    .select_next(&Default::default(), window, cx)
                    .log_err()
                    .is_none()
                {
                    break;
                }
            }
        });
    }

    pub fn select_previous(
        &mut self,
        _: &SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Vim::take_forced_motion(cx);
        let count =
            Vim::take_count(cx).unwrap_or_else(|| if self.mode.is_visual() { 1 } else { 2 });
        self.update_editor(cx, |_, editor, cx| {
            for _ in 0..count {
                if editor
                    .select_previous(&Default::default(), window, cx)
                    .log_err()
                    .is_none()
                {
                    break;
                }
            }
        });
    }

    pub fn select_match(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Vim::take_forced_motion(cx);
        let count = Vim::take_count(cx).unwrap_or(1);
        let Some(pane) = self.pane(window, cx) else {
            return;
        };
        let vim_is_normal = self.mode == Mode::Normal;
        let mut start_selection = MultiBufferOffset(0);
        let mut end_selection = MultiBufferOffset(0);

        self.update_editor(cx, |_, editor, _| {
            editor.set_collapse_matches(false);
        });
        if vim_is_normal {
            pane.update(cx, |pane, cx| {
                if let Some(search_bar) = pane.toolbar().read(cx).item_of_type::<BufferSearchBar>()
                {
                    search_bar.update(cx, |search_bar, cx| {
                        if !search_bar.has_active_match() || !search_bar.show(window, cx) {
                            return;
                        }
                        // without update_match_index there is a bug when the cursor is before the first match
                        search_bar.update_match_index(window, cx);
                        search_bar.select_match(direction.opposite(), 1, window, cx);
                    });
                }
            });
        }
        self.update_editor(cx, |_, editor, cx| {
            let latest = editor
                .selections
                .newest::<MultiBufferOffset>(&editor.display_snapshot(cx));
            start_selection = latest.start;
            end_selection = latest.end;
        });

        let mut match_exists = false;
        pane.update(cx, |pane, cx| {
            if let Some(search_bar) = pane.toolbar().read(cx).item_of_type::<BufferSearchBar>() {
                search_bar.update(cx, |search_bar, cx| {
                    search_bar.update_match_index(window, cx);
                    search_bar.select_match(direction, count, window, cx);
                    match_exists = search_bar.match_exists(window, cx);
                });
            }
        });
        if !match_exists {
            self.update_editor(cx, |_, editor, _| {
                editor.set_collapse_matches(true);
            });
            self.clear_operator(window, cx);
            self.stop_replaying(cx);
            return;
        }
        self.update_editor(cx, |_, editor, cx| {
            let latest = editor
                .selections
                .newest::<MultiBufferOffset>(&editor.display_snapshot(cx));
            if vim_is_normal {
                start_selection = latest.start;
                end_selection = latest.end;
            } else {
                start_selection = start_selection.min(latest.start);
                end_selection = end_selection.max(latest.end);
            }
            if direction == Direction::Prev {
                std::mem::swap(&mut start_selection, &mut end_selection);
            }
            editor.change_selections(Default::default(), window, cx, |s| {
                s.select_ranges([start_selection..end_selection]);
            });
            editor.set_collapse_matches(true);
        });

        match self.maybe_pop_operator() {
            Some(Operator::Change) => self.substitute(None, false, window, cx),
            Some(Operator::Delete) => {
                self.stop_recording(cx);
                self.visual_delete(false, window, cx);
            }
            Some(Operator::Yank) => self.visual_yank(false, window, cx),
            _ => {} // Ignoring other operators
        }
    }
}
