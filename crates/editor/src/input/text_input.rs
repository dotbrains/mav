use super::*;

impl Editor {
    pub fn handle_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text: Arc<str> = text.into();

        if self.read_only(cx) {
            return;
        }

        self.unfold_buffers_with_selections(cx);

        let selections = self.selections.all_adjusted(&self.display_snapshot(cx));
        let mut bracket_inserted = false;
        let mut edits = Vec::new();
        let mut linked_edits = LinkedEdits::new();
        let mut new_selections = Vec::with_capacity(selections.len());
        let mut new_autoclose_regions = Vec::new();
        let snapshot = self.buffer.read(cx).read(cx);
        let mut clear_linked_edit_ranges = false;
        let mut all_selections_read_only = true;
        let mut has_adjacent_edits = false;
        let mut in_adjacent_group = false;

        let mut regions = self
            .selections_with_autoclose_regions(selections, &snapshot)
            .peekable();

        while let Some((selection, autoclose_region)) = regions.next() {
            if snapshot
                .point_to_buffer_point(selection.head())
                .is_none_or(|(snapshot, ..)| !snapshot.capability.editable())
            {
                continue;
            }
            if snapshot
                .point_to_buffer_point(selection.tail())
                .is_none_or(|(snapshot, ..)| !snapshot.capability.editable())
            {
                // note, ideally we'd clip the tail to the closest writeable region towards the head
                continue;
            }
            all_selections_read_only = false;

            if let Some(scope) = snapshot.language_scope_at(selection.head()) {
                // Determine if the inserted text matches the opening or closing
                // bracket of any of this language's bracket pairs.
                let mut bracket_pair = None;
                let mut is_bracket_pair_start = false;
                let mut is_bracket_pair_end = false;
                if !text.is_empty() {
                    let mut bracket_pair_matching_end = None;
                    // `text` can be empty when a user is using IME (e.g. Chinese Wubi Simplified)
                    //  and they are removing the character that triggered IME popup.
                    for (pair, enabled) in scope.brackets() {
                        if !pair.close && !pair.surround {
                            continue;
                        }

                        if enabled && pair.start.ends_with(text.as_ref()) {
                            let prefix_len = pair.start.len() - text.len();
                            let preceding_text_matches_prefix = prefix_len == 0
                                || (selection.start.column >= (prefix_len as u32)
                                    && snapshot.contains_str_at(
                                        Point::new(
                                            selection.start.row,
                                            selection.start.column - (prefix_len as u32),
                                        ),
                                        &pair.start[..prefix_len],
                                    ));
                            if preceding_text_matches_prefix {
                                bracket_pair = Some(pair.clone());
                                is_bracket_pair_start = true;
                                break;
                            }
                        }
                        if pair.end.as_str() == text.as_ref() && bracket_pair_matching_end.is_none()
                        {
                            // take first bracket pair matching end, but don't break in case a later bracket
                            // pair matches start
                            bracket_pair_matching_end = Some(pair.clone());
                        }
                    }
                    if let Some(end) = bracket_pair_matching_end
                        && bracket_pair.is_none()
                    {
                        bracket_pair = Some(end);
                        is_bracket_pair_end = true;
                    }
                }

                if let Some(bracket_pair) = bracket_pair {
                    let snapshot_settings = snapshot.language_settings_at(selection.start, cx);
                    let autoclose = self.use_autoclose && snapshot_settings.use_autoclose;
                    let auto_surround =
                        self.use_auto_surround && snapshot_settings.use_auto_surround;
                    if selection.is_empty() {
                        if is_bracket_pair_start {
                            // If the inserted text is a suffix of an opening bracket and the
                            // selection is preceded by the rest of the opening bracket, then
                            // insert the closing bracket.
                            let following_text_allows_autoclose = snapshot
                                .chars_at(selection.start)
                                .next()
                                .is_none_or(|c| scope.should_autoclose_before(c));

                            let preceding_text_allows_autoclose = selection.start.column == 0
                                || snapshot
                                    .reversed_chars_at(selection.start)
                                    .next()
                                    .is_none_or(|c| {
                                        bracket_pair.start != bracket_pair.end
                                            || !snapshot
                                                .char_classifier_at(selection.start)
                                                .is_word(c)
                                    });

                            let is_closing_quote = if bracket_pair.end == bracket_pair.start
                                && bracket_pair.start.len() == 1
                            {
                                if let Some(target) = bracket_pair.start.chars().next() {
                                    let mut byte_offset = 0u32;
                                    let current_line_count = snapshot
                                        .reversed_chars_at(selection.start)
                                        .take_while(|&c| c != '\n')
                                        .filter(|c| {
                                            byte_offset += c.len_utf8() as u32;
                                            if *c != target {
                                                return false;
                                            }

                                            let point = Point::new(
                                                selection.start.row,
                                                selection.start.column.saturating_sub(byte_offset),
                                            );

                                            let is_enabled = snapshot
                                                .language_scope_at(point)
                                                .and_then(|scope| {
                                                    scope
                                                        .brackets()
                                                        .find(|(pair, _)| {
                                                            pair.start == bracket_pair.start
                                                        })
                                                        .map(|(_, enabled)| enabled)
                                                })
                                                .unwrap_or(true);

                                            let is_delimiter = snapshot
                                                .language_scope_at(Point::new(
                                                    point.row,
                                                    point.column + 1,
                                                ))
                                                .and_then(|scope| {
                                                    scope
                                                        .brackets()
                                                        .find(|(pair, _)| {
                                                            pair.start == bracket_pair.start
                                                        })
                                                        .map(|(_, enabled)| !enabled)
                                                })
                                                .unwrap_or(false);

                                            is_enabled && !is_delimiter
                                        })
                                        .count();
                                    current_line_count % 2 == 1
                                } else {
                                    false
                                }
                            } else {
                                false
                            };

                            if autoclose
                                && bracket_pair.close
                                && following_text_allows_autoclose
                                && preceding_text_allows_autoclose
                                && !is_closing_quote
                            {
                                let anchor = snapshot.anchor_before(selection.end);
                                new_selections.push((selection.map(|_| anchor), text.len()));
                                new_autoclose_regions.push((
                                    anchor,
                                    text.len(),
                                    selection.id,
                                    bracket_pair.clone(),
                                ));
                                edits.push((
                                    selection.range(),
                                    format!("{}{}", text, bracket_pair.end).into(),
                                ));
                                bracket_inserted = true;
                                continue;
                            }
                        }

                        if let Some(region) = autoclose_region {
                            // If the selection is followed by an auto-inserted closing bracket,
                            // then don't insert that closing bracket again; just move the selection
                            // past the closing bracket.
                            let should_skip = selection.end == region.range.end.to_point(&snapshot)
                                && text.as_ref() == region.pair.end.as_str()
                                && snapshot.contains_str_at(region.range.end, text.as_ref());
                            if should_skip {
                                let anchor = snapshot.anchor_after(selection.end);
                                new_selections
                                    .push((selection.map(|_| anchor), region.pair.end.len()));
                                continue;
                            }
                        }

                        let always_treat_brackets_as_autoclosed = snapshot
                            .language_settings_at(selection.start, cx)
                            .always_treat_brackets_as_autoclosed;
                        if always_treat_brackets_as_autoclosed
                            && is_bracket_pair_end
                            && snapshot.contains_str_at(selection.end, text.as_ref())
                        {
                            // Otherwise, when `always_treat_brackets_as_autoclosed` is set to `true
                            // and the inserted text is a closing bracket and the selection is followed
                            // by the closing bracket then move the selection past the closing bracket.
                            let anchor = snapshot.anchor_after(selection.end);
                            new_selections.push((selection.map(|_| anchor), text.len()));
                            continue;
                        }
                    }
                    // If an opening bracket is 1 character long and is typed while
                    // text is selected, then surround that text with the bracket pair.
                    else if auto_surround
                        && bracket_pair.surround
                        && is_bracket_pair_start
                        && bracket_pair.start.chars().count() == 1
                    {
                        edits.push((selection.start..selection.start, text.clone()));
                        edits.push((
                            selection.end..selection.end,
                            bracket_pair.end.as_str().into(),
                        ));
                        bracket_inserted = true;
                        new_selections.push((
                            Selection {
                                id: selection.id,
                                start: snapshot.anchor_after(selection.start),
                                end: snapshot.anchor_before(selection.end),
                                reversed: selection.reversed,
                                goal: selection.goal,
                            },
                            0,
                        ));
                        continue;
                    }
                }
            }

            if self.auto_replace_emoji_shortcode
                && selection.is_empty()
                && text.as_ref().ends_with(':')
                && let Some(possible_emoji_short_code) =
                    Self::find_possible_emoji_shortcode_at_position(&snapshot, selection.start)
                && !possible_emoji_short_code.is_empty()
                && let Some(emoji) = emojis::get_by_shortcode(&possible_emoji_short_code)
            {
                let emoji_shortcode_start = Point::new(
                    selection.start.row,
                    selection.start.column - possible_emoji_short_code.len() as u32 - 1,
                );

                // Remove shortcode from buffer
                edits.push((
                    emoji_shortcode_start..selection.start,
                    "".to_string().into(),
                ));
                new_selections.push((
                    Selection {
                        id: selection.id,
                        start: snapshot.anchor_after(emoji_shortcode_start),
                        end: snapshot.anchor_before(selection.start),
                        reversed: selection.reversed,
                        goal: selection.goal,
                    },
                    0,
                ));

                // Insert emoji
                let selection_start_anchor = snapshot.anchor_after(selection.start);
                new_selections.push((selection.map(|_| selection_start_anchor), 0));
                edits.push((selection.start..selection.end, emoji.to_string().into()));

                continue;
            }

            let next_is_adjacent = regions
                .peek()
                .is_some_and(|(next, _)| selection.end == next.start);

            // If not handling any auto-close operation, then just replace the selected
            // text with the given input and move the selection to the end of the
            // newly inserted text.
            let anchor = if in_adjacent_group || next_is_adjacent {
                // After edits the right bias would shift those anchor to the next visible fragment
                // but we want to resolve to the previous one
                snapshot.anchor_before(selection.end)
            } else {
                snapshot.anchor_after(selection.end)
            };

            if !self.linked_edit_ranges.is_empty() {
                let start_anchor = snapshot.anchor_before(selection.start);
                let classifier = snapshot
                    .char_classifier_at(start_anchor)
                    .scope_context(Some(CharScopeContext::LinkedEdit));

                if let Some((_, anchor_range)) =
                    snapshot.anchor_range_to_buffer_anchor_range(start_anchor..anchor)
                {
                    let is_word_char = text
                        .chars()
                        .next()
                        .is_none_or(|char| classifier.is_word(char));

                    let is_dot = text.as_ref() == ".";
                    let should_apply_linked_edit = is_word_char || is_dot;

                    if should_apply_linked_edit {
                        linked_edits.push(&self, anchor_range, text.clone(), cx);
                    } else {
                        clear_linked_edit_ranges = true;
                    }
                }
            }

            new_selections.push((selection.map(|_| anchor), 0));
            edits.push((selection.start..selection.end, text.clone()));

            has_adjacent_edits |= next_is_adjacent;
            in_adjacent_group = next_is_adjacent;
        }

