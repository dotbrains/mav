use super::*;

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let snapshot = self.buffer.read(cx).read(cx);
        let start = snapshot.clip_offset_utf16(
            MultiBufferOffsetUtf16(OffsetUtf16(range_utf16.start)),
            Bias::Left,
        );
        let end = snapshot.clip_offset_utf16(
            MultiBufferOffsetUtf16(OffsetUtf16(range_utf16.end)),
            Bias::Right,
        );
        if (start.0.0..end.0.0) != range_utf16 {
            adjusted_range.replace(start.0.0..end.0.0);
        }
        Some(snapshot.text_for_range(start..end).collect())
    }

    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        // Prevent the IME menu from appearing when holding down an alphabetic key
        // while input is disabled.
        if !ignore_disabled_input && !self.input_enabled {
            return None;
        }

        let selection = self
            .selections
            .newest::<MultiBufferOffsetUtf16>(&self.display_snapshot(cx));
        let range = selection.range();

        Some(UTF16Selection {
            range: range.start.0.0..range.end.0.0,
            reversed: selection.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, cx: &mut Context<Self>) -> Option<Range<usize>> {
        let snapshot = self.buffer.read(cx).read(cx);
        let range = self
            .text_highlights(HighlightKey::InputComposition, cx)?
            .1
            .first()?;
        Some(range.start.to_offset_utf16(&snapshot).0.0..range.end.to_offset_utf16(&snapshot).0.0)
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.clear_highlights(HighlightKey::InputComposition, cx);
        self.ime_transaction.take();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            cx.emit(EditorEvent::InputIgnored { text: text.into() });
            return;
        }

        self.transact(window, cx, |this, window, cx| {
            let new_selected_ranges = if let Some(range_utf16) = range_utf16 {
                if let Some(marked_ranges) = this.marked_text_ranges(cx) {
                    // During IME composition, macOS reports the replacement range
                    // relative to the first marked region (the only one visible via
                    // marked_text_range). The correct targets for replacement are the
                    // marked ranges themselves — one per cursor — so use them directly.
                    Some(marked_ranges)
                } else if range_utf16.start == range_utf16.end {
                    // An empty replacement range means "insert at cursor" with no text
                    // to replace. macOS reports the cursor position from its own
                    // (single-cursor) view of the buffer, which diverges from our actual
                    // cursor positions after multi-cursor edits have shifted offsets.
                    // Treating this as range_utf16=None lets each cursor insert in place.
                    None
                } else {
                    // Outside of IME composition (e.g. Accessibility Keyboard word
                    // completion), the range is an absolute document offset for the
                    // newest cursor. Fan it out to all cursors via
                    // selection_replacement_ranges, which applies the delta relative
                    // to the newest selection to every cursor.
                    let range_utf16 = MultiBufferOffsetUtf16(OffsetUtf16(range_utf16.start))
                        ..MultiBufferOffsetUtf16(OffsetUtf16(range_utf16.end));
                    Some(this.selection_replacement_ranges(range_utf16, cx))
                }
            } else {
                this.marked_text_ranges(cx)
            };

            let range_to_replace = new_selected_ranges.as_ref().and_then(|ranges_to_replace| {
                let newest_selection_id = this.selections.newest_anchor().id;
                this.selections
                    .all::<MultiBufferOffsetUtf16>(&this.display_snapshot(cx))
                    .iter()
                    .zip(ranges_to_replace.iter())
                    .find_map(|(selection, range)| {
                        if selection.id == newest_selection_id {
                            Some(
                                (range.start.0.0 as isize - selection.head().0.0 as isize)
                                    ..(range.end.0.0 as isize - selection.head().0.0 as isize),
                            )
                        } else {
                            None
                        }
                    })
            });

            cx.emit(EditorEvent::InputHandled {
                utf16_range_to_replace: range_to_replace,
                text: text.into(),
            });

            if let Some(new_selected_ranges) = new_selected_ranges {
                // Only backspace if at least one range covers actual text. When all
                // ranges are empty (e.g. a trailing-space insertion from Accessibility
                // Keyboard sends replacementRange=cursor..cursor), backspace would
                // incorrectly delete the character just before the cursor.
                let should_backspace = new_selected_ranges.iter().any(|r| r.start != r.end);
                this.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    selections.select_ranges(new_selected_ranges)
                });
                if should_backspace {
                    this.backspace(&Default::default(), window, cx);
                }
            }

            this.handle_input(text, window, cx);
        });

        if let Some(transaction) = self.ime_transaction {
            self.buffer.update(cx, |buffer, cx| {
                buffer.group_until_transaction(transaction, cx);
            });
        }

        self.unmark_text(window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            return;
        }

        let transaction = self.transact(window, cx, |this, window, cx| {
            let ranges_to_replace = if let Some(mut marked_ranges) = this.marked_text_ranges(cx) {
                let snapshot = this.buffer.read(cx).read(cx);
                if let Some(relative_range_utf16) = range_utf16.as_ref() {
                    for marked_range in &mut marked_ranges {
                        marked_range.end = marked_range.start + relative_range_utf16.end;
                        marked_range.start += relative_range_utf16.start;
                        marked_range.start =
                            snapshot.clip_offset_utf16(marked_range.start, Bias::Left);
                        marked_range.end =
                            snapshot.clip_offset_utf16(marked_range.end, Bias::Right);
                    }
                }
                Some(marked_ranges)
            } else if let Some(range_utf16) = range_utf16 {
                let range_utf16 = MultiBufferOffsetUtf16(OffsetUtf16(range_utf16.start))
                    ..MultiBufferOffsetUtf16(OffsetUtf16(range_utf16.end));
                Some(this.selection_replacement_ranges(range_utf16, cx))
            } else {
                None
            };

            let range_to_replace = ranges_to_replace.as_ref().and_then(|ranges_to_replace| {
                let newest_selection_id = this.selections.newest_anchor().id;
                this.selections
                    .all::<MultiBufferOffsetUtf16>(&this.display_snapshot(cx))
                    .iter()
                    .zip(ranges_to_replace.iter())
                    .find_map(|(selection, range)| {
                        if selection.id == newest_selection_id {
                            Some(
                                (range.start.0.0 as isize - selection.head().0.0 as isize)
                                    ..(range.end.0.0 as isize - selection.head().0.0 as isize),
                            )
                        } else {
                            None
                        }
                    })
            });

            cx.emit(EditorEvent::InputHandled {
                utf16_range_to_replace: range_to_replace,
                text: text.into(),
            });

            if let Some(ranges) = ranges_to_replace {
                this.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_ranges(ranges)
                });
            }

            let marked_ranges = {
                let snapshot = this.buffer.read(cx).read(cx);
                this.selections
                    .disjoint_anchors_arc()
                    .iter()
                    .map(|selection| {
                        selection.start.bias_left(&snapshot)..selection.end.bias_right(&snapshot)
                    })
                    .collect::<Vec<_>>()
            };

            if text.is_empty() {
                this.unmark_text(window, cx);
            } else {
                this.highlight_text(
                    HighlightKey::InputComposition,
                    marked_ranges.clone(),
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

            // Disable auto-closing when composing text (i.e. typing a `"` on a Brazilian keyboard)
            let use_autoclose = this.use_autoclose;
            let use_auto_surround = this.use_auto_surround;
            this.set_use_autoclose(false);
            this.set_use_auto_surround(false);
            this.handle_input(text, window, cx);
            this.set_use_autoclose(use_autoclose);
            this.set_use_auto_surround(use_auto_surround);

            if let Some(new_selected_range) = new_selected_range_utf16 {
                let snapshot = this.buffer.read(cx).read(cx);
                let new_selected_ranges = marked_ranges
                    .into_iter()
                    .map(|marked_range| {
                        let insertion_start = marked_range.start.to_offset_utf16(&snapshot).0;
                        let new_start = MultiBufferOffsetUtf16(OffsetUtf16(
                            insertion_start.0 + new_selected_range.start,
                        ));
                        let new_end = MultiBufferOffsetUtf16(OffsetUtf16(
                            insertion_start.0 + new_selected_range.end,
                        ));
                        snapshot.clip_offset_utf16(new_start, Bias::Left)
                            ..snapshot.clip_offset_utf16(new_end, Bias::Right)
                    })
                    .collect::<Vec<_>>();

                drop(snapshot);
                this.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    selections.select_ranges(new_selected_ranges)
                });
            }
        });

        self.ime_transaction = self.ime_transaction.or(transaction);
        if let Some(transaction) = self.ime_transaction {
            self.buffer.update(cx, |buffer, cx| {
                buffer.group_until_transaction(transaction, cx);
            });
        }

        if self
            .text_highlights(HighlightKey::InputComposition, cx)
            .is_none()
        {
            self.ime_transaction.take();
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: gpui::Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Bounds<Pixels>> {
        let text_layout_details = self.text_layout_details(window, cx);
        let CharacterDimensions {
            em_width,
            em_advance,
            line_height,
        } = self.character_dimensions(window, cx);

        let snapshot = self.snapshot(window, cx);
        let scroll_position = snapshot.scroll_position();
        let scroll_left = scroll_position.x * ScrollOffset::from(em_advance);

        let start =
            MultiBufferOffsetUtf16(OffsetUtf16(range_utf16.start)).to_display_point(&snapshot);
        let x = Pixels::from(
            ScrollOffset::from(
                snapshot.x_for_display_point(start, &text_layout_details)
                    + self.gutter_dimensions.full_width(),
            ) - scroll_left,
        );
        let y = line_height * (start.row().as_f64() - scroll_position.y) as f32;

        Some(Bounds {
            origin: element_bounds.origin + point(x, y),
            size: size(em_width, line_height),
        })
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let position_map = self.last_position_map.as_ref()?;
        if !position_map.text_hitbox.contains(&point) {
            return None;
        }
        let display_point = position_map.point_for_position(point).previous_valid;
        let anchor = position_map
            .snapshot
            .display_point_to_anchor(display_point, Bias::Left);
        let utf16_offset = anchor.to_offset_utf16(&position_map.snapshot.buffer_snapshot());
        Some(utf16_offset.0.0)
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        self.expects_character_input
    }
}
