use editor::{
    Anchor, Bias, BufferOffset, DisplayPoint, Editor, MultiBufferOffset, RowExt, ToOffset,
    ToPoint as _,
    display_map::{DisplayRow, DisplaySnapshot, FoldPoint, ToDisplayPoint},
    movement::{
        self, FindRange, TextLayoutDetails, find_boundary, find_preceding_boundary_display_point,
    },
};
use gpui::{Context, Window, actions, px};
use language::{CharKind, Point, Selection, SelectionGoal, TextObject, TreeSitterOptions};
use multi_buffer::MultiBufferRow;
use std::{f64, ops::Range};

use workspace::searchable::Direction;

use crate::{
    Vim,
    normal::mark,
    state::{Mode, Operator},
    surrounds::SurroundsType,
};

mod action_types;
mod matching;
pub use action_types::StartOfLine;
use action_types::{
    Down, NextSubwordEnd, NextSubwordStart, PreviousSubwordEnd, PreviousSubwordStart, Up,
};
use action_types::{
    EndOfLine, FirstNonWhitespace, Matching, MiddleOfLine, NextWordEnd, NextWordStart,
    PreviousWordEnd, PreviousWordStart, UnmatchedBackward, UnmatchedForward,
};
use matching::matching;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MotionKind {
    Linewise,
    Exclusive,
    Inclusive,
}

impl MotionKind {
    pub(crate) fn for_mode(mode: Mode) -> Self {
        match mode {
            Mode::VisualLine => MotionKind::Linewise,
            _ => MotionKind::Exclusive,
        }
    }

