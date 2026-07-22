use editor::{Editor, EditorSettings};
use gpui::{Action, Context, TaskExt, Window, actions};
use language::Point;
use schemars::JsonSchema;
use search::{BufferSearchBar, SearchOptions, buffer_search};
use serde::Deserialize;
use settings::Settings;
use std::{iter::Peekable, str::Chars};
use util::serde::default_true;
use workspace::{notifications::NotifyResultExt, searchable::Direction};

use crate::{
    Vim, VimSettings,
    command::CommandRange,
    motion::Motion,
    state::{Mode, SearchState},
};

/// Moves to the next search match.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(crate) struct MoveToNext {
    #[serde(default = "default_true")]
    case_sensitive: bool,
    #[serde(default)]
    partial_word: bool,
    #[serde(default = "default_true")]
    regex: bool,
}

/// Moves to the previous search match.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(crate) struct MoveToPrevious {
    #[serde(default = "default_true")]
    case_sensitive: bool,
    #[serde(default)]
    partial_word: bool,
    #[serde(default = "default_true")]
    regex: bool,
}

/// Searches for the word under the cursor without moving.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchUnderCursor {
    #[serde(default = "default_true")]
    case_sensitive: bool,
    #[serde(default)]
    partial_word: bool,
    #[serde(default = "default_true")]
    regex: bool,
}

/// Searches for the word under the cursor without moving (backwards).
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchUnderCursorPrevious {
    #[serde(default = "default_true")]
    case_sensitive: bool,
    #[serde(default)]
    partial_word: bool,
    #[serde(default = "default_true")]
    regex: bool,
}

/// Initiates a search operation with the specified parameters.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(crate) struct Search {
    #[serde(default)]
    backwards: bool,
    #[serde(default = "default_true")]
    regex: bool,
}

/// Executes a find command to search for patterns in the buffer.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub struct FindCommand {
    pub query: String,
    pub backwards: bool,
}

/// Executes a search and replace command within the specified range.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = vim, no_json, no_register)]
pub struct ReplaceCommand {
    pub(crate) range: CommandRange,
    pub(crate) replacement: Replacement,
}

mod commands;
mod movement;
mod replacement;
pub use replacement::Replacement;

#[cfg(test)]
mod test_dismiss_settings;
#[cfg(test)]
mod test_motion;
#[cfg(test)]
mod test_replace;

actions!(
    vim,
    [
        /// Submits the current search query.
        SearchSubmit,
        /// Moves to the next search match.
        MoveToNextMatch,
        /// Moves to the previous search match.
        MoveToPreviousMatch
    ]
);

pub(crate) fn register(editor: &mut Editor, cx: &mut Context<Vim>) {
    Vim::action(editor, cx, Vim::move_to_next);
    Vim::action(editor, cx, Vim::move_to_previous);
    Vim::action(editor, cx, Vim::search_under_cursor);
    Vim::action(editor, cx, Vim::search_under_cursor_previous);
    Vim::action(editor, cx, Vim::move_to_next_match);
    Vim::action(editor, cx, Vim::move_to_previous_match);
    Vim::action(editor, cx, Vim::search);
    Vim::action(editor, cx, Vim::search_deploy);
    Vim::action(editor, cx, Vim::find_command);
    Vim::action(editor, cx, Vim::replace_command);
}
