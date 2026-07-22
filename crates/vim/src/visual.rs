use std::sync::Arc;

use collections::HashMap;
use editor::{
    Bias, DisplayPoint, Editor, MultiBufferOffset, SelectionEffects,
    display_map::{DisplaySnapshot, ToDisplayPoint},
    movement,
};
use gpui::{Context, Window, actions};
use language::{Point, Selection, SelectionGoal};
use multi_buffer::MultiBufferRow;
use search::BufferSearchBar;
use text::TransactionId;
use util::ResultExt;
use workspace::searchable::Direction;

use crate::{
    Vim,
    motion::{Motion, MotionKind, first_non_whitespace, next_line_end, start_of_line},
    object::Object,
    state::{Mark, Mode, Operator},
};

actions!(
    vim,
    [
        /// Toggles visual mode.
        ToggleVisual,
        /// Toggles visual line mode.
        ToggleVisualLine,
        /// Toggles visual block mode.
        ToggleVisualBlock,
        /// Deletes the visual selection.
        VisualDelete,
        /// Deletes entire lines in visual selection.
        VisualDeleteLine,
        /// Yanks (copies) the visual selection.
        VisualYank,
        /// Yanks entire lines in visual selection.
        VisualYankLine,
        /// Moves cursor to the other end of the selection.
        OtherEnd,
        /// Moves cursor to the other end of the selection (row-aware).
        OtherEndRowAware,
        /// Selects the next occurrence of the current selection.
        SelectNext,
        /// Selects the previous occurrence of the current selection.
        SelectPrevious,
        /// Selects the next match of the current selection.
        SelectNextMatch,
        /// Selects the previous match of the current selection.
        SelectPreviousMatch,
        /// Selects the next smaller syntax node.
        SelectSmallerSyntaxNode,
        /// Selects the next larger syntax node.
        SelectLargerSyntaxNode,
        /// Selects the next syntax node sibling.
        SelectNextSyntaxNode,
        /// Selects the previous syntax node sibling.
        SelectPreviousSyntaxNode,
        /// Restores the previous visual selection.
        RestoreVisualSelection,
        /// Inserts at the end of each line in visual selection.
        VisualInsertEndOfLine,
        /// Inserts at the first non-whitespace character of each line.
        VisualInsertFirstNonWhiteSpace,
    ]
);