        if all_selections_read_only {
            return;
        }

        drop(regions);
        drop(snapshot);

        self.transact(window, cx, |this, window, cx| {
            if clear_linked_edit_ranges {
                this.linked_edit_ranges.clear();
            }
            let initial_buffer_versions =
                jsx_tag_auto_close::construct_initial_buffer_versions_map(this, &edits, cx);

            this.buffer.update(cx, |buffer, cx| {
                if has_adjacent_edits {
                    buffer.edit_non_coalesce(edits, this.autoindent_mode.clone(), cx);
                } else {
                    buffer.edit(edits, this.autoindent_mode.clone(), cx);
                }
            });
            linked_edits.apply(cx);
            let new_anchor_selections = new_selections.iter().map(|e| &e.0);
            let new_selection_deltas = new_selections.iter().map(|e| e.1);
            let map = this.display_map.update(cx, |map, cx| map.snapshot(cx));
            let new_selections = resolve_selections_wrapping_blocks::<MultiBufferOffset, _>(
                new_anchor_selections,
                &map,
            )
            .zip(new_selection_deltas)
            .map(|(selection, delta)| Selection {
                id: selection.id,
                start: selection.start + delta,
                end: selection.end + delta,
                reversed: selection.reversed,
                goal: SelectionGoal::None,
            })
            .collect::<Vec<_>>();

            let mut i = 0;
            for (position, delta, selection_id, pair) in new_autoclose_regions {
                let position = position.to_offset(map.buffer_snapshot()) + delta;
                let start = map.buffer_snapshot().anchor_before(position);
                let end = map.buffer_snapshot().anchor_after(position);
                while let Some(existing_state) = this.autoclose_regions.get(i) {
                    match existing_state
                        .range
                        .start
                        .cmp(&start, map.buffer_snapshot())
                    {
                        Ordering::Less => i += 1,
                        Ordering::Greater => break,
                        Ordering::Equal => {
                            match end.cmp(&existing_state.range.end, map.buffer_snapshot()) {
                                Ordering::Less => i += 1,
                                Ordering::Equal => break,
                                Ordering::Greater => break,
                            }
                        }
                    }
                }
                this.autoclose_regions.insert(
                    i,
                    AutocloseRegion {
                        selection_id,
                        range: start..end,
                        pair,
                    },
                );
            }

            let had_active_edit_prediction = this.has_active_edit_prediction();
            this.change_selections(
                SelectionEffects::scroll(Autoscroll::fit()).completions(false),
                window,
                cx,
                |s| s.select(new_selections),
            );

            if !bracket_inserted
                && let Some(on_type_format_task) =
                    this.trigger_on_type_formatting(text.to_string(), window, cx)
            {
                on_type_format_task.detach_and_log_err(cx);
            }

            let editor_settings = EditorSettings::get_global(cx);
            if bracket_inserted
                && (editor_settings.auto_signature_help
                    || editor_settings.show_signature_help_after_edits)
            {
                this.show_signature_help(&ShowSignatureHelp, window, cx);
            }

            let trigger_in_words =
                this.show_edit_predictions_in_menu() || !had_active_edit_prediction;
            if this.hard_wrap.is_some() {
                let latest: Range<Point> = this.selections.newest(&map).range();
                if latest.is_empty()
                    && this
                        .buffer()
                        .read(cx)
                        .snapshot(cx)
                        .line_len(MultiBufferRow(latest.start.row))
                        == latest.start.column
                {
                    this.rewrap(
                        RewrapOptions {
                            override_language_settings: true,
                            preserve_existing_whitespace: true,
                            line_length: None,
                        },
                        cx,
                    )
                }
            }
            this.trigger_completion_on_input(&text, trigger_in_words, window, cx);
            refresh_linked_ranges(this, window, cx);
            this.refresh_edit_prediction(
                true,
                false,
                EditPredictionRequestTrigger::BufferEdit,
                window,
                cx,
            );
            jsx_tag_auto_close::handle_from(this, initial_buffer_versions, window, cx);
        });
    }
}
