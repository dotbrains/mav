mod change;
mod convert;
mod delete;
mod edit_actions;
mod increment;
mod insert_actions;
pub(crate) mod mark;
mod operator_dispatch;
pub(crate) mod paste;
mod register;
pub(crate) mod repeat;
mod scroll;
pub(crate) mod search;
pub mod substitute;
mod toggle_comments;
pub(crate) mod yank;
pub(crate) use register::register;

use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    Vim,
    indent::IndentDirection,
    motion::{self, Motion, first_non_whitespace, next_line_end, right},
    object::Object,
    state::{Mark, Mode, Operator},
    surrounds::SurroundsType,
};
use collections::BTreeSet;
use convert::ConvertTarget;
use editor::Editor;
use editor::{Anchor, SelectionEffects};
use editor::{Bias, ToPoint};
use editor::{display_map::ToDisplayPoint, movement};
use gpui::{Context, TaskExt, Window, actions};
use language::{AutoIndentMode, Point, SelectionGoal};
use log::error;
use multi_buffer::MultiBufferRow;

actions!(
    vim,
    [
        /// Inserts text after the cursor.
        InsertAfter,
        /// Inserts text before the cursor.
        InsertBefore,
        /// Inserts at the first non-whitespace character.
        InsertFirstNonWhitespace,
        /// Inserts at the end of the line.
        InsertEndOfLine,
        /// Inserts a new line above the current line.
        InsertLineAbove,
        /// Inserts a new line below the current line.
        InsertLineBelow,
        /// Inserts an empty line above without entering insert mode.
        InsertEmptyLineAbove,
        /// Inserts an empty line below without entering insert mode.
        InsertEmptyLineBelow,
        /// Inserts at the previous insert position.
        InsertAtPrevious,
        /// Joins the current line with the next line.
        JoinLines,
        /// Joins lines without adding whitespace.
        JoinLinesNoWhitespace,
        /// Deletes character to the left.
        DeleteLeft,
        /// Deletes character to the right.
        DeleteRight,
        /// Deletes using Helix-style behavior.
        HelixDelete,
        /// Collapse the current selection
        HelixCollapseSelection,
        /// Changes from cursor to end of line.
        ChangeToEndOfLine,
        /// Deletes from cursor to end of line.
        DeleteToEndOfLine,
        /// Yanks (copies) the selected text.
        Yank,
        /// Yanks the entire line.
        YankLine,
        /// Yanks from cursor to end of line.
        YankToEndOfLine,
        /// Toggles the case of selected text.
        ChangeCase,
        /// Converts selected text to uppercase.
        ConvertToUpperCase,
        /// Converts selected text to lowercase.
        ConvertToLowerCase,
        /// Applies ROT13 cipher to selected text.
        ConvertToRot13,
        /// Applies ROT47 cipher to selected text.
        ConvertToRot47,
        /// Toggles comments for selected lines.
        ToggleComments,
        /// Toggles block comments for selected lines.
        ToggleBlockComments,
        /// Shows the current location in the file.
        ShowLocation,
        /// Undoes the last change.
        Undo,
        /// Redoes the last undone change.
        Redo,
        /// Undoes all changes to the most recently changed line.
        UndoLastLine,
        /// Go to tab page (with count support).
        GoToTab,
        /// Go to previous tab page (with count support).
        GoToPreviousTab,
        /// Goes to the previous reference to the symbol under the cursor.
        GoToPreviousReference,
        /// Goes to the next reference to the symbol under the cursor.
        GoToNextReference,
    ]
);

#[cfg(test)]
mod test_basic_motions;
#[cfg(test)]
mod test_find_replace;
#[cfg(test)]
mod test_insert_delete;
#[cfg(test)]
mod test_yank_undo_tabs;
