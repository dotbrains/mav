use super::*;

#[derive(Clone, Copy, PartialEq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SaveIntent {
    /// write all files (even if unchanged)
    /// prompt before overwriting on-disk changes
    Save,
    /// same as Save, but always formats regardless of the format_on_save setting
    FormatAndSave,
    /// same as Save, but without auto formatting
    SaveWithoutFormat,
    /// write any files that have local changes
    /// prompt before overwriting on-disk changes
    SaveAll,
    /// always prompt for a new path
    SaveAs,
    /// prompt "you have unsaved changes" before writing
    Close,
    /// write all dirty files, don't prompt on conflict
    Overwrite,
    /// skip all save-related behavior
    Skip,
}

/// Activates a specific item in the pane by its index.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
pub struct ActivateItem(pub usize);

/// Closes the currently active item in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseActiveItem {
    #[serde(default)]
    pub save_intent: Option<SaveIntent>,
    #[serde(default)]
    pub close_pinned: bool,
}

/// Closes all inactive items in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
#[action(deprecated_aliases = ["pane::CloseInactiveItems"])]
pub struct CloseOtherItems {
    #[serde(default)]
    pub save_intent: Option<SaveIntent>,
    #[serde(default)]
    pub close_pinned: bool,
}

/// Closes all multibuffers in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseMultibufferItems {
    #[serde(default)]
    pub save_intent: Option<SaveIntent>,
    #[serde(default)]
    pub close_pinned: bool,
}

/// Closes all items in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseAllItems {
    #[serde(default)]
    pub save_intent: Option<SaveIntent>,
    #[serde(default)]
    pub close_pinned: bool,
}

/// Closes all items that have no unsaved changes.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseCleanItems {
    #[serde(default)]
    pub close_pinned: bool,
}

/// Closes all items to the right of the current item.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseItemsToTheRight {
    #[serde(default)]
    pub close_pinned: bool,
}

/// Closes all items to the left of the current item.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseItemsToTheLeft {
    #[serde(default)]
    pub close_pinned: bool,
}

/// Reveals the current item in the project panel.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct RevealInProjectPanel {
    #[serde(skip)]
    pub entry_id: Option<u64>,
}

/// Opens the search interface with the specified configuration.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct DeploySearch {
    #[serde(default)]
    pub replace_enabled: bool,
    #[serde(default)]
    pub included_files: Option<String>,
    #[serde(default)]
    pub excluded_files: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub regex: Option<bool>,
    #[serde(default)]
    pub case_sensitive: Option<bool>,
    #[serde(default)]
    pub whole_word: Option<bool>,
    #[serde(default)]
    pub include_ignored: Option<bool>,
}

#[derive(Clone, Copy, PartialEq, Debug, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub enum SplitMode {
    /// Clone the current pane.
    #[default]
    ClonePane,
    /// Create an empty new pane.
    EmptyPane,
    /// Move the item into a new pane. This will map to nop if only one pane exists.
    MovePane,
}

macro_rules! split_structs {
    ($($name:ident => $doc:literal),* $(,)?) => {
        $(
            #[doc = $doc]
            #[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
            #[action(namespace = pane)]
            #[serde(deny_unknown_fields, default)]
            pub struct $name {
                pub mode: SplitMode,
            }
        )*
    };
}

split_structs!(
    SplitLeft => "Splits the pane to the left.",
    SplitRight => "Splits the pane to the right.",
    SplitUp => "Splits the pane upward.",
    SplitDown => "Splits the pane downward.",
    SplitHorizontal => "Splits the pane horizontally.",
    SplitVertical => "Splits the pane vertically."
);

/// Activates the previous item in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields, default)]
pub struct ActivatePreviousItem {
    /// Whether to wrap from the first item to the last item.
    #[serde(default = "default_true")]
    pub wrap_around: bool,
}

impl Default for ActivatePreviousItem {
    fn default() -> Self {
        Self { wrap_around: true }
    }
}

/// Activates the next item in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields, default)]
pub struct ActivateNextItem {
    /// Whether to wrap from the last item to the first item.
    #[serde(default = "default_true")]
    pub wrap_around: bool,
}

impl Default for ActivateNextItem {
    fn default() -> Self {
        Self { wrap_around: true }
    }
}

actions!(
    pane,
    [
        /// Activates the last item in the pane.
        ActivateLastItem,
        /// Switches to the alternate file.
        AlternateFile,
        /// Navigates back in history.
        GoBack,
        /// Navigates forward in history.
        GoForward,
        /// Navigates back in the tag stack.
        GoToOlderTag,
        /// Navigates forward in the tag stack.
        GoToNewerTag,
        /// Joins this pane into the next pane.
        JoinIntoNext,
        /// Joins all panes into one.
        JoinAll,
        /// Reopens the most recently closed item.
        ReopenClosedItem,
        /// Splits the pane to the left, moving the current item.
        SplitAndMoveLeft,
        /// Splits the pane upward, moving the current item.
        SplitAndMoveUp,
        /// Splits the pane to the right, moving the current item.
        SplitAndMoveRight,
        /// Splits the pane downward, moving the current item.
        SplitAndMoveDown,
        /// Swaps the current item with the one to the left.
        SwapItemLeft,
        /// Swaps the current item with the one to the right.
        SwapItemRight,
        /// Toggles preview mode for the current tab.
        TogglePreviewTab,
        /// Toggles pin status for the current tab.
        TogglePinTab,
        /// Unpins all tabs in the pane.
        UnpinAllTabs,
    ]
);
