use super::*;

pub fn register(editor: &mut Editor, cx: &mut Context<Vim>) {
    Vim::action(editor, cx, |vim, _: &Left, window, cx| {
        vim.motion(Motion::Left, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &WrappingLeft, window, cx| {
        vim.motion(Motion::WrappingLeft, window, cx)
    });
    Vim::action(editor, cx, |vim, action: &Down, window, cx| {
        vim.motion(
            Motion::Down {
                display_lines: action.display_lines,
            },
            window,
            cx,
        )
    });
    Vim::action(editor, cx, |vim, action: &Up, window, cx| {
        vim.motion(
            Motion::Up {
                display_lines: action.display_lines,
            },
            window,
            cx,
        )
    });
    Vim::action(editor, cx, |vim, _: &Right, window, cx| {
        vim.motion(Motion::Right, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &WrappingRight, window, cx| {
        vim.motion(Motion::WrappingRight, window, cx)
    });
    Vim::action(
        editor,
        cx,
        |vim, action: &FirstNonWhitespace, window, cx| {
            vim.motion(
                Motion::FirstNonWhitespace {
                    display_lines: action.display_lines,
                },
                window,
                cx,
            )
        },
    );
    Vim::action(editor, cx, |vim, action: &StartOfLine, window, cx| {
        vim.motion(
            Motion::StartOfLine {
                display_lines: action.display_lines,
            },
            window,
            cx,
        )
    });
    Vim::action(editor, cx, |vim, action: &MiddleOfLine, window, cx| {
        vim.motion(
            Motion::MiddleOfLine {
                display_lines: action.display_lines,
            },
            window,
            cx,
        )
    });
    Vim::action(editor, cx, |vim, action: &EndOfLine, window, cx| {
        vim.motion(
            Motion::EndOfLine {
                display_lines: action.display_lines,
            },
            window,
            cx,
        )
    });
    Vim::action(editor, cx, |vim, _: &CurrentLine, window, cx| {
        vim.motion(Motion::CurrentLine, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &StartOfParagraph, window, cx| {
        vim.motion(Motion::StartOfParagraph, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &EndOfParagraph, window, cx| {
        vim.motion(Motion::EndOfParagraph, window, cx)
    });

    Vim::action(editor, cx, |vim, _: &SentenceForward, window, cx| {
        vim.motion(Motion::SentenceForward, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &SentenceBackward, window, cx| {
        vim.motion(Motion::SentenceBackward, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &StartOfDocument, window, cx| {
        vim.motion(Motion::StartOfDocument, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &EndOfDocument, window, cx| {
        vim.motion(Motion::EndOfDocument, window, cx)
    });
    Vim::action(
        editor,
        cx,
        |vim, &Matching { match_quotes }: &Matching, window, cx| {
            vim.motion(Motion::Matching { match_quotes }, window, cx)
        },
    );

    Vim::action(editor, cx, |vim, _: &GoToPercentage, window, cx| {
        vim.motion(Motion::GoToPercentage, window, cx)
    });
    Vim::action(
        editor,
        cx,
        |vim, &UnmatchedForward { char }: &UnmatchedForward, window, cx| {
            vim.motion(Motion::UnmatchedForward { char }, window, cx)
        },
    );
    Vim::action(
        editor,
        cx,
        |vim, &UnmatchedBackward { char }: &UnmatchedBackward, window, cx| {
            vim.motion(Motion::UnmatchedBackward { char }, window, cx)
        },
    );
    Vim::action(
        editor,
        cx,
        |vim, &NextWordStart { ignore_punctuation }: &NextWordStart, window, cx| {
            vim.motion(Motion::NextWordStart { ignore_punctuation }, window, cx)
        },
    );
    Vim::action(
        editor,
        cx,
        |vim, &NextWordEnd { ignore_punctuation }: &NextWordEnd, window, cx| {
            vim.motion(Motion::NextWordEnd { ignore_punctuation }, window, cx)
        },
    );
    Vim::action(
        editor,
        cx,
        |vim, &PreviousWordStart { ignore_punctuation }: &PreviousWordStart, window, cx| {
            vim.motion(Motion::PreviousWordStart { ignore_punctuation }, window, cx)
        },
    );
    Vim::action(
        editor,
        cx,
        |vim, &PreviousWordEnd { ignore_punctuation }, window, cx| {
            vim.motion(Motion::PreviousWordEnd { ignore_punctuation }, window, cx)
        },
    );
    Vim::action(
        editor,
        cx,
        |vim, &NextSubwordStart { ignore_punctuation }: &NextSubwordStart, window, cx| {
            vim.motion(Motion::NextSubwordStart { ignore_punctuation }, window, cx)
        },
    );
    Vim::action(
        editor,
        cx,
        |vim, &NextSubwordEnd { ignore_punctuation }: &NextSubwordEnd, window, cx| {
            vim.motion(Motion::NextSubwordEnd { ignore_punctuation }, window, cx)
        },
    );
    Vim::action(
        editor,
        cx,
        |vim, &PreviousSubwordStart { ignore_punctuation }: &PreviousSubwordStart, window, cx| {
            vim.motion(
                Motion::PreviousSubwordStart { ignore_punctuation },
                window,
                cx,
            )
        },
    );
    Vim::action(
        editor,
        cx,
        |vim, &PreviousSubwordEnd { ignore_punctuation }, window, cx| {
            vim.motion(
                Motion::PreviousSubwordEnd { ignore_punctuation },
                window,
                cx,
            )
        },
    );
    Vim::action(editor, cx, |vim, &NextLineStart, window, cx| {
        vim.motion(Motion::NextLineStart, window, cx)
    });
    Vim::action(editor, cx, |vim, &PreviousLineStart, window, cx| {
        vim.motion(Motion::PreviousLineStart, window, cx)
    });
    Vim::action(editor, cx, |vim, &StartOfLineDownward, window, cx| {
        vim.motion(Motion::StartOfLineDownward, window, cx)
    });
    Vim::action(editor, cx, |vim, &EndOfLineDownward, window, cx| {
        vim.motion(Motion::EndOfLineDownward, window, cx)
    });
    Vim::action(editor, cx, |vim, &GoToColumn, window, cx| {
        vim.motion(Motion::GoToColumn, window, cx)
    });

    Vim::action(editor, cx, |vim, _: &RepeatFind, window, cx| {
        if let Some(last_find) = Vim::globals(cx).last_find.clone().map(Box::new) {
            vim.motion(Motion::RepeatFind { last_find }, window, cx);
        }
    });

    Vim::action(editor, cx, |vim, _: &RepeatFindReversed, window, cx| {
        if let Some(last_find) = Vim::globals(cx).last_find.clone().map(Box::new) {
            vim.motion(Motion::RepeatFindReversed { last_find }, window, cx);
        }
    });
    Vim::action(editor, cx, |vim, &WindowTop, window, cx| {
        vim.motion(Motion::WindowTop, window, cx)
    });
    Vim::action(editor, cx, |vim, &WindowMiddle, window, cx| {
        vim.motion(Motion::WindowMiddle, window, cx)
    });
    Vim::action(editor, cx, |vim, &WindowBottom, window, cx| {
        vim.motion(Motion::WindowBottom, window, cx)
    });

    Vim::action(editor, cx, |vim, &PreviousSectionStart, window, cx| {
        vim.motion(Motion::PreviousSectionStart, window, cx)
    });
    Vim::action(editor, cx, |vim, &NextSectionStart, window, cx| {
        vim.motion(Motion::NextSectionStart, window, cx)
    });
    Vim::action(editor, cx, |vim, &PreviousSectionEnd, window, cx| {
        vim.motion(Motion::PreviousSectionEnd, window, cx)
    });
    Vim::action(editor, cx, |vim, &NextSectionEnd, window, cx| {
        vim.motion(Motion::NextSectionEnd, window, cx)
    });
    Vim::action(editor, cx, |vim, &PreviousMethodStart, window, cx| {
        vim.motion(Motion::PreviousMethodStart, window, cx)
    });
    Vim::action(editor, cx, |vim, &NextMethodStart, window, cx| {
        vim.motion(Motion::NextMethodStart, window, cx)
    });
    Vim::action(editor, cx, |vim, &PreviousMethodEnd, window, cx| {
        vim.motion(Motion::PreviousMethodEnd, window, cx)
    });
    Vim::action(editor, cx, |vim, &NextMethodEnd, window, cx| {
        vim.motion(Motion::NextMethodEnd, window, cx)
    });
    Vim::action(editor, cx, |vim, &NextComment, window, cx| {
        vim.motion(Motion::NextComment, window, cx)
    });
    Vim::action(editor, cx, |vim, &PreviousComment, window, cx| {
        vim.motion(Motion::PreviousComment, window, cx)
    });
    Vim::action(editor, cx, |vim, &PreviousLesserIndent, window, cx| {
        vim.motion(Motion::PreviousLesserIndent, window, cx)
    });
    Vim::action(editor, cx, |vim, &PreviousGreaterIndent, window, cx| {
        vim.motion(Motion::PreviousGreaterIndent, window, cx)
    });
    Vim::action(editor, cx, |vim, &PreviousSameIndent, window, cx| {
        vim.motion(Motion::PreviousSameIndent, window, cx)
    });
    Vim::action(editor, cx, |vim, &NextLesserIndent, window, cx| {
        vim.motion(Motion::NextLesserIndent, window, cx)
    });
    Vim::action(editor, cx, |vim, &NextGreaterIndent, window, cx| {
        vim.motion(Motion::NextGreaterIndent, window, cx)
    });
    Vim::action(editor, cx, |vim, &NextSameIndent, window, cx| {
        vim.motion(Motion::NextSameIndent, window, cx)
    });
}
