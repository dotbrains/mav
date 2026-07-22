use super::*;

impl Vim {
    pub(super) fn is_monospace_jump_font(
        text_system: &WindowTextSystem,
        font: &Font,
        font_size: Pixels,
    ) -> bool {
        let font_id = text_system.resolve_font(font);
        let width_of_char = |ch| {
            text_system
                .advance(font_id, font_size, ch)
                .map(|size| size.width)
                .unwrap_or_else(|_| text_system.layout_width(font_id, font_size, ch))
        };

        let a = width_of_char('i');
        let b = width_of_char('w');
        let c = width_of_char('0');
        let d = width_of_char('1');
        let diff_1 = if a > b { a - b } else { b - a };
        let diff_2 = if c > d { c - d } else { d - c };
        diff_1 <= HELIX_JUMP_MONOSPACE_TOLERANCE && diff_2 <= HELIX_JUMP_MONOSPACE_TOLERANCE
    }

    /// Fit a jump label over a word in a proportional font.
    ///
    /// Prefer fitting within the word itself, using available whitespace to the left
    /// before consuming trailing whitespace after the word. If the label still cannot
    /// fit cleanly, allow a small amount of scaling.
    pub(super) fn fit_proportional_jump_label<F: Fn(&str) -> Pixels>(
        buffer: &MultiBufferSnapshot,
        candidate: &JumpCandidate,
        end_offset: MultiBufferOffset,
        label_width: Pixels,
        width_of: &F,
    ) -> JumpLabelFit {
        let fit_budget = Self::jump_label_fit_budget(buffer, candidate, end_offset, width_of);

        let mut hidden_prefix = HiddenPrefixFitState::new(candidate.first_two_end);
        let min_label_scale = if fit_budget.preserve_full_scale {
            1.0
        } else {
            HELIX_JUMP_MIN_LABEL_SCALE
        };

        hidden_prefix.extend_to_fit(
            buffer,
            candidate.word_start,
            candidate.word_end,
            candidate.word_end,
            label_width,
            fit_budget.max_left_shift,
            min_label_scale,
            width_of,
        );

        if label_width > px(0.0)
            && hidden_prefix.needs_more_width(label_width, fit_budget.max_left_shift)
            && fit_budget.allowed_trailing_hide_end > candidate.word_end
        {
            hidden_prefix.extend_to_fit(
                buffer,
                candidate.word_end,
                fit_budget.allowed_trailing_hide_end,
                candidate.word_end,
                label_width,
                fit_budget.max_left_shift,
                min_label_scale,
                width_of,
            );
        }

        // Jump candidates always contain at least two word characters, and the initial
        // scan above always measures through that second character before we read the width.
        let hidden_width = hidden_prefix.hidden_width;

        let left_shift = if label_width > hidden_width {
            (label_width - hidden_width).min(fit_budget.max_left_shift)
        } else {
            px(0.0)
        };

        let scale_factor = if label_width > px(0.0) {
            let scale = ((hidden_width + left_shift) / label_width).min(1.0);
            if scale < 1.0 { scale * 0.99 } else { 1.0 }
        } else {
            1.0
        };

        JumpLabelFit {
            hide_end_offset: hidden_prefix.hide_end_offset,
            left_shift,
            scale_factor: if fit_budget.preserve_full_scale {
                1.0
            } else {
                scale_factor
            },
        }
    }

    pub(super) fn jump_label_fit_budget<F: Fn(&str) -> Pixels>(
        buffer: &MultiBufferSnapshot,
        candidate: &JumpCandidate,
        end_offset: MultiBufferOffset,
        width_of: &F,
    ) -> JumpLabelFitBudget {
        let mut left_ws_rev = String::new();
        let mut left_ws_count = 0usize;
        let mut left_stopped_at_line_break = false;
        let mut left_stopped_at_non_ws = false;
        let mut left_hit_limit = false;

        for ch in buffer.reversed_chars_at(candidate.word_start) {
            if ch == '\n' || ch == '\r' {
                left_stopped_at_line_break = true;
                break;
            }

            if !ch.is_whitespace() {
                left_stopped_at_non_ws = true;
                break;
            }

            left_ws_count += 1;
            if left_ws_count > HELIX_JUMP_MAX_LEFT_WS_CHARS {
                left_hit_limit = true;
                break;
            }

            left_ws_rev.push(ch);
        }

        let left_ws: String = left_ws_rev.chars().rev().collect();
        let left_ws_width = width_of(&left_ws);
        let left_is_indentation =
            left_stopped_at_line_break || (!left_stopped_at_non_ws && !left_hit_limit);
        // Between tokens leave a small gap so the label doesn't touch the previous word;
        // for line-leading indentation the full width is safe.
        let min_left_gap = if left_is_indentation {
            px(0.0)
        } else {
            px(2.0)
        };
        let max_left_shift = (left_ws_width - min_left_gap).max(px(0.0));

        let mut allowed_trailing_hide_end = candidate.word_end;
        let mut ws_count = 0usize;
        let mut last_ws_start = candidate.word_end;
        let mut ws_end_offset = candidate.word_end;
        let mut next_non_ws = None;
        let mut hit_line_break_after_word = false;

        let mut ws_scan_offset = candidate.word_end;
        'scan: for chunk in buffer.text_for_range(candidate.word_end..end_offset) {
            for (idx, ch) in chunk.char_indices() {
                let absolute = ws_scan_offset + idx;
                if ch == '\n' || ch == '\r' {
                    hit_line_break_after_word = true;
                    break 'scan;
                }
                if !ch.is_whitespace() {
                    next_non_ws = Some(ch);
                    break 'scan;
                }

                ws_count += 1;
                last_ws_start = absolute;
                ws_end_offset = absolute + ch.len_utf8();
            }
            ws_scan_offset += chunk.len();
        }

        let preserve_full_scale = hit_line_break_after_word && next_non_ws.is_none()
            || matches!(
                buffer.chars_at(candidate.word_end).next(),
                None | Some('\n') | Some('\r')
            );

        if ws_count > 0 {
            let next_is_word = match next_non_ws {
                Some(ch) => is_jump_word_char(ch),
                None => false,
            };

            if next_is_word {
                // Keep at least one whitespace character visible so adjacent labels
                // remain visually separated.
                if ws_count > 1 {
                    allowed_trailing_hide_end = last_ws_start;
                }
            } else {
                // Next token is punctuation or end-of-range — safe to hide all whitespace.
                allowed_trailing_hide_end = ws_end_offset;
            }
        }

        JumpLabelFitBudget {
            max_left_shift,
            allowed_trailing_hide_end,
            preserve_full_scale,
        }
    }
}
