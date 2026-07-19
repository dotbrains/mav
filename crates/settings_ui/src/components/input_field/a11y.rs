use editor::{Editor, MultiBufferOffset};
use gpui::{A11ySubtreeBuilder, ElementId, Entity, accesskit};
use ui::prelude::*;

/// Compute the shared accessibility state for a focusable wrapper around a
/// single-line [`Editor`]:
///
/// - The value to report via `aria_value`. While the editor is focused this
///   is frozen at its focus-time content: screen readers announce the full value
///   on every change of a focused control, which would re-read the whole content
///   on each keystroke. The snapshot re-syncs on blur.
/// - A closure for `a11y_synthetic_children` exposing the editor's live text
///   and selection as AccessKit text runs, enabling the platform text
///   pattern (caret tracking, review commands, typed-character echo).
///
/// The caller must also give the element an id, a text input role (e.g.
/// [`Role::TextInput`]), a label, and track the editor's focus handle.
///
/// All work is skipped when accessibility is inactive (no assistive
/// technology connected), since the results are only observable through the
/// accessibility tree.
pub(crate) fn text_field_a11y_state(
    state_key: impl Into<ElementId>,
    editor: &Entity<Editor>,
    window: &mut Window,
    cx: &mut App,
) -> (String, impl FnOnce(&mut A11ySubtreeBuilder) + 'static) {
    let state = window.is_a11y_active().then(|| {
        let (text, selection_head, selection_tail) = editor.update(cx, |editor, cx| {
            let display_snapshot = editor.display_snapshot(cx);
            let selection = editor
                .selections
                .newest::<MultiBufferOffset>(&display_snapshot);
            (editor.text(cx), selection.head().0, selection.tail().0)
        });
        let is_focused = editor.read(cx).is_focused(window);

        let a11y_value = window.use_keyed_state((state_key.into(), "a11y-value"), cx, {
            let text = text.clone();
            move |_, _| text
        });
        if !is_focused && *a11y_value.read(cx) != text {
            *a11y_value.as_mut(cx) = text.clone();
        }
        let frozen_value = a11y_value.read(cx).clone();

        (frozen_value, text, selection_head, selection_tail)
    });

    let (frozen_value, run_data) = match state {
        Some((frozen_value, text, selection_head, selection_tail)) => {
            (frozen_value, Some((text, selection_head, selection_tail)))
        }
        None => (String::new(), None),
    };

    let text_runs = move |builder: &mut A11ySubtreeBuilder| {
        if let Some((text, selection_head, selection_tail)) = run_data {
            push_a11y_text_runs(builder, &text, selection_tail, selection_head);
        }
    };

    (frozen_value, text_runs)
}

/// AccessKit's `word_starts` uses `u8` indices, so a single text run cannot
/// exceed this many characters. Longer text is split into multiple runs.
const MAX_CHARS_PER_TEXT_RUN: usize = 255;

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn char_index_for_byte(text: &str, byte_offset: usize) -> usize {
    text.char_indices()
        .take_while(|(byte_ix, _)| *byte_ix < byte_offset)
        .count()
}

fn a11y_text_position(
    char_index: usize,
    synthetic_node_id: impl Fn(u64) -> accesskit::NodeId,
) -> accesskit::TextPosition {
    let chunk_index = if char_index > 0 && char_index.is_multiple_of(MAX_CHARS_PER_TEXT_RUN) {
        char_index / MAX_CHARS_PER_TEXT_RUN - 1
    } else {
        char_index / MAX_CHARS_PER_TEXT_RUN
    };
    accesskit::TextPosition {
        node: synthetic_node_id(chunk_index as u64),
        character_index: char_index - chunk_index * MAX_CHARS_PER_TEXT_RUN,
    }
}

