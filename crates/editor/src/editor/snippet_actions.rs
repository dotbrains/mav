use super::*;
use snippet::Snippet;

impl Editor {
    fn show_snippet_choices(
        &mut self,
        choices: &Vec<String>,
        selection: Range<Anchor>,
        cx: &mut Context<Self>,
    ) {
        let buffer_snapshot = self.buffer.read(cx).snapshot(cx);
        let Some((buffer_snapshot, range)) =
            buffer_snapshot.anchor_range_to_buffer_anchor_range(selection.clone())
        else {
            return;
        };
        let Some(buffer) = self.buffer.read(cx).buffer(buffer_snapshot.remote_id()) else {
            return;
        };

        let id = post_inc(&mut self.next_completion_id);
        let snippet_sort_order = EditorSettings::get_global(cx).snippet_sort_order;
        let mut context_menu = self.context_menu.borrow_mut();
        let old_menu = context_menu.take();
        *context_menu = Some(CodeContextMenu::Completions(
            CompletionsMenu::new_snippet_choices(
                id,
                true,
                choices,
                selection.start,
                range,
                buffer,
                old_menu.map(|menu| menu.primary_scroll_handle()),
                snippet_sort_order,
            ),
        ));
    }

    pub fn insert_snippet(
        &mut self,
        insertion_ranges: &[Range<MultiBufferOffset>],
        snippet: Snippet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        struct Tabstop<T> {
            is_end_tabstop: bool,
            ranges: Vec<Range<T>>,
            choices: Option<Vec<String>>,
        }

        let tabstops = self.buffer.update(cx, |buffer, cx| {
            let snippet_text: Arc<str> = snippet.text.clone().into();
            let edits = insertion_ranges
                .iter()
                .cloned()
                .map(|range| (range, snippet_text.clone()));
            let autoindent_mode = AutoindentMode::Block {
                original_indent_columns: Vec::new(),
            };
            buffer.edit(edits, Some(autoindent_mode), cx);

            let snapshot = &*buffer.read(cx);
            let snippet = &snippet;
            snippet
                .tabstops
                .iter()
                .map(|tabstop| {
                    let is_end_tabstop = tabstop.ranges.first().is_some_and(|tabstop| {
                        tabstop.is_empty() && tabstop.start == snippet.text.len() as isize
                    });
                    let mut tabstop_ranges = tabstop
                        .ranges
                        .iter()
                        .flat_map(|tabstop_range| {
                            let mut delta = 0_isize;
                            insertion_ranges.iter().map(move |insertion_range| {
                                let insertion_start = insertion_range.start + delta;
                                delta += snippet.text.len() as isize
                                    - (insertion_range.end - insertion_range.start) as isize;

                                let start =
                                    (insertion_start + tabstop_range.start).min(snapshot.len());
                                let end = (insertion_start + tabstop_range.end).min(snapshot.len());
                                snapshot.anchor_before(start)..snapshot.anchor_after(end)
                            })
                        })
                        .collect::<Vec<_>>();
                    tabstop_ranges.sort_unstable_by(|a, b| a.start.cmp(&b.start, snapshot));

                    Tabstop {
                        is_end_tabstop,
                        ranges: tabstop_ranges,
                        choices: tabstop.choices.clone(),
                    }
                })
                .collect::<Vec<_>>()
        });
        if let Some(tabstop) = tabstops.first() {
            self.change_selections(Default::default(), window, cx, |s| {
                // Reverse order so that the first range is the newest created selection.
                // Completions will use it and autoscroll will prioritize it.
                s.select_ranges(tabstop.ranges.iter().rev().cloned());
            });

            if let Some(choices) = &tabstop.choices
                && let Some(selection) = tabstop.ranges.first()
            {
                self.show_snippet_choices(choices, selection.clone(), cx)
            }

            if !tabstop.is_end_tabstop {
                let choices = tabstops
                    .iter()
                    .map(|tabstop| tabstop.choices.clone())
                    .collect();

                let ranges = tabstops
                    .into_iter()
                    .map(|tabstop| tabstop.ranges)
                    .collect::<Vec<_>>();

                self.snippet_stack.push(SnippetState {
                    active_index: 0,
                    ranges,
                    choices,
                });
            }

            if self.autoclose_regions.is_empty() {
                let snapshot = self.buffer.read(cx).snapshot(cx);
                for selection in &mut self.selections.all::<Point>(&self.display_snapshot(cx)) {
                    let selection_head = selection.head();
                    let Some(scope) = snapshot.language_scope_at(selection_head) else {
                        continue;
                    };

                    let mut bracket_pair = None;
                    let max_lookup_length = scope
                        .brackets()
                        .map(|(pair, _)| {
                            pair.start
                                .as_str()
                                .chars()
                                .count()
                                .max(pair.end.as_str().chars().count())
                        })
                        .max();
                    if let Some(max_lookup_length) = max_lookup_length {
                        let next_text = snapshot
                            .chars_at(selection_head)
                            .take(max_lookup_length)
                            .collect::<String>();
                        let prev_text = snapshot
                            .reversed_chars_at(selection_head)
                            .take(max_lookup_length)
                            .collect::<String>();

                        for (pair, enabled) in scope.brackets() {
                            if enabled
                                && pair.close
                                && prev_text.starts_with(pair.start.as_str())
                                && next_text.starts_with(pair.end.as_str())
                            {
                                bracket_pair = Some(pair.clone());
                                break;
                            }
                        }
                    }

                    if let Some(pair) = bracket_pair {
                        let snapshot_settings = snapshot.language_settings_at(selection_head, cx);
                        let autoclose_enabled =
                            self.use_autoclose && snapshot_settings.use_autoclose;
                        if autoclose_enabled {
                            let start = snapshot.anchor_after(selection_head);
                            let end = snapshot.anchor_after(selection_head);
                            self.autoclose_regions.push(AutocloseRegion {
                                selection_id: selection.id,
                                range: start..end,
                                pair,
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn move_to_next_snippet_tabstop(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.move_to_snippet_tabstop(Bias::Right, window, cx)
    }

    pub fn move_to_prev_snippet_tabstop(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.move_to_snippet_tabstop(Bias::Left, window, cx)
    }

    pub fn next_snippet_tabstop(
        &mut self,
        _: &NextSnippetTabstop,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode.is_single_line() || self.snippet_stack.is_empty() {
            cx.propagate();
            return;
        }

        if self.move_to_next_snippet_tabstop(window, cx) {
            return;
        }
        cx.propagate();
    }

    pub fn previous_snippet_tabstop(
        &mut self,
        _: &PreviousSnippetTabstop,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode.is_single_line() || self.snippet_stack.is_empty() {
            cx.propagate();
            return;
        }

        if self.move_to_prev_snippet_tabstop(window, cx) {
            return;
        }
        cx.propagate();
    }

    pub fn move_to_snippet_tabstop(
        &mut self,
        bias: Bias,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(mut snippet) = self.snippet_stack.pop() {
            match bias {
                Bias::Left => {
                    if snippet.active_index > 0 {
                        snippet.active_index -= 1;
                    } else {
                        self.snippet_stack.push(snippet);
                        return false;
                    }
                }
                Bias::Right => {
                    if snippet.active_index + 1 < snippet.ranges.len() {
                        snippet.active_index += 1;
                    } else {
                        self.snippet_stack.push(snippet);
                        return false;
                    }
                }
            }
            if let Some(current_ranges) = snippet.ranges.get(snippet.active_index) {
                self.change_selections(Default::default(), window, cx, |s| {
                    // Reverse order so that the first range is the newest created selection.
                    // Completions will use it and autoscroll will prioritize it.
                    s.select_ranges(current_ranges.iter().rev().cloned())
                });

                if let Some(choices) = &snippet.choices[snippet.active_index]
                    && let Some(selection) = current_ranges.first()
                {
                    self.show_snippet_choices(choices, selection.clone(), cx);
                }

                if snippet.active_index + 1 < snippet.ranges.len() {
                    self.snippet_stack.push(snippet);
                }
                return true;
            }
        }

        false
    }
}
