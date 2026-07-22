use super::*;

impl Motion {
    fn default_kind(&self) -> MotionKind {
        use Motion::*;
        match self {
            Down { .. }
            | Up { .. }
            | StartOfDocument
            | EndOfDocument
            | CurrentLine
            | NextLineStart
            | PreviousLineStart
            | StartOfLineDownward
            | WindowTop
            | WindowMiddle
            | WindowBottom
            | NextSectionStart
            | NextSectionEnd
            | PreviousSectionStart
            | PreviousSectionEnd
            | NextMethodStart
            | NextMethodEnd
            | PreviousMethodStart
            | PreviousMethodEnd
            | NextComment
            | PreviousComment
            | PreviousLesserIndent
            | PreviousGreaterIndent
            | PreviousSameIndent
            | NextLesserIndent
            | NextGreaterIndent
            | NextSameIndent
            | GoToPercentage
            | Jump { line: true, .. } => MotionKind::Linewise,
            EndOfLine { .. }
            | EndOfLineDownward
            | Matching { .. }
            | FindForward { .. }
            | NextWordEnd { .. }
            | PreviousWordEnd { .. }
            | NextSubwordEnd { .. }
            | PreviousSubwordEnd { .. } => MotionKind::Inclusive,
            Left
            | WrappingLeft
            | Right
            | WrappingRight
            | StartOfLine { .. }
            | StartOfParagraph
            | EndOfParagraph
            | SentenceBackward
            | SentenceForward
            | GoToColumn
            | MiddleOfLine { .. }
            | UnmatchedForward { .. }
            | UnmatchedBackward { .. }
            | NextWordStart { .. }
            | PreviousWordStart { .. }
            | NextSubwordStart { .. }
            | PreviousSubwordStart { .. }
            | FirstNonWhitespace { .. }
            | FindBackward { .. }
            | Sneak { .. }
            | SneakBackward { .. }
            | Jump { .. }
            | MavSearchResult { .. } => MotionKind::Exclusive,
            RepeatFind { last_find: motion } | RepeatFindReversed { last_find: motion } => {
                motion.default_kind()
            }
        }
    }

    fn skip_exclusive_special_case(&self) -> bool {
        matches!(self, Motion::WrappingLeft | Motion::WrappingRight)
    }

    pub(crate) fn push_to_jump_list(&self) -> bool {
        use Motion::*;
        match self {
            CurrentLine
            | Down { .. }
            | EndOfLine { .. }
            | EndOfLineDownward
            | FindBackward { .. }
            | FindForward { .. }
            | FirstNonWhitespace { .. }
            | GoToColumn
            | Left
            | MiddleOfLine { .. }
            | NextLineStart
            | NextSubwordEnd { .. }
            | NextSubwordStart { .. }
            | NextWordEnd { .. }
            | NextWordStart { .. }
            | PreviousLineStart
            | PreviousSubwordEnd { .. }
            | PreviousSubwordStart { .. }
            | PreviousWordEnd { .. }
            | PreviousWordStart { .. }
            | RepeatFind { .. }
            | RepeatFindReversed { .. }
            | Right
            | StartOfLine { .. }
            | StartOfLineDownward
            | Up { .. }
            | WrappingLeft
            | WrappingRight => false,
            EndOfDocument
            | EndOfParagraph
            | GoToPercentage
            | Jump { .. }
            | Matching { .. }
            | NextComment
            | NextGreaterIndent
            | NextLesserIndent
            | NextMethodEnd
            | NextMethodStart
            | NextSameIndent
            | NextSectionEnd
            | NextSectionStart
            | PreviousComment
            | PreviousGreaterIndent
            | PreviousLesserIndent
            | PreviousMethodEnd
            | PreviousMethodStart
            | PreviousSameIndent
            | PreviousSectionEnd
            | PreviousSectionStart
            | SentenceBackward
            | SentenceForward
            | Sneak { .. }
            | SneakBackward { .. }
            | StartOfDocument
            | StartOfParagraph
            | UnmatchedBackward { .. }
            | UnmatchedForward { .. }
            | WindowBottom
            | WindowMiddle
            | WindowTop
            | MavSearchResult { .. } => true,
        }
    }

    pub fn infallible(&self) -> bool {
        use Motion::*;
        match self {
            StartOfDocument | EndOfDocument | CurrentLine | EndOfLine { .. } => true,
            Down { .. }
            | Up { .. }
            | MiddleOfLine { .. }
            | Matching { .. }
            | UnmatchedForward { .. }
            | UnmatchedBackward { .. }
            | FindForward { .. }
            | RepeatFind { .. }
            | Left
            | WrappingLeft
            | Right
            | WrappingRight
            | StartOfLine { .. }
            | StartOfParagraph
            | EndOfParagraph
            | SentenceBackward
            | SentenceForward
            | StartOfLineDownward
            | EndOfLineDownward
            | GoToColumn
            | GoToPercentage
            | NextWordStart { .. }
            | NextWordEnd { .. }
            | PreviousWordStart { .. }
            | PreviousWordEnd { .. }
            | NextSubwordStart { .. }
            | NextSubwordEnd { .. }
            | PreviousSubwordStart { .. }
            | PreviousSubwordEnd { .. }
            | FirstNonWhitespace { .. }
            | FindBackward { .. }
            | Sneak { .. }
            | SneakBackward { .. }
            | RepeatFindReversed { .. }
            | WindowTop
            | WindowMiddle
            | WindowBottom
            | NextLineStart
            | PreviousLineStart
            | MavSearchResult { .. }
            | NextSectionStart
            | NextSectionEnd
            | PreviousSectionStart
            | PreviousSectionEnd
            | NextMethodStart
            | NextMethodEnd
            | PreviousMethodStart
            | PreviousMethodEnd
            | NextComment
            | PreviousComment
            | PreviousLesserIndent
            | PreviousGreaterIndent
            | PreviousSameIndent
            | NextLesserIndent
            | NextGreaterIndent
            | NextSameIndent
            | Jump { .. } => false,
        }
    }
}
