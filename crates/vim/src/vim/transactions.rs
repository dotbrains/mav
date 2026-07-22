use super::*;

impl Vim {
    pub(super) fn transaction_begun(
        &mut self,
        transaction_id: TransactionId,
        _window: &mut Window,
        _: &mut Context<Self>,
    ) {
        let mode = if (self.mode == Mode::Insert
            || self.mode == Mode::Replace
            || self.mode == Mode::Normal)
            && self.current_tx.is_none()
        {
            self.current_tx = Some(transaction_id);
            self.last_mode
        } else {
            self.mode
        };
        if mode == Mode::VisualLine || mode == Mode::VisualBlock {
            self.undo_modes.insert(transaction_id, mode);
        }
    }

    pub(super) fn transaction_undone(
        &mut self,
        transaction_id: &TransactionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.mode {
            Mode::VisualLine | Mode::VisualBlock | Mode::Visual | Mode::HelixSelect => {
                self.update_editor(cx, |vim, editor, cx| {
                    let original_mode = vim.undo_modes.get(transaction_id);
                    editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                        match original_mode {
                            Some(Mode::VisualLine) => {
                                s.move_with(&mut |map, selection| {
                                    selection.collapse_to(
                                        map.prev_line_boundary(selection.start.to_point(map)).1,
                                        SelectionGoal::None,
                                    )
                                });
                            }
                            Some(Mode::VisualBlock) => {
                                let mut first = s.first_anchor();
                                first.collapse_to(first.start, first.goal);
                                s.select_anchors(vec![first]);
                            }
                            _ => {
                                s.move_with(&mut |map, selection| {
                                    selection.collapse_to(
                                        map.clip_at_line_end(selection.start),
                                        selection.goal,
                                    );
                                });
                            }
                        }
                    });
                });
                self.switch_mode(Mode::Normal, true, window, cx)
            }
            Mode::Normal => {
                self.update_editor(cx, |_, editor, cx| {
                    editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                        s.move_with(&mut |map, selection| {
                            selection
                                .collapse_to(map.clip_at_line_end(selection.end), selection.goal)
                        })
                    })
                });
            }
            Mode::Insert | Mode::Replace | Mode::HelixNormal => {}
        }
    }

    pub(super) fn local_selections_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor() else { return };

        if editor.read(cx).leader_id().is_some() {
            return;
        }

        let newest = editor.read(cx).selections.newest_anchor().clone();
        let is_multicursor = editor.read(cx).selections.count() > 1;
        if self.mode == Mode::Insert && self.current_tx.is_some() {
            if let Some(current_anchor) = &self.current_anchor {
                if current_anchor != &newest
                    && let Some(tx_id) = self.current_tx.take()
                {
                    self.update_editor(cx, |_, editor, cx| {
                        editor.group_until_transaction(tx_id, cx)
                    });
                }
            } else {
                self.current_anchor = Some(newest);
            }
        } else if self.mode == Mode::Normal && newest.start != newest.end {
            if matches!(newest.goal, SelectionGoal::HorizontalRange { .. }) {
                self.switch_mode(Mode::VisualBlock, false, window, cx);
            } else {
                self.switch_mode(Mode::Visual, false, window, cx)
            }
        } else if newest.start == newest.end
            && !is_multicursor
            && [Mode::Visual, Mode::VisualLine, Mode::VisualBlock].contains(&self.mode)
        {
            self.switch_mode(Mode::Normal, false, window, cx);
        }
    }
}
