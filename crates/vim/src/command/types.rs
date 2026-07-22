use super::*;

/// Goes to the specified line number in the editor.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = vim, no_json, no_register)]
pub struct GoToLine {
    range: CommandRange,
}

/// Yanks (copies) text based on the specified range.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = vim, no_json, no_register)]
pub struct YankCommand {
    range: CommandRange,
}

/// Executes a command with the specified range.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = vim, no_json, no_register)]
pub struct WithRange {
    restore_selection: bool,
    range: CommandRange,
    action: WrappedAction,
}

/// Executes a command with the specified count.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = vim, no_json, no_register)]
pub struct WithCount {
    count: u32,
    action: WrappedAction,
}

/// Saves the current file with optional save intent.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = vim, no_json, no_register)]
struct VimSave {
    pub range: Option<CommandRange>,
    pub save_intent: Option<SaveIntent>,
    pub filename: String,
}

/// Deletes the specified marks from the editor.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = vim, no_json, no_register)]
struct VimSplit {
    pub vertical: bool,
    pub filename: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = vim, no_json, no_register)]
enum DeleteMarks {
    Marks(String),
    AllLocal,
}

actions!(
    vim,
    [
        /// Executes a command in visual mode.
        VisualCommand,
        /// Executes a command with a count prefix.
        CountCommand,
        /// Executes a shell command.
        ShellCommand,
        /// Indicates that an argument is required for the command.
        ArgumentRequired
    ]
);

/// Opens the specified file for editing.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = vim, no_json, no_register)]
struct VimEdit {
    pub filename: String,
}

/// Pastes the specified file's contents.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = vim, no_json, no_register)]
struct VimRead {
    pub range: Option<CommandRange>,
    pub filename: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = vim, no_json, no_register)]
struct VimNorm {
    pub range: Option<CommandRange>,
    pub command: String,
    /// Places cursors at beginning of each given row.
    /// Overrides given range and current cursor.
    pub override_rows: Option<Vec<u32>>,
}

#[derive(Debug)]
struct WrappedAction(Box<dyn Action>);

impl PartialEq for WrappedAction {
    fn eq(&self, other: &Self) -> bool {
        self.0.partial_eq(&*other.0)
    }
}

impl Clone for WrappedAction {
    fn clone(&self) -> Self {
        Self(self.0.boxed_clone())
    }
}

impl Deref for WrappedAction {
    type Target = dyn Action;
    fn deref(&self) -> &dyn Action {
        &*self.0
    }
}
