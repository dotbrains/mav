use super::*;

impl Editor {
    pub fn unwrap_syntax_node(
        &mut self,
        _: &UnwrapSyntaxNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }

        let buffer = self.buffer.read(cx).snapshot(cx);
        let selections = self
            .selections
            .all::<MultiBufferOffset>(&self.display_snapshot(cx))
            .into_iter()
            // subtracting the offset requires sorting
            .sorted_by_key(|i| i.start);

        let full_edits = selections
            .into_iter()
            .filter_map(|selection| {
                let child = if selection.is_empty()
                    && let Some((_, ancestor_range)) =
                        buffer.syntax_ancestor(selection.start..selection.end)
                {
                    ancestor_range
                } else {
                    selection.range()
                };

                let mut parent = child.clone();
                while let Some((_, ancestor_range)) = buffer.syntax_ancestor(parent.clone()) {
                    parent = ancestor_range;
                    if parent.start < child.start || parent.end > child.end {
                        break;
                    }
                }

                if parent == child {
                    return None;
                }
                let text = buffer.text_for_range(child).collect::<String>();
                Some((selection.id, parent, text))
            })
            .collect::<Vec<_>>();
        if full_edits.is_empty() {
            return;
        }

        self.transact(window, cx, |this, window, cx| {
            this.buffer.update(cx, |buffer, cx| {
                buffer.edit(
                    full_edits
                        .iter()
                        .map(|(_, p, t)| (p.clone(), t.clone()))
                        .collect::<Vec<_>>(),
                    None,
                    cx,
                );
            });
            this.change_selections(Default::default(), window, cx, |s| {
                let mut offset = 0;
                let mut selections = vec![];
                for (id, parent, text) in full_edits {
                    let start = parent.start - offset;
                    offset += (parent.end - parent.start) - text.len();
                    selections.push(Selection {
                        id,
                        start,
                        end: start + text.len(),
                        reversed: false,
                        goal: Default::default(),
                    });
                }
                s.select(selections);
            });
        });
    }

    pub(super) fn observe_pending_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut pending: String = window
            .pending_input_keystrokes()
            .into_iter()
            .flatten()
            .filter_map(|keystroke| keystroke.key_char.clone())
            .collect();

        if !self.input_enabled || self.read_only || !self.focus_handle.is_focused(window) {
            pending = "".to_string();
        }

        let existing_pending = self
            .text_highlights(HighlightKey::PendingInput, cx)
            .map(|(_, ranges)| ranges.to_vec());
        if existing_pending.is_none() && pending.is_empty() {
            return;
        }
        let transaction =
            self.transact(window, cx, |this, window, cx| {
                let selections = this
                    .selections
                    .all::<MultiBufferOffset>(&this.display_snapshot(cx));
                let edits = selections
                    .iter()
                    .map(|selection| (selection.end..selection.end, pending.clone()));
                this.edit(edits, cx);
                this.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_ranges(selections.into_iter().enumerate().map(|(ix, sel)| {
                        sel.start + ix * pending.len()..sel.end + ix * pending.len()
                    }));
                });
                if let Some(existing_ranges) = existing_pending {
                    let edits = existing_ranges.iter().map(|range| (range.clone(), ""));
                    this.edit(edits, cx);
                }
            });

        let snapshot = self.snapshot(window, cx);
        let ranges = self
            .selections
            .all::<MultiBufferOffset>(&snapshot.display_snapshot)
            .into_iter()
            .map(|selection| {
                snapshot.buffer_snapshot().anchor_after(selection.end)
                    ..snapshot
                        .buffer_snapshot()
                        .anchor_before(selection.end + pending.len())
            })
            .collect();

        if pending.is_empty() {
            self.clear_highlights(HighlightKey::PendingInput, cx);
        } else {
            self.highlight_text(
                HighlightKey::PendingInput,
                ranges,
                HighlightStyle {
                    underline: Some(UnderlineStyle {
                        thickness: px(1.),
                        color: None,
                        wavy: false,
                    }),
                    ..Default::default()
                },
                cx,
            );
        }

        self.ime_transaction = self.ime_transaction.or(transaction);
        if let Some(transaction) = self.ime_transaction {
            self.buffer.update(cx, |buffer, cx| {
                buffer.group_until_transaction(transaction, cx);
            });
        }

        if self
            .text_highlights(HighlightKey::PendingInput, cx)
            .is_none()
        {
            self.ime_transaction.take();
        }
    }

    pub(super) fn linked_editing_ranges_for(
        &self,
        query_range: Range<text::Anchor>,
        cx: &App,
    ) -> Option<HashMap<Entity<Buffer>, Vec<Range<text::Anchor>>>> {
        use text::ToOffset as TO;

        if self.linked_edit_ranges.is_empty() {
            return None;
        }
        if query_range.start.buffer_id != query_range.end.buffer_id {
            return None;
        };
        let multibuffer_snapshot = self.buffer.read(cx).snapshot(cx);
        let buffer = self.buffer.read(cx).buffer(query_range.end.buffer_id)?;
        let buffer_snapshot = buffer.read(cx).snapshot();
        let (base_range, linked_ranges) = self.linked_edit_ranges.get(
            buffer_snapshot.remote_id(),
            query_range.clone(),
            &buffer_snapshot,
        )?;
        // find offset from the start of current range to current cursor position
        let start_byte_offset = TO::to_offset(&base_range.start, &buffer_snapshot);

        let start_offset = TO::to_offset(&query_range.start, &buffer_snapshot);
        let start_difference = start_offset - start_byte_offset;
        let end_offset = TO::to_offset(&query_range.end, &buffer_snapshot);
        let end_difference = end_offset - start_byte_offset;

        // Current range has associated linked ranges.
        let mut linked_edits = HashMap::<_, Vec<_>>::default();
        for range in linked_ranges.iter() {
            let start_offset = TO::to_offset(&range.start, &buffer_snapshot);
            let end_offset = start_offset + end_difference;
            let start_offset = start_offset + start_difference;
            if start_offset > buffer_snapshot.len() || end_offset > buffer_snapshot.len() {
                continue;
            }
            if self.selections.disjoint_anchor_ranges().any(|s| {
                let Some((selection_start, _)) =
                    multibuffer_snapshot.anchor_to_buffer_anchor(s.start)
                else {
                    return false;
                };
                let Some((selection_end, _)) = multibuffer_snapshot.anchor_to_buffer_anchor(s.end)
                else {
                    return false;
                };
                if selection_start.buffer_id != query_range.start.buffer_id
                    || selection_end.buffer_id != query_range.end.buffer_id
                {
                    return false;
                }
                TO::to_offset(&selection_start, &buffer_snapshot) <= end_offset
                    && TO::to_offset(&selection_end, &buffer_snapshot) >= start_offset
            }) {
                continue;
            }
            let start = buffer_snapshot.anchor_after(start_offset);
            let end = buffer_snapshot.anchor_after(end_offset);
            linked_edits
                .entry(buffer.clone())
                .or_default()
                .push(start..end);
        }
        Some(linked_edits)
    }

    pub(super) fn marked_text_ranges(
        &self,
        cx: &App,
    ) -> Option<Vec<Range<MultiBufferOffsetUtf16>>> {
        let snapshot = self.buffer.read(cx).read(cx);
        let (_, ranges) = self.text_highlights(HighlightKey::InputComposition, cx)?;
        Some(
            ranges
                .iter()
                .map(move |range| {
                    range.start.to_offset_utf16(&snapshot)..range.end.to_offset_utf16(&snapshot)
                })
                .collect(),
        )
    }

    /// Replaces the editor's selections with the provided `text`, applying the
    /// given `autoindent_mode` (`None` will skip autoindentation).
    ///
    /// Early returns if the editor is in read-only mode, without applying any
    /// edits.
    pub(super) fn replace_selections(
        &mut self,
        text: &str,
        autoindent_mode: Option<AutoindentMode>,
        window: &mut Window,
        cx: &mut Context<Self>,
        apply_linked_edits: bool,
    ) {
        if self.read_only(cx) {
            return;
        }

        let text: Arc<str> = text.into();
        self.transact(window, cx, |this, window, cx| {
            let old_selections = this.selections.all_adjusted(&this.display_snapshot(cx));
            let linked_edits = if apply_linked_edits {
                this.linked_edits_for_selections(text.clone(), cx)
            } else {
                LinkedEdits::new()
            };

            let selection_anchors = this.buffer.update(cx, |buffer, cx| {
                let anchors = {
                    let snapshot = buffer.read(cx);
                    old_selections
                        .iter()
                        .map(|s| {
                            let anchor = snapshot.anchor_after(s.head());
                            s.map(|_| anchor)
                        })
                        .collect::<Vec<_>>()
                };
                buffer.edit(
                    old_selections
                        .iter()
                        .map(|s| (s.start..s.end, text.clone())),
                    autoindent_mode,
                    cx,
                );
                anchors
            });

            linked_edits.apply(cx);

            this.change_selections(Default::default(), window, cx, |s| {
                s.select_anchors(selection_anchors);
            });

            if apply_linked_edits {
                refresh_linked_ranges(this, window, cx);
            }

            cx.notify();
        });
    }

    /// If any empty selections is touching the start of its innermost containing autoclose
    /// region, expand it to select the brackets.
    pub(super) fn select_autoclose_pair(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selections = self
            .selections
            .all::<MultiBufferOffset>(&self.display_snapshot(cx));
        let buffer = self.buffer.read(cx).read(cx);
        let new_selections = self
            .selections_with_autoclose_regions(selections, &buffer)
            .map(|(mut selection, region)| {
                if !selection.is_empty() {
                    return selection;
                }

                if let Some(region) = region {
                    let mut range = region.range.to_offset(&buffer);
                    if selection.start == range.start && range.start.0 >= region.pair.start.len() {
                        range.start -= region.pair.start.len();
                        if buffer.contains_str_at(range.start, &region.pair.start)
                            && buffer.contains_str_at(range.end, &region.pair.end)
                        {
                            range.end += region.pair.end.len();
                            selection.start = range.start;
                            selection.end = range.end;

                            return selection;
                        }
                    }
                }

                let always_treat_brackets_as_autoclosed = buffer
                    .language_settings_at(selection.start, cx)
                    .always_treat_brackets_as_autoclosed;

                if !always_treat_brackets_as_autoclosed {
                    return selection;
                }

                if let Some(scope) = buffer.language_scope_at(selection.start) {
                    for (pair, enabled) in scope.brackets() {
                        if !enabled || !pair.close {
                            continue;
                        }

                        if buffer.contains_str_at(selection.start, &pair.end) {
                            let pair_start_len = pair.start.len();
                            if buffer.contains_str_at(
                                selection.start.saturating_sub_usize(pair_start_len),
                                &pair.start,
                            ) {
                                selection.start -= pair_start_len;
                                selection.end += pair.end.len();

                                return selection;
                            }
                        }
                    }
                }

                selection
            })
            .collect();

        drop(buffer);
        self.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select(new_selections)
        });
    }

    /// Remove any autoclose regions that no longer contain their selection or have invalid anchors in ranges.
    pub(super) fn invalidate_autoclose_regions(
        &mut self,
        mut selections: &[Selection<Anchor>],
        buffer: &MultiBufferSnapshot,
    ) {
        self.autoclose_regions.retain(|state| {
            if !state.range.start.is_valid(buffer) || !state.range.end.is_valid(buffer) {
                return false;
            }

            let mut i = 0;
            while let Some(selection) = selections.get(i) {
                if selection.end.cmp(&state.range.start, buffer).is_lt() {
                    selections = &selections[1..];
                    continue;
                }
                if selection.start.cmp(&state.range.end, buffer).is_gt() {
                    break;
                }
                if selection.id == state.selection_id {
                    return true;
                } else {
                    i += 1;
                }
            }
            false
        });
    }
}
