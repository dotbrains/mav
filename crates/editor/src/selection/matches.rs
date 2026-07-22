use super::*;

impl Editor {
    pub fn select_all_matches(
        &mut self,
        _action: &SelectAllMatches,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));

        self.select_next_match_internal(&display_map, false, None, window, cx)?;
        let Some(select_next_state) = self.select_next_state.as_mut().filter(|state| !state.done)
        else {
            return Ok(());
        };

        let mut new_selections = Vec::new();
        let initial_selection = self.selections.oldest::<MultiBufferOffset>(&display_map);
        let reversed = initial_selection.reversed;
        let buffer = display_map.buffer_snapshot();
        let query_matches = select_next_state
            .query
            .stream_find_iter(buffer.bytes_in_range(MultiBufferOffset(0)..buffer.len()));

        for query_match in query_matches.into_iter() {
            let query_match = query_match.context("query match for select all action")?; // can only fail due to I/O
            let offset_range = if reversed {
                MultiBufferOffset(query_match.end())..MultiBufferOffset(query_match.start())
            } else {
                MultiBufferOffset(query_match.start())..MultiBufferOffset(query_match.end())
            };

            let is_partial_word_match = select_next_state.wordwise
                && (buffer.is_inside_word(offset_range.start, None)
                    || buffer.is_inside_word(offset_range.end, None));

            let is_initial_selection = MultiBufferOffset(query_match.start())
                == initial_selection.start
                && MultiBufferOffset(query_match.end()) == initial_selection.end;

            if !is_partial_word_match && !is_initial_selection {
                new_selections.push(offset_range);
            }
        }

        // Ensure that the initial range is the last selection, as
        // `MutableSelectionsCollection::select_ranges` makes the last selection
        // the newest selection, which the editor then relies on as the primary
        // cursor for scroll targeting. Without this, the last match would then
        // be automatically focused when the user started editing the selected
        // matches.
        let initial_directed_range = if reversed {
            initial_selection.end..initial_selection.start
        } else {
            initial_selection.start..initial_selection.end
        };
        new_selections.push(initial_directed_range);

        select_next_state.done = true;
        self.unfold_ranges(&new_selections, false, false, cx);
        self.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_ranges(new_selections)
        });

        Ok(())
    }

    pub fn select_next(
        &mut self,
        action: &SelectNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        self.select_next_match_internal(
            &display_map,
            action.replace_newest,
            Some(Autoscroll::newest()),
            window,
            cx,
        )
    }

    pub fn select_previous(
        &mut self,
        action: &SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let buffer = display_map.buffer_snapshot();
        let mut selections = self.selections.all::<MultiBufferOffset>(&display_map);
        if let Some(mut select_prev_state) = self.select_prev_state.take() {
            let query = &select_prev_state.query;
            if !select_prev_state.done {
                let first_selection = selections
                    .iter()
                    .min_by_key(|s| s.id)
                    .context("missing selection for select previous action")?;
                let last_selection = selections
                    .iter()
                    .max_by_key(|s| s.id)
                    .context("missing selection for select previous action")?;
                let mut next_selected_range = None;
                // When we're iterating matches backwards, the oldest match will actually be the furthest one in the buffer.
                let bytes_before_last_selection =
                    buffer.reversed_bytes_in_range(MultiBufferOffset(0)..last_selection.start);
                let bytes_after_first_selection =
                    buffer.reversed_bytes_in_range(first_selection.end..buffer.len());
                let query_matches = query
                    .stream_find_iter(bytes_before_last_selection)
                    .map(|result| (last_selection.start, result))
                    .chain(
                        query
                            .stream_find_iter(bytes_after_first_selection)
                            .map(|result| (buffer.len(), result)),
                    );
                for (end_offset, query_match) in query_matches {
                    let query_match =
                        query_match.context("query match for select previous action")?;
                    let offset_range =
                        end_offset - query_match.end()..end_offset - query_match.start();

                    if !select_prev_state.wordwise
                        || (!buffer.is_inside_word(offset_range.start, None)
                            && !buffer.is_inside_word(offset_range.end, None))
                    {
                        next_selected_range = Some(offset_range);
                        break;
                    }
                }

                if let Some(next_selected_range) = next_selected_range {
                    self.select_match_ranges(
                        next_selected_range,
                        last_selection.reversed,
                        action.replace_newest,
                        Some(Autoscroll::newest()),
                        window,
                        cx,
                    );
                } else {
                    select_prev_state.done = true;
                }
            }

            self.select_prev_state = Some(select_prev_state);
        } else {
            let mut only_carets = true;
            let mut same_text_selected = true;
            let mut selected_text = None;

            let mut selections_iter = selections.iter().peekable();
            while let Some(selection) = selections_iter.next() {
                if selection.start != selection.end {
                    only_carets = false;
                }

                if same_text_selected {
                    if selected_text.is_none() {
                        selected_text =
                            Some(buffer.text_for_range(selection.range()).collect::<String>());
                    }

                    if let Some(next_selection) = selections_iter.peek() {
                        if next_selection.len() == selection.len() {
                            let next_selected_text = buffer
                                .text_for_range(next_selection.range())
                                .collect::<String>();
                            if Some(next_selected_text) != selected_text {
                                same_text_selected = false;
                                selected_text = None;
                            }
                        } else {
                            same_text_selected = false;
                            selected_text = None;
                        }
                    }
                }
            }

            if only_carets {
                for selection in &mut selections {
                    let (word_range, _) = buffer.surrounding_word(selection.start, None);
                    selection.start = word_range.start;
                    selection.end = word_range.end;
                    selection.goal = SelectionGoal::None;
                    selection.reversed = false;
                    self.select_match_ranges(
                        selection.start..selection.end,
                        selection.reversed,
                        action.replace_newest,
                        Some(Autoscroll::newest()),
                        window,
                        cx,
                    );
                }
                if selections.len() == 1 {
                    let selection = selections
                        .last()
                        .expect("ensured that there's only one selection");
                    let query = buffer
                        .text_for_range(selection.start..selection.end)
                        .collect::<String>();
                    let is_empty = query.is_empty();
                    let select_state = SelectNextState {
                        query: self.build_query(&[query.chars().rev().collect::<String>()], cx)?,
                        wordwise: true,
                        done: is_empty,
                    };
                    self.select_prev_state = Some(select_state);
                } else {
                    self.select_prev_state = None;
                }
            } else if let Some(selected_text) = selected_text {
                self.select_prev_state = Some(SelectNextState {
                    query: self
                        .build_query(&[selected_text.chars().rev().collect::<String>()], cx)?,
                    wordwise: false,
                    done: false,
                });
                self.select_previous(action, window, cx)?;
            }
        }
        Ok(())
    }

    pub fn find_next_match(
        &mut self,
        _: &FindNextMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let selections = self.selections.disjoint_anchors_arc();
        match selections.first() {
            Some(first) if selections.len() >= 2 => {
                self.change_selections(Default::default(), window, cx, |s| {
                    s.select_ranges([first.range()]);
                });
            }
            _ => self.select_next(
                &SelectNext {
                    replace_newest: true,
                },
                window,
                cx,
            )?,
        }
        Ok(())
    }

    pub fn find_previous_match(
        &mut self,
        _: &FindPreviousMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let selections = self.selections.disjoint_anchors_arc();
        match selections.last() {
            Some(last) if selections.len() >= 2 => {
                self.change_selections(Default::default(), window, cx, |s| {
                    s.select_ranges([last.range()]);
                });
            }
            _ => self.select_previous(
                &SelectPrevious {
                    replace_newest: true,
                },
                window,
                cx,
            )?,
        }
        Ok(())
    }

    fn select_match_ranges(
        &mut self,
        range: Range<MultiBufferOffset>,
        reversed: bool,
        replace_newest: bool,
        auto_scroll: Option<Autoscroll>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        self.unfold_ranges(
            std::slice::from_ref(&range),
            false,
            auto_scroll.is_some(),
            cx,
        );
        let effects = if let Some(scroll) = auto_scroll {
            SelectionEffects::scroll(scroll)
        } else {
            SelectionEffects::no_scroll()
        };
        self.change_selections(effects, window, cx, |s| {
            if replace_newest {
                s.delete(s.newest_anchor().id);
            }
            if reversed {
                s.insert_range(range.end..range.start);
            } else {
                s.insert_range(range);
            }
        });
    }

    fn select_next_match_internal(
        &mut self,
        display_map: &DisplaySnapshot,
        replace_newest: bool,
        autoscroll: Option<Autoscroll>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let buffer = display_map.buffer_snapshot();
        let mut selections = self.selections.all::<MultiBufferOffset>(&display_map);
        if let Some(mut select_next_state) = self.select_next_state.take() {
            let query = &select_next_state.query;
            if !select_next_state.done {
                let first_selection = selections
                    .iter()
                    .min_by_key(|s| s.id)
                    .context("missing selection for select next action")?;
                let last_selection = selections
                    .iter()
                    .max_by_key(|s| s.id)
                    .context("missing selection for select next action")?;
                let mut next_selected_range = None;

                let bytes_after_last_selection =
                    buffer.bytes_in_range(last_selection.end..buffer.len());
                let bytes_before_first_selection =
                    buffer.bytes_in_range(MultiBufferOffset(0)..first_selection.start);
                let query_matches = query
                    .stream_find_iter(bytes_after_last_selection)
                    .map(|result| (last_selection.end, result))
                    .chain(
                        query
                            .stream_find_iter(bytes_before_first_selection)
                            .map(|result| (MultiBufferOffset(0), result)),
                    );

                for (start_offset, query_match) in query_matches {
                    let query_match = query_match.context("query match for select next action")?;
                    let offset_range =
                        start_offset + query_match.start()..start_offset + query_match.end();

                    if !select_next_state.wordwise
                        || (!buffer.is_inside_word(offset_range.start, None)
                            && !buffer.is_inside_word(offset_range.end, None))
                    {
                        let idx = selections
                            .partition_point(|selection| selection.end <= offset_range.start);
                        let overlaps = selections
                            .get(idx)
                            .map_or(false, |selection| selection.start < offset_range.end);

                        if !overlaps {
                            next_selected_range = Some(offset_range);
                            break;
                        }
                    }
                }

                if let Some(next_selected_range) = next_selected_range {
                    self.select_match_ranges(
                        next_selected_range,
                        last_selection.reversed,
                        replace_newest,
                        autoscroll,
                        window,
                        cx,
                    );
                } else {
                    select_next_state.done = true;
                }
            }

            self.select_next_state = Some(select_next_state);
        } else {
            let mut only_carets = true;
            let mut same_text_selected = true;
            let mut selected_text = None;

            let mut selections_iter = selections.iter().peekable();
            while let Some(selection) = selections_iter.next() {
                if selection.start != selection.end {
                    only_carets = false;
                }

                if same_text_selected {
                    if selected_text.is_none() {
                        selected_text =
                            Some(buffer.text_for_range(selection.range()).collect::<String>());
                    }

                    if let Some(next_selection) = selections_iter.peek() {
                        if next_selection.len() == selection.len() {
                            let next_selected_text = buffer
                                .text_for_range(next_selection.range())
                                .collect::<String>();
                            if Some(next_selected_text) != selected_text {
                                same_text_selected = false;
                                selected_text = None;
                            }
                        } else {
                            same_text_selected = false;
                            selected_text = None;
                        }
                    }
                }
            }

            if only_carets {
                for selection in &mut selections {
                    let (word_range, _) = buffer.surrounding_word(selection.start, None);
                    selection.start = word_range.start;
                    selection.end = word_range.end;
                    selection.goal = SelectionGoal::None;
                    selection.reversed = false;
                    self.select_match_ranges(
                        selection.start..selection.end,
                        selection.reversed,
                        replace_newest,
                        autoscroll,
                        window,
                        cx,
                    );
                }

                if selections.len() == 1 {
                    let selection = selections
                        .last()
                        .expect("ensured that there's only one selection");
                    let query = buffer
                        .text_for_range(selection.start..selection.end)
                        .collect::<String>();
                    let is_empty = query.is_empty();
                    let select_state = SelectNextState {
                        query: self.build_query(&[query], cx)?,
                        wordwise: true,
                        done: is_empty,
                    };
                    self.select_next_state = Some(select_state);
                } else {
                    self.select_next_state = None;
                }
            } else if let Some(selected_text) = selected_text {
                self.select_next_state = Some(SelectNextState {
                    query: self.build_query(&[selected_text], cx)?,
                    wordwise: false,
                    done: false,
                });
                self.select_next_match_internal(
                    display_map,
                    replace_newest,
                    autoscroll,
                    window,
                    cx,
                )?;
            }
        }
        Ok(())
    }

    fn build_query<I, P>(&self, patterns: I, cx: &Context<Self>) -> Result<AhoCorasick, BuildError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<[u8]>,
    {
        let case_sensitive = self
            .select_next_is_case_sensitive
            .unwrap_or_else(|| EditorSettings::get_global(cx).search.case_sensitive);

        let mut builder = AhoCorasickBuilder::new();
        builder.ascii_case_insensitive(!case_sensitive);
        builder.build(patterns)
    }
}
