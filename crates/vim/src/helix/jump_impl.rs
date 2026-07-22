use super::*;

impl Vim {
    pub(super) fn start_helix_jump(
        &mut self,
        behaviour: HelixJumpBehaviour,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let allow_targets_in_selection = self.mode.has_selection();
        let Some(data) = self.collect_helix_jump_data(allow_targets_in_selection, window, cx)
        else {
            return;
        };

        if data.labels.is_empty() {
            self.clear_helix_jump_ui(window, cx);
            return;
        }

        if !self.apply_helix_jump_ui(data.overlays, window, cx) {
            return;
        }

        self.push_operator(
            Operator::HelixJump {
                behaviour,
                first_char: None,
                labels: data.labels,
            },
            window,
            cx,
        );
    }

    pub(super) fn collect_helix_jump_data(
        &mut self,
        allow_targets_in_selection: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<HelixJumpUiData> {
        self.update_editor(cx, |_, editor, cx| {
            let snapshot = editor.snapshot(window, cx);
            let display_snapshot = &snapshot.display_snapshot;
            let buffer_snapshot = display_snapshot.buffer_snapshot();
            let visible_range = Self::visible_jump_range(editor, &snapshot, display_snapshot, cx);
            let start_offset = buffer_snapshot.point_to_offset(visible_range.start);
            let end_offset = buffer_snapshot.point_to_offset(visible_range.end);

            let selections = editor.selections.all::<Point>(&display_snapshot);
            let skip_data = Self::selection_skip_offsets(
                buffer_snapshot,
                &selections,
                allow_targets_in_selection,
            );

            // Get the primary cursor position for alternating forward/backward labeling
            let cursor_offset = selections
                .first()
                .map(|s| buffer_snapshot.point_to_offset(s.head()))
                .unwrap_or(start_offset);

            let style = editor.style(cx);
            let font = style.text.font();
            let font_size = style.text.font_size.to_pixels(window.rem_size());
            let label_color = cx.theme().colors().vim_helix_jump_label_foreground;

            Self::build_helix_jump_ui_data(
                buffer_snapshot,
                start_offset,
                end_offset,
                cursor_offset,
                label_color,
                &skip_data,
                window.text_system(),
                font,
                font_size,
            )
        })
    }

    pub(super) fn visible_jump_range(
        editor: &Editor,
        snapshot: &editor::EditorSnapshot,
        display_snapshot: &DisplaySnapshot,
        cx: &App,
    ) -> Range<Point> {
        let visible_range = editor.multi_buffer_visible_range(display_snapshot, cx);
        if editor.visible_line_count().is_some() || visible_range.start != visible_range.end {
            return visible_range;
        }

        let scroll_position = snapshot.scroll_position();
        let top_row = scroll_position.y.floor().max(0.0) as u32;
        let visible_rows = display_snapshot
            .max_point()
            .row()
            .0
            .saturating_sub(top_row)
            .saturating_add(1);
        let start_display_point = DisplayPoint::new(DisplayRow(top_row), 0);
        let end_display_point =
            DisplayPoint::new(DisplayRow(top_row.saturating_add(visible_rows)), 0);

        display_snapshot.display_point_to_point(start_display_point, Bias::Left)
            ..display_snapshot.display_point_to_point(end_display_point, Bias::Right)
    }

    pub(super) fn build_helix_jump_ui_data(
        buffer: &MultiBufferSnapshot,
        start_offset: MultiBufferOffset,
        end_offset: MultiBufferOffset,
        cursor_offset: MultiBufferOffset,
        label_color: Hsla,
        skip_data: &HelixJumpSkipData,
        text_system: &WindowTextSystem,
        font: Font,
        font_size: Pixels,
    ) -> HelixJumpUiData {
        if start_offset >= end_offset {
            return HelixJumpUiData::default();
        }

        // First pass: collect all word candidates without assigning labels
        let candidates = Self::collect_jump_candidates(buffer, start_offset, end_offset, skip_data);

        if candidates.is_empty() {
            return HelixJumpUiData::default();
        }

        let ordered_candidates = Self::order_jump_candidates(candidates, cursor_offset);

        // Now assign labels and build UI data
        let mut labels = Vec::with_capacity(ordered_candidates.len());
        let mut overlays = Vec::with_capacity(ordered_candidates.len());

        let width_of = |text: &str| -> Pixels {
            if text.is_empty() {
                return px(0.0);
            }

            let run = gpui::TextRun {
                len: text.len(),
                font: font.clone(),
                color: Hsla::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };

            text_system.layout_line(text, font_size, &[run], None).width
        };

        let is_monospace = Self::is_monospace_jump_font(text_system, &font, font_size);

        for (label_index, candidate) in ordered_candidates.into_iter().enumerate() {
            let start_anchor = buffer.anchor_after(candidate.word_start);
            let end_anchor = buffer.anchor_after(candidate.word_end);
            let label = Self::jump_label_for_index(label_index);
            let label_text = label.iter().collect::<String>();
            // Monospace fonts: the label always matches the width of the first two characters,
            // so no per-word measurement is needed.
            // Proportional fonts: a label like "mw" can be wider than a short word like "if",
            // so we hide enough of the word (and possibly trailing whitespace) to make room,
            // or shift the label left into preceding whitespace.
            let fit = if is_monospace {
                JumpLabelFit::monospace(candidate.first_two_end)
            } else {
                let label_width = width_of(&label_text);
                Self::fit_proportional_jump_label(
                    buffer,
                    &candidate,
                    end_offset,
                    label_width,
                    &width_of,
                )
            };

            let hide_end_anchor = buffer.anchor_after(fit.hide_end_offset);

            labels.push(HelixJumpLabel {
                label,
                range: start_anchor..end_anchor,
            });

            overlays.push(NavigationTargetOverlay {
                target_range: start_anchor..end_anchor,
                label: NavigationOverlayLabel {
                    text: label_text.into(),
                    text_color: label_color,
                    x_offset: -fit.left_shift,
                    scale_factor: fit.scale_factor,
                },
                covered_text_range: Some(start_anchor..hide_end_anchor),
            });
        }

        HelixJumpUiData { labels, overlays }
    }

    pub(super) fn collect_jump_candidates(
        buffer: &MultiBufferSnapshot,
        start_offset: MultiBufferOffset,
        end_offset: MultiBufferOffset,
        skip_data: &HelixJumpSkipData,
    ) -> Vec<JumpCandidate> {
        let mut candidates = Vec::new();

        let mut offset = start_offset;
        let mut in_word = false;
        let mut word_start = start_offset;
        let mut first_two_end = start_offset;
        let mut char_count = 0;

        for chunk in buffer.text_for_range(start_offset..end_offset) {
            for (idx, ch) in chunk.char_indices() {
                let absolute = offset + idx;
                let is_word = is_jump_word_char(ch);
                if is_word {
                    if !in_word {
                        in_word = true;
                        word_start = absolute;
                        char_count = 0;
                    }
                    if char_count == 1 {
                        first_two_end = absolute + ch.len_utf8();
                    }
                    char_count += 1;
                }

                if !is_word && in_word {
                    if char_count >= 2
                        && !Self::should_skip_jump_candidate(word_start, absolute, skip_data)
                    {
                        candidates.push(JumpCandidate {
                            word_start,
                            word_end: absolute,
                            first_two_end,
                        });
                    }
                    in_word = false;
                }
            }
            offset += chunk.len();
        }

        // Handle word at end of buffer
        if in_word
            && char_count >= 2
            && !Self::should_skip_jump_candidate(word_start, end_offset, skip_data)
        {
            candidates.push(JumpCandidate {
                word_start,
                word_end: end_offset,
                first_two_end,
            });
        }

        candidates
    }

    pub(super) fn selection_skip_offsets(
        buffer: &MultiBufferSnapshot,
        selections: &[Selection<Point>],
        allow_targets_in_selection: bool,
    ) -> HelixJumpSkipData {
        let mut skip_points = Vec::with_capacity(selections.len());
        let mut skip_ranges = Vec::new();

        for selection in selections {
            let head_offset = buffer.point_to_offset(selection.head());
            skip_points.push(head_offset);

            if !allow_targets_in_selection && selection.start != selection.end {
                let mut start = buffer.point_to_offset(selection.start);
                let mut end = buffer.point_to_offset(selection.end);
                if start > end {
                    std::mem::swap(&mut start, &mut end);
                }
                skip_ranges.push(start..end);
            }
        }

        skip_points.sort_unstable();

        skip_ranges.sort_unstable_by_key(|range| range.start);
        let mut merged_ranges: Vec<Range<MultiBufferOffset>> =
            Vec::with_capacity(skip_ranges.len());
        for range in skip_ranges {
            if let Some(previous_range) = merged_ranges.last_mut()
                && range.start <= previous_range.end
            {
                previous_range.end = previous_range.end.max(range.end);
            } else {
                merged_ranges.push(range);
            }
        }

        HelixJumpSkipData {
            points: skip_points,
            ranges: merged_ranges,
        }
    }

    pub(super) fn should_skip_jump_candidate(
        word_start: MultiBufferOffset,
        word_end: MultiBufferOffset,
        skip_data: &HelixJumpSkipData,
    ) -> bool {
        // word_end is exclusive, so points at the following delimiter should not skip the word.
        let point_index = skip_data
            .points
            .partition_point(|offset| *offset < word_start);
        if skip_data
            .points
            .get(point_index)
            .is_some_and(|offset| *offset < word_end)
        {
            return true;
        }

        let range_index = skip_data
            .ranges
            .partition_point(|range| range.end <= word_start);
        skip_data
            .ranges
            .get(range_index)
            .is_some_and(|range| range.start < word_end)
    }

    /// Interleave candidates so forward targets get even label indices (aa, ac, ae...)
    /// and backward targets get odd indices (ab, ad, af...), matching Helix's algorithm.
    /// This keeps the earliest label assignments close to the cursor in both directions.
    pub(super) fn order_jump_candidates(
        candidates: Vec<JumpCandidate>,
        cursor_offset: MultiBufferOffset,
    ) -> Vec<JumpCandidate> {
        let mut forward = Vec::with_capacity(candidates.len());
        let mut backward = Vec::new();

        for candidate in candidates {
            if candidate.word_start < cursor_offset {
                backward.push(candidate);
            } else {
                forward.push(candidate);
            }
        }

        backward.reverse();

        let mut ordered_candidates =
            Vec::with_capacity((forward.len() + backward.len()).min(HELIX_JUMP_LABEL_LIMIT));
        let mut forward_candidates = forward.into_iter();
        let mut backward_candidates = backward.into_iter();

        loop {
            let mut pushed_candidate = false;

            if ordered_candidates.len() < HELIX_JUMP_LABEL_LIMIT
                && let Some(candidate) = forward_candidates.next()
            {
                ordered_candidates.push(candidate);
                pushed_candidate = true;
            }

            if ordered_candidates.len() < HELIX_JUMP_LABEL_LIMIT
                && let Some(candidate) = backward_candidates.next()
            {
                ordered_candidates.push(candidate);
                pushed_candidate = true;
            }

            if !pushed_candidate {
                break;
            }
        }

        ordered_candidates
    }

    pub(super) fn jump_label_for_index(index: usize) -> [char; 2] {
        [
            HELIX_JUMP_ALPHABET[index / HELIX_JUMP_ALPHABET.len()],
            HELIX_JUMP_ALPHABET[index % HELIX_JUMP_ALPHABET.len()],
        ]
    }
}