pub fn register(editor: &mut Editor, cx: &mut Context<Vim>) {
    Vim::action(editor, cx, |vim, _: &ToggleVisual, window, cx| {
        vim.toggle_mode(Mode::Visual, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &ToggleVisualLine, window, cx| {
        vim.toggle_mode(Mode::VisualLine, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &ToggleVisualBlock, window, cx| {
        vim.toggle_mode(Mode::VisualBlock, window, cx)
    });
    Vim::action(editor, cx, Vim::other_end);
    Vim::action(editor, cx, Vim::other_end_row_aware);
    Vim::action(editor, cx, Vim::visual_insert_end_of_line);
    Vim::action(editor, cx, Vim::visual_insert_first_non_white_space);
    Vim::action(editor, cx, |vim, _: &VisualDelete, window, cx| {
        vim.record_current_action(cx);
        vim.visual_delete(false, window, cx);
    });
    Vim::action(editor, cx, |vim, _: &VisualDeleteLine, window, cx| {
        vim.record_current_action(cx);
        vim.visual_delete(true, window, cx);
    });
    Vim::action(editor, cx, |vim, _: &VisualYank, window, cx| {
        vim.visual_yank(false, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &VisualYankLine, window, cx| {
        vim.visual_yank(true, window, cx)
    });

    Vim::action(editor, cx, Vim::select_next);
    Vim::action(editor, cx, Vim::select_previous);
    Vim::action(editor, cx, |vim, _: &SelectNextMatch, window, cx| {
        vim.select_match(Direction::Next, window, cx);
    });
    Vim::action(editor, cx, |vim, _: &SelectPreviousMatch, window, cx| {
        vim.select_match(Direction::Prev, window, cx);
    });

    Vim::action(editor, cx, |vim, _: &SelectLargerSyntaxNode, window, cx| {
        let count = Vim::take_count(cx).unwrap_or(1);
        Vim::take_forced_motion(cx);
        for _ in 0..count {
            vim.update_editor(cx, |_, editor, cx| {
                editor.select_larger_syntax_node(&Default::default(), window, cx);
            });
        }
    });

    Vim::action(editor, cx, |vim, _: &SelectNextSyntaxNode, window, cx| {
        let count = Vim::take_count(cx).unwrap_or(1);
        Vim::take_forced_motion(cx);
        for _ in 0..count {
            vim.update_editor(cx, |_, editor, cx| {
                editor.select_next_syntax_node(&Default::default(), window, cx);
            });
        }
    });

    Vim::action(
        editor,
        cx,
        |vim, _: &SelectPreviousSyntaxNode, window, cx| {
            let count = Vim::take_count(cx).unwrap_or(1);
            Vim::take_forced_motion(cx);
            for _ in 0..count {
                vim.update_editor(cx, |_, editor, cx| {
                    editor.select_prev_syntax_node(&Default::default(), window, cx);
                });
            }
        },
    );

    Vim::action(
        editor,
        cx,
        |vim, _: &SelectSmallerSyntaxNode, window, cx| {
            let count = Vim::take_count(cx).unwrap_or(1);
            Vim::take_forced_motion(cx);
            for _ in 0..count {
                vim.update_editor(cx, |_, editor, cx| {
                    editor.select_smaller_syntax_node(&Default::default(), window, cx);
                });
            }
        },
    );

    Vim::action(editor, cx, |vim, _: &RestoreVisualSelection, window, cx| {
        let Some((stored_mode, reversed)) = vim.stored_visual_mode.take() else {
            return;
        };
        let marks = vim
            .update_editor(cx, |vim, editor, cx| {
                vim.get_mark("<", editor, window, cx)
                    .zip(vim.get_mark(">", editor, window, cx))
            })
            .flatten();
        let Some((Mark::Local(start), Mark::Local(end))) = marks else {
            return;
        };
        let ranges = start
            .iter()
            .zip(end)
            .zip(reversed)
            .map(|((start, end), reversed)| (*start, end, reversed))
            .collect::<Vec<_>>();

        if vim.mode.is_visual() {
            vim.create_visual_marks(vim.mode, window, cx);
        }

        vim.update_editor(cx, |_, editor, cx| {
            editor.set_clip_at_line_ends(false, cx);
            editor.change_selections(Default::default(), window, cx, |s| {
                let map = s.display_snapshot();
                let ranges = ranges
                    .into_iter()
                    .map(|(start, end, reversed)| {
                        let mut new_end =
                            movement::saturating_right(&map, end.to_display_point(&map));
                        let mut new_start = start.to_display_point(&map);
                        if new_start >= new_end {
                            if new_end.column() == 0 {
                                new_end = movement::right(&map, new_end)
                            } else {
                                new_start = movement::saturating_left(&map, new_end);
                            }
                        }
                        Selection {
                            id: s.new_selection_id(),
                            start: new_start.to_point(&map),
                            end: new_end.to_point(&map),
                            reversed,
                            goal: SelectionGoal::None,
                        }
                    })
                    .collect();
                s.select(ranges);
            })
        });
        vim.switch_mode(stored_mode, true, window, cx)
    });
}

mod editing;
mod motion;
mod selection;

#[cfg(test)]
mod test {
    use super::*;
    use indoc::indoc;
    use workspace::item::Item;

    use crate::{
        state::Mode,
        test::{NeovimBackedTestContext, VimTestContext},
    };

    mod block;
    mod commands;
    mod delete_yank;
    mod modes;
    mod objects;
    mod selection_matches;
    mod syntax_replace;
}