fn build_a11y_text_runs(
    text: &str,
    selection_tail: usize,
    selection_head: usize,
    synthetic_node_id: impl Fn(u64) -> accesskit::NodeId,
) -> (
    Vec<(accesskit::NodeId, accesskit::Node)>,
    accesskit::TextSelection,
) {
    let chars: Vec<char> = text.chars().collect();
    let total_chars = chars.len();
    let num_chunks = total_chars.div_ceil(MAX_CHARS_PER_TEXT_RUN).max(1);

    let mut word_starts = Vec::new();
    let mut was_word_char = false;
    for (ix, c) in chars.iter().enumerate() {
        let is_word = is_word_char(*c);
        if is_word && !was_word_char {
            word_starts.push(ix);
        }
        was_word_char = is_word;
    }

    let mut runs = Vec::with_capacity(num_chunks);
    for chunk_index in 0..num_chunks {
        let char_start = chunk_index * MAX_CHARS_PER_TEXT_RUN;
        let char_end = (char_start + MAX_CHARS_PER_TEXT_RUN).min(total_chars);
        let chunk_chars = &chars[char_start..char_end];

        let mut node = accesskit::Node::new(accesskit::Role::TextRun);
        node.set_text_direction(accesskit::TextDirection::LeftToRight);
        node.set_value(chunk_chars.iter().collect::<String>());
        node.set_character_lengths(
            chunk_chars
                .iter()
                .map(|c| c.len_utf8() as u8)
                .collect::<Vec<u8>>(),
        );
        node.set_word_starts(
            word_starts
                .iter()
                .filter(|&&word_start| word_start >= char_start && word_start < char_end)
                .map(|&word_start| (word_start - char_start) as u8)
                .collect::<Vec<u8>>(),
        );
        if chunk_index > 0 {
            node.set_previous_on_line(synthetic_node_id(chunk_index as u64 - 1));
        }
        if chunk_index + 1 < num_chunks {
            node.set_next_on_line(synthetic_node_id(chunk_index as u64 + 1));
        }

        runs.push((synthetic_node_id(chunk_index as u64), node));
    }

    let anchor = a11y_text_position(
        char_index_for_byte(text, selection_tail),
        &synthetic_node_id,
    );
    let focus = a11y_text_position(
        char_index_for_byte(text, selection_head),
        &synthetic_node_id,
    );
    (runs, accesskit::TextSelection { anchor, focus })
}

fn push_a11y_text_runs(
    builder: &mut A11ySubtreeBuilder,
    text: &str,
    selection_tail: usize,
    selection_head: usize,
) {
    let (runs, selection) = build_a11y_text_runs(text, selection_tail, selection_head, |chunk| {
        builder.synthetic_node_id(chunk)
    });
    for (id, node) in runs {
        builder.push_child(id, node);
    }
    builder.parent_node().set_text_selection(selection);
}

#[cfg(test)]
mod tests {
    use super::build_a11y_text_runs;
    use gpui::accesskit::NodeId;
    use gpui::proptest::strategy::Strategy;

    fn arbitrary_text() -> impl Strategy<Value = String> {
        let character = gpui::proptest::prop_oneof![
            gpui::proptest::char::range(' ', '~'),
            gpui::proptest::char::range('\u{00A1}', '\u{00FF}'),
            gpui::proptest::char::range('\u{0100}', '\u{024F}'),
            gpui::proptest::char::range('\u{0400}', '\u{04FF}'),
            gpui::proptest::char::range('\u{0600}', '\u{06FF}'),
            gpui::proptest::char::range('\u{4E00}', '\u{9FFF}'),
            gpui::proptest::char::range('\u{1F300}', '\u{1FAFF}'),
            gpui::proptest::char::any(),
        ];
        gpui::proptest::collection::vec(character, 0..600)
            .prop_map(|chars| chars.into_iter().collect::<String>())
    }

    #[gpui::property_test]
    fn building_text_runs_never_panics(
        #[strategy = arbitrary_text()] text: String,
        selection_tail: usize,
        selection_head: usize,
    ) {
        let _ = build_a11y_text_runs(&text, selection_tail, selection_head, NodeId);
    }
}
