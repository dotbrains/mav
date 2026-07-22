use super::*;

impl Vim {
    pub(crate) fn search_under_cursor(
        &mut self,
        action: &SearchUnderCursor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to_internal(
            Direction::Next,
            action.case_sensitive,
            !action.partial_word,
            action.regex,
            false,
            window,
            cx,
        )
    }

    pub(crate) fn search_under_cursor_previous(
        &mut self,
        action: &SearchUnderCursorPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to_internal(
            Direction::Prev,
            action.case_sensitive,
            !action.partial_word,
            action.regex,
            false,
            window,
            cx,
        )
    }

    pub(crate) fn move_to_next(
        &mut self,
        action: &MoveToNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to_internal(
            Direction::Next,
            action.case_sensitive,
            !action.partial_word,
            action.regex,
            true,
            window,
            cx,
        )
    }

    pub(crate) fn move_to_previous(
        &mut self,
        action: &MoveToPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to_internal(
            Direction::Prev,
            action.case_sensitive,
            !action.partial_word,
            action.regex,
            true,
            window,
            cx,
        )
    }

    pub(crate) fn move_to_next_match(
        &mut self,
        _: &MoveToNextMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to_match_internal(self.search.direction, window, cx)
    }

    pub(crate) fn move_to_previous_match(
        &mut self,
        _: &MoveToPreviousMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to_match_internal(self.search.direction.opposite(), window, cx)
    }

    pub(crate) fn search(&mut self, action: &Search, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.pane(window, cx) else {
            return;
        };
        let direction = if action.backwards {
            Direction::Prev
        } else {
            Direction::Next
        };
        let count = Vim::take_count(cx).unwrap_or(1);
        Vim::take_forced_motion(cx);
        let prior_selections = self.editor_selections(window, cx);

        let Some(search_bar) = pane
            .read(cx)
            .toolbar()
            .read(cx)
            .item_of_type::<BufferSearchBar>()
        else {
            return;
        };

        let shown = search_bar.update(cx, |search_bar, cx| {
            if !search_bar.show(window, cx) {
                return false;
            }

            search_bar.select_query(window, cx);
            cx.focus_self(window);

            search_bar.set_replacement(None, cx);
            let mut options = SearchOptions::NONE;
            if action.regex && VimSettings::get_global(cx).use_regex_search {
                options |= SearchOptions::REGEX;
            }
            if action.backwards {
                options |= SearchOptions::BACKWARDS;
            }
            if EditorSettings::get_global(cx).search.case_sensitive {
                options |= SearchOptions::CASE_SENSITIVE;
            }
            search_bar.set_search_options(options, cx);
            true
        });

        if !shown {
            return;
        }

        let subscription = cx.subscribe_in(&search_bar, window, |vim, _, event, window, cx| {
            if let buffer_search::Event::Dismissed = event {
                if !vim.search.prior_selections.is_empty() {
                    let prior_selections: Vec<_> = vim.search.prior_selections.drain(..).collect();
                    vim.update_editor(cx, |_, editor, cx| {
                        editor.change_selections(Default::default(), window, cx, |s| {
                            s.select_ranges(prior_selections);
                        });
                    });
                }
            }
        });

        let prior_mode = if self.temp_mode {
            Mode::Insert
        } else {
            self.mode
        };

        self.search = SearchState {
            direction,
            count,
            cmd_f_search: false,
            prior_selections,
            prior_operator: self.operator_stack.last().cloned(),
            prior_mode,
            helix_select: false,
            _dismiss_subscription: Some(subscription),
        }
    }

    // hook into the existing to clear out any vim search state on cmd+f or edit -> find.
    pub(crate) fn search_deploy(
        &mut self,
        _: &buffer_search::Deploy,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Preserve the current mode when resetting search state
        let current_mode = self.mode;
        self.search = Default::default();
        self.search.prior_mode = current_mode;
        self.search.cmd_f_search = true;
        cx.propagate();
    }

    pub fn search_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.store_visual_marks(window, cx);
        let Some(pane) = self.pane(window, cx) else {
            return;
        };
        let new_selections = self.editor_selections(window, cx);
        let result = pane.update(cx, |pane, cx| {
            let search_bar = pane.toolbar().read(cx).item_of_type::<BufferSearchBar>()?;
            if self.search.helix_select {
                search_bar.update(cx, |search_bar, cx| {
                    search_bar.select_all_matches(&Default::default(), window, cx)
                });
                return None;
            }
            search_bar.update(cx, |search_bar, cx| {
                let mut count = self.search.count;
                let direction = self.search.direction;
                search_bar.has_active_match();
                let new_head = new_selections.last()?.start;
                let is_different_head = self
                    .search
                    .prior_selections
                    .last()
                    .is_none_or(|range| range.start != new_head);

                if is_different_head {
                    count = count.saturating_sub(1)
                }
                self.search.count = 1;
                search_bar.select_match(direction, count, window, cx);
                search_bar.focus_editor(&Default::default(), window, cx);

                let prior_selections: Vec<_> = self.search.prior_selections.drain(..).collect();
                let prior_mode = self.search.prior_mode;
                let prior_operator = self.search.prior_operator.take();

                let query = search_bar.query(cx).into();
                Vim::globals(cx).registers.insert('/', query);
                Some((prior_selections, prior_mode, prior_operator))
            })
        });

