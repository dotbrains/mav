use crate::{
    Operator, Vim,
    motion::{self, Motion},
    object::Object,
    state::Mode,
};
use editor::{
    Anchor, Bias, Editor, EditorSnapshot, HighlightKey, SelectionEffects, ToOffset, ToPoint,
    display_map::ToDisplayPoint,
};
use gpui::{ClipboardEntry, Context, Window, actions};
use language::{Point, SelectionGoal};
use std::ops::Range;
use std::sync::Arc;

#[cfg(test)]
mod test;

actions!(
    vim,
    [
        /// Toggles replace mode.
        ToggleReplace,
        /// Undoes the last replacement.
        UndoReplace
    ]
);

pub fn register(editor: &mut Editor, cx: &mut Context<Vim>) {
    Vim::action(editor, cx, |vim, _: &ToggleReplace, window, cx| {
        vim.replacements = vec![];
        vim.start_recording(cx);
        vim.switch_mode(Mode::Replace, false, window, cx);
    });

    Vim::action(editor, cx, |vim, _: &UndoReplace, window, cx| {
        if vim.mode != Mode::Replace {
            return;
        }
        let count = Vim::take_count(cx);
        Vim::take_forced_motion(cx);
        vim.undo_replace(count, window, cx)
    });
}

impl Vim {
    pub(crate) fn multi_replace(
        &mut self,
        text: Arc<str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_editor(cx, |vim, editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                editor.set_clip_at_line_ends(false, cx);
                let map = editor.snapshot(window, cx);
                let display_selections = editor.selections.all::<Point>(&map.display_snapshot);

                // Handles all string that require manipulation, including inserts and replaces
                let edits = display_selections
                    .into_iter()
                    .map(|selection| {
                        let is_new_line = text.as_ref() == "\n";
                        let mut range = selection.range();
                        // "\n" need to be handled separately, because when a "\n" is typing,
                        // we don't do a replace, we need insert a "\n"
                        if !is_new_line {
                            range.end.column += 1;
                            range.end = map.buffer_snapshot().clip_point(range.end, Bias::Right);
                        }
                        let replace_range = map.buffer_snapshot().anchor_before(range.start)
                            ..map.buffer_snapshot().anchor_after(range.end);
                        let current_text = map
                            .buffer_snapshot()
                            .text_for_range(replace_range.clone())
                            .collect();
                        vim.replacements.push((replace_range.clone(), current_text));
                        (replace_range, text.clone())
                    })
                    .collect::<Vec<_>>();

                editor.edit_with_block_indent(edits.clone(), Vec::new(), cx);

                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_anchor_ranges(edits.iter().map(|(range, _)| range.end..range.end));
                });
                editor.set_clip_at_line_ends(true, cx);
            });
        });
    }

    fn undo_replace(
        &mut self,
        maybe_times: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_editor(cx, |vim, editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                editor.set_clip_at_line_ends(false, cx);
                let map = editor.snapshot(window, cx);
                let selections = editor.selections.all::<Point>(&map.display_snapshot);
                let mut new_selections = vec![];
                let edits: Vec<(Range<Point>, String)> = selections
                    .into_iter()
                    .filter_map(|selection| {
                        let end = selection.head();
                        let start = motion::wrapping_left(
                            &map,
                            end.to_display_point(&map),
                            maybe_times.unwrap_or(1),
                        )
                        .to_point(&map);
                        new_selections.push(
                            map.buffer_snapshot().anchor_before(start)
                                ..map.buffer_snapshot().anchor_before(start),
                        );

                        let mut undo = None;
                        let edit_range = start..end;
                        for (i, (range, inverse)) in vim.replacements.iter().rev().enumerate() {
                            if range.start.to_point(&map.buffer_snapshot()) <= edit_range.start
                                && range.end.to_point(&map.buffer_snapshot()) >= edit_range.end
                            {
                                undo = Some(inverse.clone());
                                vim.replacements.remove(vim.replacements.len() - i - 1);
                                break;
                            }
                        }
                        Some((edit_range, undo?))
                    })
                    .collect::<Vec<_>>();

                editor.edit(edits, cx);

                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_ranges(new_selections);
                });
                editor.set_clip_at_line_ends(true, cx);
            });
        });
    }

    pub fn exchange_object(
        &mut self,
        object: Object,
        around: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_recording(cx);
        self.update_editor(cx, |vim, editor, cx| {
            editor.set_clip_at_line_ends(false, cx);
            let mut selection = editor
                .selections
                .newest_display(&editor.display_snapshot(cx));
            let snapshot = editor.snapshot(window, cx);
            object.expand_selection(&snapshot, &mut selection, around, None);
            let start = snapshot
                .buffer_snapshot()
                .anchor_before(selection.start.to_point(&snapshot));
            let end = snapshot
                .buffer_snapshot()
                .anchor_before(selection.end.to_point(&snapshot));
            let new_range = start..end;
            vim.exchange_impl(new_range, editor, &snapshot, window, cx);
            editor.set_clip_at_line_ends(true, cx);
        });
    }

    pub fn exchange_visual(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.stop_recording(cx);
        self.update_editor(cx, |vim, editor, cx| {
            let selection = editor.selections.newest_anchor();
            let new_range = selection.start..selection.end;
            let snapshot = editor.snapshot(window, cx);
            vim.exchange_impl(new_range, editor, &snapshot, window, cx);
        });
        self.switch_mode(Mode::Normal, false, window, cx);
    }

    pub fn clear_exchange(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.stop_recording(cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.clear_background_highlights(HighlightKey::VimExchange, cx);
        });
        self.clear_operator(window, cx);
    }

    pub fn exchange_motion(
        &mut self,
        motion: Motion,
        times: Option<usize>,
        forced_motion: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_recording(cx);
        self.update_editor(cx, |vim, editor, cx| {
            editor.set_clip_at_line_ends(false, cx);
            let text_layout_details = editor.text_layout_details(window, cx);
            let mut selection = editor
                .selections
                .newest_display(&editor.display_snapshot(cx));
            let snapshot = editor.snapshot(window, cx);
            motion.expand_selection(
                &snapshot,
                &mut selection,
                times,
                &text_layout_details,
                forced_motion,
            );
            let start = snapshot
                .buffer_snapshot()
                .anchor_before(selection.start.to_point(&snapshot));
            let end = snapshot
                .buffer_snapshot()
                .anchor_before(selection.end.to_point(&snapshot));
            let new_range = start..end;
            vim.exchange_impl(new_range, editor, &snapshot, window, cx);
            editor.set_clip_at_line_ends(true, cx);
        });
    }

    pub fn exchange_impl(
        &self,
        new_range: Range<Anchor>,
        editor: &mut Editor,
        snapshot: &EditorSnapshot,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        if let Some((_, ranges)) = editor.clear_background_highlights(HighlightKey::VimExchange, cx)
        {
            let previous_range = ranges[0].clone();

            let new_range_start = new_range.start.to_offset(&snapshot.buffer_snapshot());
            let new_range_end = new_range.end.to_offset(&snapshot.buffer_snapshot());
            let previous_range_end = previous_range.end.to_offset(&snapshot.buffer_snapshot());
            let previous_range_start = previous_range.start.to_offset(&snapshot.buffer_snapshot());

            let text_for = |range: Range<Anchor>| {
                snapshot
                    .buffer_snapshot()
                    .text_for_range(range)
                    .collect::<String>()
            };

            let mut final_cursor_position = None;

            if previous_range_end < new_range_start || new_range_end < previous_range_start {
                let previous_text = text_for(previous_range.clone());
                let new_text = text_for(new_range.clone());
                final_cursor_position = Some(new_range.start.to_display_point(snapshot));

                editor.edit([(previous_range, new_text), (new_range, previous_text)], cx);
            } else if new_range_start <= previous_range_start && new_range_end >= previous_range_end
            {
                final_cursor_position = Some(new_range.start.to_display_point(snapshot));
                editor.edit([(new_range, text_for(previous_range))], cx);
            } else if previous_range_start <= new_range_start && previous_range_end >= new_range_end
            {
                final_cursor_position = Some(previous_range.start.to_display_point(snapshot));
                editor.edit([(previous_range, text_for(new_range))], cx);
            }

            if let Some(position) = final_cursor_position {
                editor.change_selections(Default::default(), window, cx, |s| {
                    s.move_with(&mut |_map, selection| {
                        selection.collapse_to(position, SelectionGoal::None);
                    });
                })
            }
        } else {
            let ranges = [new_range];
            editor.highlight_background(
                HighlightKey::VimExchange,
                &ranges,
                |_, theme| theme.colors().editor_document_highlight_read_background,
                cx,
            );
        }
    }

    /// Pastes the clipboard contents, replacing the same number of characters
    /// as the clipboard's contents.
    pub fn paste_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let clipboard_text = cx.read_from_clipboard().and_then(|item| {
            item.entries().iter().find_map(|entry| match entry {
                ClipboardEntry::String(text) => Some(text.text().to_string()),
                _ => None,
            })
        });

        if let Some(text) = clipboard_text {
            self.push_operator(Operator::Replace, window, cx);
            self.normal_replace(Arc::from(text), window, cx);
        }
    }
}
