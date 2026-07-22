use super::*;

impl Vim {
    pub(super) fn helix_insert(
        &mut self,
        _: &HelixInsert,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_recording(cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |_map, selection| {
                    // In helix normal mode, move cursor to start of selection and collapse
                    if !selection.is_empty() {
                        selection.collapse_to(selection.start, SelectionGoal::None);
                    }
                });
            });
        });
        self.switch_mode(Mode::Insert, false, window, cx);
    }

    pub(super) fn helix_select_regex(
        &mut self,
        _: &HelixSelectRegex,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Vim::take_forced_motion(cx);
        let Some(pane) = self.pane(window, cx) else {
            return;
        };
        let prior_selections = self.editor_selections(window, cx);
        pane.update(cx, |pane, cx| {
            if let Some(search_bar) = pane.toolbar().read(cx).item_of_type::<BufferSearchBar>() {
                search_bar.update(cx, |search_bar, cx| {
                    if !search_bar.show(window, cx) {
                        return;
                    }

                    search_bar.select_query(window, cx);
                    cx.focus_self(window);

                    search_bar.set_replacement(None, cx);
                    let mut options = SearchOptions::NONE;
                    options |= SearchOptions::REGEX;
                    if EditorSettings::get_global(cx).search.case_sensitive {
                        options |= SearchOptions::CASE_SENSITIVE;
                    }
                    search_bar.set_search_options(options, cx);
                    if let Some(search) = search_bar.set_search_within_selection(
                        Some(FilteredSearchRange::Selection),
                        window,
                        cx,
                    ) {
                        cx.spawn_in(window, async move |search_bar, cx| {
                            if search.await.is_ok() {
                                search_bar.update_in(cx, |search_bar, window, cx| {
                                    search_bar.activate_current_match(window, cx)
                                })
                            } else {
                                Ok(())
                            }
                        })
                        .detach_and_log_err(cx);
                    }
                    self.search = SearchState {
                        direction: searchable::Direction::Next,
                        count: 1,
                        cmd_f_search: false,
                        prior_selections,
                        prior_operator: self.operator_stack.last().cloned(),
                        prior_mode: self.mode,
                        helix_select: true,
                        _dismiss_subscription: None,
                    }
                });
            }
        });
        self.start_recording(cx);
    }

    pub(super) fn helix_append(
        &mut self,
        _: &HelixAppend,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_recording(cx);
        self.switch_mode(Mode::Insert, false, window, cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |map, selection| {
                    let point = if selection.is_empty() {
                        right(map, selection.head(), 1)
                    } else {
                        selection.end
                    };
                    selection.collapse_to(point, SelectionGoal::None);
                });
            });
        });
    }

    /// Helix-specific implementation of `shift-a` that accounts for Helix's
    /// selection model, where selecting a line with `x` creates a selection
    /// from column 0 of the current row to column 0 of the next row, so the
    /// default [`vim::normal::InsertEndOfLine`] would move the cursor to the
    /// end of the wrong line.
    pub(super) fn helix_insert_end_of_line(
        &mut self,
        _: &HelixInsertEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_recording(cx);
        self.switch_mode(Mode::Insert, false, window, cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |map, selection| {
                    let cursor = if !selection.is_empty() && !selection.reversed {
                        movement::left(map, selection.head())
                    } else {
                        selection.head()
                    };
                    selection
                        .collapse_to(motion::next_line_end(map, cursor, 1), SelectionGoal::None);
                });
            });
        });
    }

    pub fn helix_replace(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.update_editor(cx, |_, editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                let display_map = editor.display_snapshot(cx);
                let selections = editor.selections.all_display(&display_map);

                let mut edits = Vec::new();
                let mut selection_info = Vec::new();
                for selection in &selections {
                    let mut range = selection.range();
                    let was_empty = range.is_empty();
                    let was_reversed = selection.reversed;

                    if was_empty {
                        range.end = movement::saturating_right(&display_map, range.start);
                    }

                    let byte_range = range.start.to_offset(&display_map, Bias::Left)
                        ..range.end.to_offset(&display_map, Bias::Left);

                    let snapshot = display_map.buffer_snapshot();
                    let grapheme_count = snapshot.grapheme_count_for_range(&byte_range);
                    let anchor = snapshot.anchor_before(byte_range.start);
                    let mut replacement_len = 0;

                    if !byte_range.is_empty() {
                        let mut replacement_text = text.repeat(grapheme_count);
                        LineEnding::normalize(&mut replacement_text);
                        replacement_len = replacement_text.len();
                        edits.push((byte_range, replacement_text));
                    }

                    selection_info.push((anchor, replacement_len, was_empty, was_reversed));
                }

                editor.edit(edits, cx);

                // Restore selections based on original info
                let snapshot = editor.buffer().read(cx).snapshot(cx);
                let ranges: Vec<_> = selection_info
                    .into_iter()
                    .map(|(start_anchor, replacement_len, was_empty, was_reversed)| {
                        let start_point = start_anchor.to_point(&snapshot);
                        if was_empty {
                            start_point..start_point
                        } else {
                            let end_offset = start_anchor.to_offset(&snapshot) + replacement_len;
                            let end_point = snapshot.offset_to_point(end_offset);
                            if was_reversed {
                                end_point..start_point
                            } else {
                                start_point..end_point
                            }
                        }
                    })
                    .collect();

                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_ranges(ranges);
                });
            });
        });
        self.switch_mode(Mode::HelixNormal, true, window, cx);
    }

    pub fn helix_goto_last_modification(
        &mut self,
        _: &HelixGotoLastModification,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump(".".into(), false, false, window, cx);
    }

    pub fn helix_select_lines(
        &mut self,
        _: &HelixSelectLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = Vim::take_count(cx).unwrap_or(1);
        self.update_editor(cx, |_, editor, cx| {
            let display_map = editor.display_map.update(cx, |map, cx| map.snapshot(cx));
            let mut selections = editor.selections.all::<Point>(&display_map);
            let max_point = display_map.buffer_snapshot().max_point();
            let buffer_snapshot = &display_map.buffer_snapshot();

            for selection in &mut selections {
                // Start always goes to column 0 of the first selected line
                let start_row = selection.start.row;
                let current_end_row = selection.end.row;

                // Check if cursor is on empty line by checking first character
                let line_start_offset = buffer_snapshot.point_to_offset(Point::new(start_row, 0));
                let first_char = buffer_snapshot.chars_at(line_start_offset).next();
                let extra_line = if first_char == Some('\n') && selection.is_empty() {
                    1
                } else {
                    0
                };

                let end_row = current_end_row + count as u32 + extra_line;

                selection.start = Point::new(start_row, 0);
                selection.end = if end_row > max_point.row {
                    max_point
                } else {
                    Point::new(end_row, 0)
                };
                selection.reversed = false;
            }

            editor.change_selections(Default::default(), window, cx, |s| {
                s.select(selections);
            });
        });
    }

    pub(super) fn helix_keep_newest_selection(
        &mut self,
        _: &HelixKeepNewestSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_editor(cx, |_, editor, cx| {
            let newest = editor
                .selections
                .newest::<MultiBufferOffset>(&editor.display_snapshot(cx));
            editor.change_selections(Default::default(), window, cx, |s| s.select(vec![newest]));
        });
    }

    pub(super) fn do_helix_substitute(
        &mut self,
        yank: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_editor(cx, |vim, editor, cx| {
            editor.set_clip_at_line_ends(false, cx);
            editor.transact(window, cx, |editor, window, cx| {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.move_with(&mut |map, selection| {
                        if selection.start == selection.end {
                            selection.end = movement::right(map, selection.end);
                        }

                        // If the selection starts and ends on a newline, we exclude the last one.
                        if !selection.is_empty()
                            && selection.start.column() == 0
                            && selection.end.column() == 0
                        {
                            selection.end = movement::left(map, selection.end);
                        }
                    })
                });
                if yank {
                    vim.copy_selections_content(editor, MotionKind::Exclusive, window, cx);
                }
                let selections = editor
                    .selections
                    .all::<Point>(&editor.display_snapshot(cx))
                    .into_iter();
                let edits = selections.map(|selection| (selection.start..selection.end, ""));
                editor.edit(edits, cx);
            });
        });
        self.switch_mode(Mode::Insert, true, window, cx);
    }

    pub(super) fn helix_substitute(
        &mut self,
        _: &HelixSubstitute,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_helix_substitute(true, window, cx);
    }

    pub(super) fn helix_substitute_no_yank(
        &mut self,
        _: &HelixSubstituteNoYank,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_helix_substitute(false, window, cx);
    }

    pub(super) fn helix_select_next(
        &mut self,
        _: &HelixSelectNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_helix_select(Direction::Next, window, cx);
    }

    pub(super) fn helix_select_previous(
        &mut self,
        _: &HelixSelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_helix_select(Direction::Prev, window, cx);
    }

    pub(super) fn do_helix_select(
        &mut self,
        direction: searchable::Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.pane(window, cx) else {
            return;
        };
        let count = Vim::take_count(cx).unwrap_or(1);
        Vim::take_forced_motion(cx);
        let prior_selections = self.editor_selections(window, cx);

        let success = pane.update(cx, |pane, cx| {
            let Some(search_bar) = pane.toolbar().read(cx).item_of_type::<BufferSearchBar>() else {
                return false;
            };
            search_bar.update(cx, |search_bar, cx| {
                if !search_bar.has_active_match() || !search_bar.show(window, cx) {
                    return false;
                }
                search_bar.select_match(direction, count, window, cx);
                true
            })
        });

        if !success {
            return;
        }
        if self.mode == Mode::HelixSelect {
            self.update_editor(cx, |_vim, editor, cx| {
                let snapshot = editor.snapshot(window, cx);
                editor.change_selections(SelectionEffects::default(), window, cx, |s| {
                    let buffer = snapshot.buffer_snapshot();

                    s.select_ranges(
                        prior_selections
                            .iter()
                            .cloned()
                            .chain(s.all_anchors(&snapshot).iter().map(|s| s.range()))
                            .map(|range| {
                                let start = range.start.to_offset(buffer);
                                let end = range.end.to_offset(buffer);
                                start..end
                            }),
                    );
                })
            });
        }
    }

    pub fn helix_jump_to_word(
        &mut self,
        _: &HelixJumpToWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let behaviour = match self.mode {
            // Vim normal mode treats jump-to-word as a cursor motion, while Helix
            // normal mode treats the cursor as a single-character selection.
            Mode::Normal => HelixJumpBehaviour::MoveToWordStart,
            // Vim visual mode extends like a motion, so the cursor stops at the
            // same word boundary as normal mode instead of selecting the word.
            Mode::Visual | Mode::VisualLine | Mode::VisualBlock => {
                HelixJumpBehaviour::ExtendToWordStart
            }
            Mode::HelixSelect => HelixJumpBehaviour::Extend,
            _ => HelixJumpBehaviour::Move,
        };
        self.start_helix_jump(behaviour, window, cx);
    }
}