        let Some((mut prior_selections, prior_mode, prior_operator)) = result else {
            return;
        };

        let new_selections = self.editor_selections(window, cx);

        // If the active editor has changed during a search, don't panic.
        if prior_selections.iter().any(|s| {
            self.update_editor(cx, |_, editor, cx| {
                !s.start
                    .is_valid(&editor.snapshot(window, cx).buffer_snapshot())
            })
            .unwrap_or(true)
        }) {
            prior_selections.clear();
        }

        if prior_mode != self.mode {
            self.switch_mode(prior_mode, true, window, cx);
        }
        if let Some(operator) = prior_operator {
            self.push_operator(operator, window, cx);
        };
        self.search_motion(
            Motion::MavSearchResult {
                prior_selections,
                new_selections,
            },
            window,
            cx,
        );
    }

    pub fn move_to_match_internal(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.pane(window, cx) else {
            return;
        };
        let count = Vim::take_count(cx).unwrap_or(1);
        Vim::take_forced_motion(cx);

        if self.search.cmd_f_search {
            self.search.cmd_f_search = false;
            if self.mode.is_visual() {
                self.switch_mode(Mode::Normal, false, window, cx);
            }
            self.sync_vim_settings(window, cx);
        }

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

        let new_selections = self.editor_selections(window, cx);
        self.search_motion(
            Motion::MavSearchResult {
                prior_selections,
                new_selections,
            },
            window,
            cx,
        );
    }

    pub fn move_to_internal(
        &mut self,
        direction: Direction,
        case_sensitive: bool,
        whole_word: bool,
        regex: bool,
        move_cursor: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.pane(window, cx) else {
            return;
        };
        let count = Vim::take_count(cx).unwrap_or(1);
        Vim::take_forced_motion(cx);

        if self.search.cmd_f_search {
            self.search.cmd_f_search = false;
            if self.mode.is_visual() {
                self.switch_mode(Mode::Normal, false, window, cx);
            }
            self.sync_vim_settings(window, cx);
        }

        let prior_selections = self.editor_selections(window, cx);
        let vim = cx.entity();

        let searched = pane.update(cx, |pane, cx| {
            self.search.direction = direction;
            let Some(search_bar) = pane.toolbar().read(cx).item_of_type::<BufferSearchBar>() else {
                return false;
            };
            let search = search_bar.update(cx, |search_bar, cx| {
                let mut options = SearchOptions::NONE;
                if case_sensitive {
                    options |= SearchOptions::CASE_SENSITIVE;
                }
                if regex {
                    options |= SearchOptions::REGEX;
                }
                if whole_word {
                    options |= SearchOptions::WHOLE_WORD;
                }
                if !search_bar.show(window, cx) {
                    return None;
                }
                let Some(query) = search_bar.query_suggestion(
                    Some(settings::SeedQuerySetting::Always),
                    window,
                    cx,
                ) else {
                    drop(search_bar.search("", None, false, window, cx));
                    return None;
                };

                let query = regex::escape(&query);
                Some(search_bar.search(&query, Some(options), true, window, cx))
            });

            let Some(search) = search else { return false };

            if move_cursor {
                let search_bar = search_bar.downgrade();
                cx.spawn_in(window, async move |_, cx| {
                    search.await?;
                    search_bar.update_in(cx, |search_bar, window, cx| {
                        search_bar.select_match(direction, count, window, cx);

                        vim.update(cx, |vim, cx| {
                            let new_selections = vim.editor_selections(window, cx);
                            vim.search_motion(
                                Motion::MavSearchResult {
                                    prior_selections,
                                    new_selections,
                                },
                                window,
                                cx,
                            )
                        });
                    })?;
                    anyhow::Ok(())
                })
                .detach_and_log_err(cx);
            }
            true
        });
        if !searched {
            self.clear_operator(window, cx)
        }

        if self.mode.is_visual() {
            self.switch_mode(Mode::Normal, false, window, cx)
        }
    }
}
