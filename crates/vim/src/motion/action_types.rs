use gpui::Action;
use schemars::JsonSchema;
use serde::Deserialize;

/// Moves to the start of the next word.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct NextWordStart {
    #[serde(default)]
    pub(super) ignore_punctuation: bool,
}

/// Moves to the end of the next word.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct NextWordEnd {
    #[serde(default)]
    pub(super) ignore_punctuation: bool,
}

/// Moves to the start of the previous word.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct PreviousWordStart {
    #[serde(default)]
    pub(super) ignore_punctuation: bool,
}

/// Moves to the end of the previous word.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct PreviousWordEnd {
    #[serde(default)]
    pub(super) ignore_punctuation: bool,
}

/// Moves to the start of the next subword.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(crate) struct NextSubwordStart {
    #[serde(default)]
    pub(crate) ignore_punctuation: bool,
}

/// Moves to the end of the next subword.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(crate) struct NextSubwordEnd {
    #[serde(default)]
    pub(crate) ignore_punctuation: bool,
}

/// Moves to the start of the previous subword.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreviousSubwordStart {
    #[serde(default)]
    pub(crate) ignore_punctuation: bool,
}

/// Moves to the end of the previous subword.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreviousSubwordEnd {
    #[serde(default)]
    pub(crate) ignore_punctuation: bool,
}

/// Moves cursor up by the specified number of lines.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(crate) struct Up {
    #[serde(default)]
    pub(crate) display_lines: bool,
}

/// Moves cursor down by the specified number of lines.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(crate) struct Down {
    #[serde(default)]
    pub(crate) display_lines: bool,
}

/// Moves to the first non-whitespace character on the current line.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct FirstNonWhitespace {
    #[serde(default)]
    pub(super) display_lines: bool,
}

/// Moves to the matching bracket or delimiter.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct Matching {
    #[serde(default)]
    /// Whether to include quote characters (`'`, `"`, `` ` ``) when searching
    /// for matching pairs. When `false`, only brackets and parentheses are
    /// matched, which aligns with Neovim's default `%` behavior.
    pub(super) match_quotes: bool,
}

/// Moves to the end of the current line.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct EndOfLine {
    #[serde(default)]
    pub(super) display_lines: bool,
}

/// Moves to the start of the current line.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub struct StartOfLine {
    #[serde(default)]
    pub(crate) display_lines: bool,
}

/// Moves to the middle of the current line.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct MiddleOfLine {
    #[serde(default)]
    pub(super) display_lines: bool,
}

/// Finds the next unmatched bracket or delimiter.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct UnmatchedForward {
    #[serde(default)]
    pub(super) char: char,
}

/// Finds the previous unmatched bracket or delimiter.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
pub(super) struct UnmatchedBackward {
    #[serde(default)]
    pub(super) char: char,
}
