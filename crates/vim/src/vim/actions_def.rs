use super::*;

enum HelixJumpNavigationOverlay {}

pub(crate) const HELIX_JUMP_OVERLAY_KEY: NavigationOverlayKey =
    NavigationOverlayKey::unique::<HelixJumpNavigationOverlay>();

/// Number is used to manage vim's count. Pushing a digit
/// multiplies the current value by 10 and adds the digit.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
struct Number(usize);

#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
struct SelectRegister(String);

#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
struct PushObject {
    around: bool,
}

#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
struct PushFindForward {
    before: bool,
    multiline: bool,
}

#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
struct PushFindBackward {
    after: bool,
    multiline: bool,
}

#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
/// Selects the next object.
struct PushHelixNext {
    around: bool,
}

#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
/// Selects the previous object.
struct PushHelixPrevious {
    around: bool,
}

#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
struct PushSneak {
    first_char: Option<char>,
}

#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
struct PushSneakBackward {
    first_char: Option<char>,
}

#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
struct PushAddSurrounds;

#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
struct PushChangeSurrounds {
    target: Option<Object>,
}

#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
struct PushJump {
    line: bool,
}

#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
struct PushDigraph {
    first_char: Option<char>,
}

#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
struct PushLiteral {
    prefix: Option<String>,
}

actions!(
    vim,
    [
        /// Switches to normal mode.
        SwitchToNormalMode,
        /// Switches to insert mode.
        SwitchToInsertMode,
        /// Switches to replace mode.
        SwitchToReplaceMode,
        /// Switches to visual mode.
        SwitchToVisualMode,
        /// Switches to visual line mode.
        SwitchToVisualLineMode,
        /// Switches to visual block mode.
        SwitchToVisualBlockMode,
        /// Switches to Helix-style normal mode.
        SwitchToHelixNormalMode,
        /// Clears any pending operators.
        ClearOperators,
        /// Clears the exchange register.
        ClearExchange,
        /// Inserts a tab character.
        Tab,
        /// Inserts a newline.
        Enter,
        /// Selects inner text object.
        InnerObject,
        /// Maximizes the current pane.
        MaximizePane,
        /// Resets all pane sizes to default.
        ResetPaneSizes,
        /// Resizes the pane to the right.
        ResizePaneRight,
        /// Resizes the pane to the left.
        ResizePaneLeft,
        /// Resizes the pane upward.
        ResizePaneUp,
        /// Resizes the pane downward.
        ResizePaneDown,
        /// Starts a change operation.
        PushChange,
        /// Starts a delete operation.
        PushDelete,
        /// Exchanges text regions.
        Exchange,
        /// Starts a yank operation.
        PushYank,
        /// Starts a replace operation.
        PushReplace,
        /// Deletes surrounding characters.
        PushDeleteSurrounds,
        /// Sets a mark at the current position.
        PushMark,
        /// Toggles the marks view.
        ToggleMarksView,
        /// Starts a forced motion.
        PushForcedMotion,
        /// Starts an indent operation.
        PushIndent,
        /// Starts an outdent operation.
        PushOutdent,
        /// Starts an auto-indent operation.
        PushAutoIndent,
        /// Starts a rewrap operation.
        PushRewrap,
        /// Starts a shell command operation.
        PushShellCommand,
        /// Converts to lowercase.
        PushLowercase,
        /// Converts to uppercase.
        PushUppercase,
        /// Toggles case.
        PushOppositeCase,
        /// Applies ROT13 encoding.
        PushRot13,
        /// Applies ROT47 encoding.
        PushRot47,
        /// Toggles the registers view.
        ToggleRegistersView,
        /// Selects a register.
        PushRegister,
        /// Starts recording to a register.
        PushRecordRegister,
        /// Replays a register.
        PushReplayRegister,
        /// Replaces with register contents.
        PushReplaceWithRegister,
        /// Toggles comments.
        PushToggleComments,
        /// Toggles block comments.
        PushToggleBlockComments,
        /// Selects (count) next menu item
        MenuSelectNext,
        /// Selects (count) previous menu item
        MenuSelectPrevious,
        /// Clears count or toggles project panel focus
        ToggleProjectPanelFocus,
        /// Starts a match operation.
        PushHelixMatch,
        /// Adds surrounding characters in Helix mode.
        PushHelixSurroundAdd,
        /// Replaces surrounding characters in Helix mode.
        PushHelixSurroundReplace,
        /// Deletes surrounding characters in Helix mode.
        PushHelixSurroundDelete,
    ]
);

// in the workspace namespace so it's not filtered out when vim is disabled.
actions!(
    workspace,
    [
        /// Toggles Vim mode on or off.
        ToggleVimMode,
        /// Toggles Helix mode on or off.
        ToggleHelixMode,
    ]
);
