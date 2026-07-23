use crate::{
    Vim,
    motion::{self, Motion},
    object::{Object, surrounding_markers},
    state::Mode,
};
use editor::{Anchor, Bias, MultiBufferOffset, ToOffset, movement};
use gpui::{Context, Window};
use language::BracketPair;

mod pairs;
mod resolution;

pub use pairs::{
    BRACKET_PAIRS, QUOTE_PAIRS, SURROUND_PAIRS, SurroundPair, bracket_pair_for_str_helix,
    bracket_pair_for_str_vim, surround_alias, surround_pair_for_char_helix,
    surround_pair_for_char_vim,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurroundsType {
    Motion(Motion),
    Object(Object, bool),
    Selection,
}

impl Vim {
    pub fn add_surrounds(
        &mut self,
        text: Arc<str>,
        target: SurroundsType,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_recording(cx);
        let count = Vim::take_count(cx);
        let forced_motion = Vim::take_forced_motion(cx);
        let mode = self.mode;
        self.update_editor(cx, |_, editor, cx| {
            let text_layout_details = editor.text_layout_details(window, cx);
            editor.transact(window, cx, |editor, window, cx| {
                editor.set_clip_at_line_ends(false, cx);

                let pair = bracket_pair_for_str_vim(&text);
                let surround = pair.end != surround_alias((*text).as_ref());
                let display_map = editor.display_snapshot(cx);
                let display_selections = editor.selections.all_adjusted_display(&display_map);
                let mut edits = Vec::new();
                let mut anchors = Vec::new();

                for selection in &display_selections {
                    let range = match &target {
                        SurroundsType::Object(object, around) => {
                            object.range(&display_map, selection.clone(), *around, None)
                        }
                        SurroundsType::Motion(motion) => {
                            motion
                                .range(
                                    &display_map,
                                    selection.clone(),
                                    count,
                                    &text_layout_details,
                                    forced_motion,
                                )
                                .map(|(mut range, _)| {
                                    // The Motion::CurrentLine operation will contain the newline of the current line and leading/trailing whitespace
                                    if let Motion::CurrentLine = motion {
                                        range.start = motion::first_non_whitespace(
                                            &display_map,
                                            false,
                                            range.start,
                                        );
                                        range.end = movement::saturating_right(
                                            &display_map,
                                            motion::last_non_whitespace(&display_map, range.end, 1),
                                        );
                                    }
                                    range
                                })
                        }
                        SurroundsType::Selection => Some(selection.range()),
                    };

                    if let Some(range) = range {
                        let start = range.start.to_offset(&display_map, Bias::Right);
                        let end = range.end.to_offset(&display_map, Bias::Left);
                        let (start_cursor_str, end_cursor_str) = if mode == Mode::VisualLine {
                            (format!("{}\n", pair.start), format!("\n{}", pair.end))
                        } else {
                            let maybe_space = if surround { " " } else { "" };
                            (
                                format!("{}{}", pair.start, maybe_space),
                                format!("{}{}", maybe_space, pair.end),
                            )
                        };
                        let start_anchor = display_map.buffer_snapshot().anchor_before(start);

                        edits.push((start..start, start_cursor_str));
                        edits.push((end..end, end_cursor_str));
                        anchors.push(start_anchor..start_anchor);
                    } else {
                        let start_anchor = display_map
                            .buffer_snapshot()
                            .anchor_before(selection.head().to_offset(&display_map, Bias::Left));
                        anchors.push(start_anchor..start_anchor);
                    }
                }

                editor.edit(edits, cx);
                editor.set_clip_at_line_ends(true, cx);
                editor.change_selections(Default::default(), window, cx, |s| {
                    if mode == Mode::VisualBlock {
                        s.select_anchor_ranges(anchors.into_iter().take(1))
                    } else {
                        s.select_anchor_ranges(anchors)
                    }
                });
            });
        });
        self.switch_mode(Mode::Normal, false, window, cx);
    }

    pub fn delete_surrounds(
        &mut self,
        text: Arc<str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_recording(cx);

        // only legitimate surrounds can be removed
        let Some(first_char) = text.chars().next() else {
            return;
        };
        let Some(surround_pair) = surround_pair_for_char_vim(first_char) else {
            return;
        };
        let Some(pair_object) = surround_pair.to_object() else {
            return;
        };
        let pair = surround_pair.to_bracket_pair();
        let surround = pair.end != *text;

        self.update_editor(cx, |_, editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                editor.set_clip_at_line_ends(false, cx);

                let display_map = editor.display_snapshot(cx);
                let display_selections = editor.selections.all_display(&display_map);
                let mut edits = Vec::new();
                let mut anchors = Vec::new();

                for selection in &display_selections {
                    let start = selection.start.to_offset(&display_map, Bias::Left);
                    if let Some(range) =
                        pair_object.range(&display_map, selection.clone(), true, None)
                    {
                        // If the current parenthesis object is single-line,
                        // then we need to filter whether it is the current line or not
                        if !pair_object.is_multiline() {
                            let is_same_row = selection.start.row() == range.start.row()
                                && selection.end.row() == range.end.row();
                            if !is_same_row {
                                anchors.push(start..start);
                                continue;
                            }
                        }
                        // This is a bit cumbersome, and it is written to deal with some special cases, as shown below
                        // hello«ˇ  "hello in a word"  »again.
                        // Sometimes the expand_selection will not be matched at both ends, and there will be extra spaces
                        // In order to be able to accurately match and replace in this case, some cumbersome methods are used
                        let mut chars_and_offset = display_map
                            .buffer_chars_at(range.start.to_offset(&display_map, Bias::Left))
                            .peekable();
                        while let Some((ch, offset)) = chars_and_offset.next() {
                            if ch.to_string() == pair.start {
                                let start = offset;
                                let mut end = start + 1usize;
                                if surround
                                    && let Some((next_ch, _)) = chars_and_offset.peek()
                                    && next_ch.eq(&' ')
                                {
                                    end += 1;
                                }
                                edits.push((start..end, ""));
                                anchors.push(start..start);
                                break;
                            }
                        }
                        let mut reverse_chars_and_offsets = display_map
                            .reverse_buffer_chars_at(range.end.to_offset(&display_map, Bias::Left))
                            .peekable();
                        while let Some((ch, offset)) = reverse_chars_and_offsets.next() {
                            if ch.to_string() == pair.end {
                                let mut start = offset;
                                let end = start + 1usize;
                                if surround
                                    && let Some((next_ch, _)) = reverse_chars_and_offsets.peek()
                                    && next_ch.eq(&' ')
                                {
                                    start -= 1;
                                }
                                edits.push((start..end, ""));
                                break;
                            }
                        }
                    } else {
                        anchors.push(start..start);
                    }
                }

                editor.change_selections(Default::default(), window, cx, |s| {
                    s.select_ranges(anchors);
                });
                edits.sort_by_key(|(range, _)| range.start);
                editor.edit(edits, cx);
                editor.set_clip_at_line_ends(true, cx);
            });
        });
    }

    pub fn change_surrounds(
        &mut self,
        text: Arc<str>,
        target: Object,
        opening: bool,
        bracket_anchors: Vec<Option<(Anchor, Anchor)>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(will_replace_pair) = self.object_to_bracket_pair(target, cx) {
            self.stop_recording(cx);
            self.update_editor(cx, |_, editor, cx| {
                editor.transact(window, cx, |editor, window, cx| {
                    editor.set_clip_at_line_ends(false, cx);

                    let pair = bracket_pair_for_str_vim(&text);

                    // A single space should be added if the new surround is a
                    // bracket and not a quote (pair.start != pair.end) and if
                    // the bracket used is the opening bracket.
                    let add_space =
                        !(pair.start == pair.end) && (pair.end != surround_alias((*text).as_ref()));

                    // Space should be preserved if either the surrounding
                    // characters being updated are quotes
                    // (will_replace_pair.start == will_replace_pair.end) or if
                    // the bracket used in the command is not an opening
                    // bracket.
                    let preserve_space =
                        will_replace_pair.start == will_replace_pair.end || !opening;

                    let display_map = editor.display_snapshot(cx);
                    let mut edits = Vec::new();

                    // Collect (open_offset, close_offset) pairs to replace from the
                    // pre-computed anchors stored during check_and_move_to_valid_bracket_pair.
                    let mut pairs_to_replace: Vec<(MultiBufferOffset, MultiBufferOffset)> =
                        Vec::new();
                    let snapshot = display_map.buffer_snapshot();
                    for anchors in &bracket_anchors {
                        let Some((open_anchor, close_anchor)) = anchors else {
                            continue;
                        };
                        let pair = (
                            open_anchor.to_offset(&snapshot),
                            close_anchor.to_offset(&snapshot),
                        );
                        if !pairs_to_replace.contains(&pair) {
                            pairs_to_replace.push(pair);
                        }
                    }

                    for (open_offset, close_offset) in pairs_to_replace {
                        let mut open_str = pair.start.clone();
                        let mut chars_and_offset =
                            display_map.buffer_chars_at(open_offset).peekable();
                        chars_and_offset.next(); // skip the bracket itself
                        let mut open_range_end = open_offset + 1usize;
                        while let Some((next_ch, _)) = chars_and_offset.next()
                            && next_ch == ' '
                        {
                            open_range_end += 1;
                            if preserve_space {
                                open_str.push(next_ch);
                            }
                        }
                        if add_space {
                            open_str.push(' ');
                        }
                        let edit_len = open_range_end - open_offset;
                        edits.push((open_offset..open_range_end, open_str));

                        let mut close_str = String::new();
                        let close_end = close_offset + 1usize;
                        let mut close_start = close_offset;
                        for (next_ch, _) in display_map.reverse_buffer_chars_at(close_offset) {
                            if next_ch != ' '
                                || close_str.len() >= edit_len - 1
                                || close_start <= open_range_end
                            {
                                break;
                            }
                            close_start -= 1;
                            if preserve_space {
                                close_str.push(next_ch);
                            }
                        }
                        if add_space {
                            close_str.push(' ');
                        }
                        close_str.push_str(&pair.end);
                        edits.push((close_start..close_end, close_str));
                    }

                    let stable_anchors = editor
                        .selections
                        .disjoint_anchors_arc()
                        .iter()
                        .map(|selection| {
                            let start = selection.start.bias_left(&display_map.buffer_snapshot());
                            start..start
                        })
                        .collect::<Vec<_>>();
                    edits.sort_by_key(|(range, _)| range.start);
                    editor.edit(edits, cx);
                    editor.set_clip_at_line_ends(true, cx);
                    editor.change_selections(Default::default(), window, cx, |s| {
                        s.select_anchor_ranges(stable_anchors);
                    });
                });
            });
        }
    }

    /// **Only intended for use by the `cs` (change surrounds) operator.**
    ///
    /// For each cursor, checks whether it is surrounded by a valid bracket pair for the given
    /// object. Moves each cursor to the opening bracket of its found pair, and returns a
    /// `Vec<Option<(Anchor, Anchor)>>` with one entry per selection containing the pre-computed
    /// open and close bracket positions.
    ///
    /// Storing these anchors avoids re-running the bracket search from the moved cursor position,
    /// which can misidentify the opening bracket for symmetric quote characters when the same
    /// character appears earlier on the line (e.g. `I'm 'good'`).
    ///
    /// Returns an empty `Vec` if no valid pair was found for any cursor.
    pub fn prepare_and_move_to_valid_bracket_pair(
        &mut self,
        object: Object,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Option<(Anchor, Anchor)>> {
        let mut matched_pair_anchors: Vec<Option<(Anchor, Anchor)>> = Vec::new();
        if let Some(pair) = self.object_to_bracket_pair(object, cx) {
            self.update_editor(cx, |_, editor, cx| {
                editor.transact(window, cx, |editor, window, cx| {
                    editor.set_clip_at_line_ends(false, cx);
                    let display_map = editor.display_snapshot(cx);
                    let selections = editor.selections.all_adjusted_display(&display_map);
                    let mut updated_cursor_ranges = Vec::new();

                    for selection in &selections {
                        let start = selection.start.to_offset(&display_map, Bias::Left);
                        let in_range = object
                            .range(&display_map, selection.clone(), true, None)
                            .filter(|range| {
                                object.is_multiline()
                                    || (selection.start.row() == range.start.row()
                                        && selection.end.row() == range.end.row())
                            });
                        let Some(range) = in_range else {
                            updated_cursor_ranges.push(start..start);
                            matched_pair_anchors.push(None);
                            continue;
                        };

                        let range_start = range.start.to_offset(&display_map, Bias::Left);
                        let range_end = range.end.to_offset(&display_map, Bias::Left);
                        let open_offset = display_map
                            .buffer_chars_at(range_start)
                            .find(|(ch, _)| ch.to_string() == pair.start)
                            .map(|(_, offset)| offset);
                        let close_offset = display_map
                            .reverse_buffer_chars_at(range_end)
                            .find(|(ch, _)| ch.to_string() == pair.end)
                            .map(|(_, offset)| offset);

                        if let (Some(open), Some(close)) = (open_offset, close_offset) {
                            let snapshot = &display_map.buffer_snapshot();
                            updated_cursor_ranges.push(open..open);
                            matched_pair_anchors.push(Some((
                                snapshot.anchor_before(open),
                                snapshot.anchor_before(close),
                            )));
                        } else {
                            updated_cursor_ranges.push(start..start);
                            matched_pair_anchors.push(None);
                        }
                    }
                    editor.change_selections(Default::default(), window, cx, |s| {
                        s.select_ranges(updated_cursor_ranges);
                    });
                    editor.set_clip_at_line_ends(true, cx);

                    if !matched_pair_anchors.iter().any(|a| a.is_some()) {
                        matched_pair_anchors.clear();
                    }
                });
            });
        }
        matched_pair_anchors
    }
}

#[cfg(test)]
mod add_tests;
#[cfg(test)]
mod change_tests;
#[cfg(test)]
mod delete_tests;
#[cfg(test)]
mod pair_tests;
