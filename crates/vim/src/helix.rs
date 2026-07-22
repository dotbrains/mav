mod actions_impl;
mod boundary;
mod document_points;
mod duplicate;
mod jump_fit;
mod jump_impl;
mod jump_labels;
mod motion_helpers;
mod motion_impl;
mod object;
mod paste;
mod select;
mod surround;

use editor::display_map::{DisplayRow, DisplaySnapshot};
use editor::{
    DisplayPoint, Editor, EditorSettings, MultiBufferOffset, NavigationOverlayLabel,
    NavigationTargetOverlay, SelectionEffects, ToOffset, ToPoint, movement,
};
use gpui::actions;
use gpui::{App, Context, Font, Hsla, Pixels, TaskExt, Window, WindowTextSystem};
use language::{CharClassifier, CharKind, Point, Selection};
use multi_buffer::MultiBufferSnapshot;
use search::{BufferSearchBar, SearchOptions};
use settings::Settings;
use text::{Bias, LineEnding, SelectionGoal};
use theme::ActiveTheme as _;
use ui::px;
use workspace::searchable::{self, Direction, FilteredSearchRange};

use jump_labels::*;

use crate::motion::{self, MotionKind};
use crate::state::{HelixJumpBehaviour, HelixJumpLabel, Mode, Operator, SearchState};
use crate::{
    PushHelixSurroundAdd, PushHelixSurroundDelete, PushHelixSurroundReplace, Vim,
    motion::{Motion, right},
};
use std::ops::Range;

actions!(
    vim,
    [
        /// Yanks the current selection or character if no selection.
        HelixYank,
        /// Inserts at the beginning of the selection.
        HelixInsert,
        /// Appends at the end of the selection.
        HelixAppend,
        /// Inserts at the end of the current Helix cursor line.
        HelixInsertEndOfLine,
        /// Goes to the location of the last modification.
        HelixGotoLastModification,
        /// Select entire line or multiple lines, extending downwards.
        HelixSelectLine,
        /// Select all matches of a given pattern within the current selection.
        HelixSelectRegex,
        /// Removes all but the one selection that was created last.
        /// `Newest` can eventually be `Primary`.
        HelixKeepNewestSelection,
        /// Copies all selections below.
        HelixDuplicateBelow,
        /// Copies all selections above.
        HelixDuplicateAbove,
        /// Delete the selection and enter edit mode.
        HelixSubstitute,
        /// Delete the selection and enter edit mode, without yanking the selection.
        HelixSubstituteNoYank,
        /// Activate Helix-style word jump labels.
        HelixJumpToWord,
        /// Select the next match for the current search query.
        HelixSelectNext,
        /// Select the previous match for the current search query.
        HelixSelectPrevious,
    ]
);

pub fn register(editor: &mut Editor, cx: &mut Context<Vim>) {
    Vim::action(editor, cx, Vim::helix_select_lines);
    Vim::action(editor, cx, Vim::helix_insert);
    Vim::action(editor, cx, Vim::helix_append);
    Vim::action(editor, cx, Vim::helix_insert_end_of_line);
    Vim::action(editor, cx, Vim::helix_yank);
    Vim::action(editor, cx, Vim::helix_goto_last_modification);
    Vim::action(editor, cx, Vim::helix_paste);
    Vim::action(editor, cx, Vim::helix_select_regex);
    Vim::action(editor, cx, Vim::helix_keep_newest_selection);
    Vim::action(editor, cx, |vim, _: &HelixDuplicateBelow, window, cx| {
        let times = Vim::take_count(cx);
        vim.helix_duplicate_selections_below(times, window, cx);
    });
    Vim::action(editor, cx, |vim, _: &HelixDuplicateAbove, window, cx| {
        let times = Vim::take_count(cx);
        vim.helix_duplicate_selections_above(times, window, cx);
    });
    Vim::action(editor, cx, Vim::helix_substitute);
    Vim::action(editor, cx, Vim::helix_substitute_no_yank);
    Vim::action(editor, cx, Vim::helix_jump_to_word);
    Vim::action(editor, cx, Vim::helix_select_next);
    Vim::action(editor, cx, Vim::helix_select_previous);
    Vim::action(editor, cx, |vim, _: &PushHelixSurroundAdd, window, cx| {
        vim.clear_operator(window, cx);
        vim.push_operator(Operator::HelixSurroundAdd, window, cx);
    });
    Vim::action(
        editor,
        cx,
        |vim, _: &PushHelixSurroundReplace, window, cx| {
            vim.clear_operator(window, cx);
            vim.push_operator(
                Operator::HelixSurroundReplace {
                    replaced_char: None,
                },
                window,
                cx,
            );
        },
    );
    Vim::action(
        editor,
        cx,
        |vim, _: &PushHelixSurroundDelete, window, cx| {
            vim.clear_operator(window, cx);
            vim.push_operator(Operator::HelixSurroundDelete, window, cx);
        },
    );
}

#[cfg(test)]
#[path = "helix/tests.rs"]
mod test;