    pub(crate) fn linewise(&self) -> bool {
        matches!(self, MotionKind::Linewise)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    WrappingLeft,
    Down {
        display_lines: bool,
    },
    Up {
        display_lines: bool,
    },
    Right,
    WrappingRight,
    NextWordStart {
        ignore_punctuation: bool,
    },
    NextWordEnd {
        ignore_punctuation: bool,
    },
    PreviousWordStart {
        ignore_punctuation: bool,
    },
    PreviousWordEnd {
        ignore_punctuation: bool,
    },
    NextSubwordStart {
        ignore_punctuation: bool,
    },
    NextSubwordEnd {
        ignore_punctuation: bool,
    },
    PreviousSubwordStart {
        ignore_punctuation: bool,
    },
    PreviousSubwordEnd {
        ignore_punctuation: bool,
    },
    FirstNonWhitespace {
        display_lines: bool,
    },
    CurrentLine,
    StartOfLine {
        display_lines: bool,
    },
    MiddleOfLine {
        display_lines: bool,
    },
    EndOfLine {
        display_lines: bool,
    },
    SentenceBackward,
    SentenceForward,
    StartOfParagraph,
    EndOfParagraph,
    StartOfDocument,
    EndOfDocument,
    Matching {
        match_quotes: bool,
    },
    GoToPercentage,
    UnmatchedForward {
        char: char,
    },
    UnmatchedBackward {
        char: char,
    },
    FindForward {
        before: bool,
        char: char,
        mode: FindRange,
        smartcase: bool,
    },
    FindBackward {
        after: bool,
        char: char,
        mode: FindRange,
        smartcase: bool,
    },
    Sneak {
        first_char: char,
        second_char: char,
        smartcase: bool,
    },
    SneakBackward {
        first_char: char,
        second_char: char,
        smartcase: bool,
    },
    RepeatFind {
        last_find: Box<Motion>,
    },
    RepeatFindReversed {
        last_find: Box<Motion>,
    },
    NextLineStart,
    PreviousLineStart,
    StartOfLineDownward,
    EndOfLineDownward,
    GoToColumn,
    WindowTop,
    WindowMiddle,
    WindowBottom,
    NextSectionStart,
    NextSectionEnd,
    PreviousSectionStart,
    PreviousSectionEnd,
    NextMethodStart,
    NextMethodEnd,
    PreviousMethodStart,
    PreviousMethodEnd,
    NextComment,
    PreviousComment,
    PreviousLesserIndent,
    PreviousGreaterIndent,
    PreviousSameIndent,
    NextLesserIndent,
    NextGreaterIndent,
    NextSameIndent,

    // we don't have a good way to run a search synchronously, so
    // we handle search motions by running the search async and then
    // calling back into motion with this
    MavSearchResult {
        prior_selections: Vec<Range<Anchor>>,
        new_selections: Vec<Range<Anchor>>,
    },
    Jump {
        anchor: Anchor,
        line: bool,
    },
}

#[derive(Clone, Copy)]
enum IndentType {
    Lesser,
    Greater,
    Same,
}

actions!(
    vim,
    [
        /// Moves cursor left one character.
        Left,
        /// Moves cursor left one character, wrapping to previous line.
        #[action(deprecated_aliases = ["vim::Backspace"])]
        WrappingLeft,
        /// Moves cursor right one character.
        Right,
        /// Moves cursor right one character, wrapping to next line.
        #[action(deprecated_aliases = ["vim::Space"])]
        WrappingRight,
        /// Selects the current line.
        CurrentLine,
        /// Moves to the start of the next sentence.
        SentenceForward,
        /// Moves to the start of the previous sentence.
        SentenceBackward,
        /// Moves to the start of the paragraph.
        StartOfParagraph,
        /// Moves to the end of the paragraph.
        EndOfParagraph,
        /// Moves to the start of the document.
        StartOfDocument,
        /// Moves to the end of the document.
        EndOfDocument,
        /// Goes to a percentage position in the file.
        GoToPercentage,
        /// Moves to the start of the next line.
        NextLineStart,
        /// Moves to the start of the previous line.
        PreviousLineStart,
        /// Moves to the start of a line downward.
        StartOfLineDownward,
        /// Moves to the end of a line downward.
        EndOfLineDownward,
        /// Goes to a specific column number.
        GoToColumn,
        /// Repeats the last character find.
        RepeatFind,
        /// Repeats the last character find in reverse.
        RepeatFindReversed,
        /// Moves to the top of the window.
        WindowTop,
        /// Moves to the middle of the window.
        WindowMiddle,
        /// Moves to the bottom of the window.
        WindowBottom,
        /// Moves to the start of the next section.
        NextSectionStart,
        /// Moves to the end of the next section.
        NextSectionEnd,
        /// Moves to the start of the previous section.
        PreviousSectionStart,
        /// Moves to the end of the previous section.
        PreviousSectionEnd,
        /// Moves to the start of the next method.
        NextMethodStart,
        /// Moves to the end of the next method.
        NextMethodEnd,
        /// Moves to the start of the previous method.
        PreviousMethodStart,
        /// Moves to the end of the previous method.
        PreviousMethodEnd,
        /// Moves to the next comment.
        NextComment,
        /// Moves to the previous comment.
        PreviousComment,
        /// Moves to the previous line with lesser indentation.
        PreviousLesserIndent,
        /// Moves to the previous line with greater indentation.
        PreviousGreaterIndent,
        /// Moves to the previous line with the same indentation.
        PreviousSameIndent,
        /// Moves to the next line with lesser indentation.
        NextLesserIndent,
        /// Moves to the next line with greater indentation.
        NextGreaterIndent,
        /// Moves to the next line with the same indentation.
        NextSameIndent,
    ]
);

#[path = "motion/char_motion.rs"]
mod char_motion;
#[path = "motion/code_motion.rs"]
mod code_motion;
#[path = "motion/document_motion.rs"]
mod document_motion;
#[path = "motion/find_motion.rs"]
mod find_motion;
#[path = "motion/indent_motion.rs"]
mod indent_motion;
#[path = "motion/line_motion.rs"]
mod line_motion;
#[path = "motion/motion_kind.rs"]
mod motion_kind;
#[path = "motion/motion_move_point.rs"]
mod motion_move_point;
#[path = "motion/motion_range.rs"]
mod motion_range;
#[path = "motion/register.rs"]
mod register_actions;
#[path = "motion/sentence_paragraph.rs"]
mod sentence_paragraph;
#[cfg(test)]
mod test;
#[path = "motion/vim_dispatch.rs"]
mod vim_dispatch;
#[path = "motion/window_motion.rs"]
mod window_motion;
#[path = "motion/word_motion.rs"]
mod word_motion;
