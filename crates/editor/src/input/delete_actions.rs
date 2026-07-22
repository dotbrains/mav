use super::*;

impl Editor {
    pub fn insert(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let autoindent = text.is_empty().not().then(|| AutoindentMode::Block {
            original_indent_columns: Vec::new(),
        });
        self.replace_selections(text, autoindent, window, cx, false);
    }

    /// Collects linked edits for the current selections, pairing each linked
    /// range with `text`.
    pub fn linked_edits_for_selections(&self, text: Arc<str>, cx: &App) -> LinkedEdits {
        let multibuffer_snapshot = self.buffer().read(cx).snapshot(cx);
        let mut linked_edits = LinkedEdits::new();
        if !self.linked_edit_ranges.is_empty() {
            for selection in self.selections.disjoint_anchors() {
                let Some((_, range)) =
                    multibuffer_snapshot.anchor_range_to_buffer_anchor_range(selection.range())
                else {
                    continue;
                };
                linked_edits.push(self, range, text.clone(), cx);
            }
        }
        linked_edits
    }

    /// Deletes the content covered by the current selections and applies
    /// linked edits.
    pub fn delete_selections_with_linked_edits(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_selections("", None, window, cx, true);
    }

    pub fn delete_to_previous_word_start(
        &mut self,
        action: &DeleteToPreviousWordStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        self.transact(window, cx, |this, window, cx| {
            this.select_autoclose_pair(window, cx);
            this.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |map, selection| {
                    if selection.is_empty() {
                        let mut cursor = if action.ignore_newlines {
                            movement::previous_word_start(map, selection.head())
                        } else {
                            movement::previous_word_start_or_newline(map, selection.head())
                        };
                        cursor = movement::adjust_greedy_deletion(
                            map,
                            selection.head(),
                            cursor,
                            action.ignore_brackets,
                        );
                        selection.set_head(cursor, SelectionGoal::None);
                    }
                });
            });
            this.insert("", window, cx);
        });
    }

    pub fn delete_to_previous_subword_start(
        &mut self,
        action: &DeleteToPreviousSubwordStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        self.transact(window, cx, |this, window, cx| {
            this.select_autoclose_pair(window, cx);
            this.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |map, selection| {
                    if selection.is_empty() {
                        let mut cursor = if action.ignore_newlines {
                            movement::previous_subword_start(map, selection.head())
                        } else {
                            movement::previous_subword_start_or_newline(map, selection.head())
                        };
                        cursor = movement::adjust_greedy_deletion(
                            map,
                            selection.head(),
                            cursor,
                            action.ignore_brackets,
                        );
                        selection.set_head(cursor, SelectionGoal::None);
                    }
                });
            });
            this.insert("", window, cx);
        });
    }

    pub fn delete_to_next_word_end(
        &mut self,
        action: &DeleteToNextWordEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        self.transact(window, cx, |this, window, cx| {
            this.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |map, selection| {
                    if selection.is_empty() {
                        let mut cursor = if action.ignore_newlines {
                            movement::next_word_end(map, selection.head())
                        } else {
                            movement::next_word_end_or_newline(map, selection.head())
                        };
                        cursor = movement::adjust_greedy_deletion(
                            map,
                            selection.head(),
                            cursor,
                            action.ignore_brackets,
                        );
                        selection.set_head(cursor, SelectionGoal::None);
                    }
                });
            });
            this.insert("", window, cx);
        });
    }

    pub fn delete_to_next_subword_end(
        &mut self,
        action: &DeleteToNextSubwordEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        self.transact(window, cx, |this, window, cx| {
            this.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |map, selection| {
                    if selection.is_empty() {
                        let mut cursor = if action.ignore_newlines {
                            movement::next_subword_end(map, selection.head())
                        } else {
                            movement::next_subword_end_or_newline(map, selection.head())
                        };
                        cursor = movement::adjust_greedy_deletion(
                            map,
                            selection.head(),
                            cursor,
                            action.ignore_brackets,
                        );
                        selection.set_head(cursor, SelectionGoal::None);
                    }
                });
            });
            this.insert("", window, cx);
        });
    }

    pub fn delete_to_beginning_of_line(
        &mut self,
        action: &DeleteToBeginningOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        self.transact(window, cx, |this, window, cx| {
            this.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |_, selection| {
                    selection.reversed = true;
                });
            });

            this.select_to_beginning_of_line(
                &SelectToBeginningOfLine {
                    stop_at_soft_wraps: false,
                    stop_at_indent: action.stop_at_indent,
                },
                window,
                cx,
            );
            this.backspace(&Backspace, window, cx);
        });
    }

    pub fn delete_to_end_of_line(
        &mut self,
        _: &DeleteToEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        self.transact(window, cx, |this, window, cx| {
            this.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |_, selection| {
                    selection.reversed = false;
                });
            });

            this.select_to_end_of_line(
                &SelectToEndOfLine {
                    stop_at_soft_wraps: false,
                },
                window,
                cx,
            );
            this.delete(&Delete, window, cx);
        });
    }

    pub fn cut_to_end_of_line(
        &mut self,
        action: &CutToEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        self.transact(window, cx, |this, window, cx| {
            this.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |_, selection| {
                    selection.reversed = false;
                });
            });

            this.select_to_end_of_line(
                &SelectToEndOfLine {
                    stop_at_soft_wraps: false,
                },
                window,
                cx,
            );
            if !action.stop_at_newlines {
                this.change_selections(Default::default(), window, cx, |s| {
                    s.move_with(&mut |_, sel| {
                        if sel.is_empty() {
                            sel.end = DisplayPoint::new(sel.end.row() + 1_u32, 0);
                        }
                    });
                });
            }
            let item = this.cut_common(false, window, cx);
            cx.write_to_clipboard(item);
        });
    }
}
