use super::*;

pub(super) const HELIX_JUMP_ALPHABET: &[char; 26] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z',
];
pub(super) const HELIX_JUMP_LABEL_LIMIT: usize =
    HELIX_JUMP_ALPHABET.len() * HELIX_JUMP_ALPHABET.len();
pub(super) const HELIX_JUMP_MONOSPACE_TOLERANCE: Pixels = px(0.5);
pub(super) const HELIX_JUMP_MIN_LABEL_SCALE: f32 = 1.0;
const HELIX_JUMP_MAX_HIDDEN_CHARS: usize = 16;
pub(super) const HELIX_JUMP_MAX_LEFT_WS_CHARS: usize = 32;

pub(super) fn is_jump_word_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

/// A word candidate for jump labels, before label assignment.
#[derive(Clone)]
pub(super) struct JumpCandidate {
    pub(super) word_start: MultiBufferOffset,
    pub(super) word_end: MultiBufferOffset,
    pub(super) first_two_end: MultiBufferOffset,
}

pub(super) struct HelixJumpSkipData {
    pub(super) points: Vec<MultiBufferOffset>,
    pub(super) ranges: Vec<Range<MultiBufferOffset>>,
}

pub(super) struct JumpLabelFit {
    pub(super) hide_end_offset: MultiBufferOffset,
    pub(super) left_shift: Pixels,
    pub(super) scale_factor: f32,
}

pub(super) struct JumpLabelFitBudget {
    pub(super) max_left_shift: Pixels,
    pub(super) allowed_trailing_hide_end: MultiBufferOffset,
    pub(super) preserve_full_scale: bool,
}

pub(super) struct HiddenPrefixFitState {
    text: String,
    pub(super) hide_end_offset: MultiBufferOffset,
    pub(super) hidden_width: Pixels,
    total_char_count: usize,
    word_char_count: usize,
}

impl JumpLabelFit {
    pub(super) fn monospace(hide_end_offset: MultiBufferOffset) -> Self {
        Self {
            hide_end_offset,
            left_shift: px(0.0),
            scale_factor: 1.0,
        }
    }
}

impl HiddenPrefixFitState {
    pub(super) fn new(hide_end_offset: MultiBufferOffset) -> Self {
        Self {
            text: String::new(),
            hide_end_offset,
            hidden_width: px(0.0),
            total_char_count: 0,
            word_char_count: 0,
        }
    }

    pub(super) fn needs_more_width(&self, label_width: Pixels, max_left_shift: Pixels) -> bool {
        (self.hidden_width + max_left_shift) / label_width < HELIX_JUMP_MIN_LABEL_SCALE
    }

    pub(super) fn extend_to_fit<F: Fn(&str) -> Pixels>(
        &mut self,
        buffer: &MultiBufferSnapshot,
        range_start: MultiBufferOffset,
        range_end: MultiBufferOffset,
        word_end: MultiBufferOffset,
        label_width: Pixels,
        max_left_shift: Pixels,
        min_label_scale: f32,
        width_of: &F,
    ) {
        let mut offset = range_start;
        for chunk in buffer.text_for_range(range_start..range_end) {
            for (idx, ch) in chunk.char_indices() {
                let absolute = offset + idx;

                self.total_char_count += 1;
                if self.total_char_count > HELIX_JUMP_MAX_HIDDEN_CHARS {
                    return;
                }

                self.text.push(ch);
                let end_offset = absolute + ch.len_utf8();

                if absolute < word_end && is_jump_word_char(ch) {
                    self.word_char_count += 1;
                }

                if self.word_char_count < 2 {
                    continue;
                }

                self.hide_end_offset = end_offset;
                self.hidden_width = width_of(self.text.as_str());

                let effective_width = self.hidden_width + max_left_shift;
                let scale_needed = if label_width > px(0.0) {
                    (effective_width / label_width).min(1.0)
                } else {
                    1.0
                };

                if scale_needed >= min_label_scale {
                    return;
                }
            }
            offset += chunk.len();
        }
    }
}

#[derive(Default)]
pub(super) struct HelixJumpUiData {
    pub(super) labels: Vec<HelixJumpLabel>,
    pub(super) overlays: Vec<NavigationTargetOverlay>,
}
