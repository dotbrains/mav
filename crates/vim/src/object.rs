use std::ops::Range;

use crate::{
    Vim,
    motion::{is_subword_end, is_subword_start, right},
    state::{Mode, Operator},
    surrounds::{BRACKET_PAIRS, QUOTE_PAIRS, SurroundPair},
};
use editor::{
    Bias, BufferOffset, DisplayPoint, Editor, MultiBufferOffset, ToOffset,
    display_map::{DisplaySnapshot, ToDisplayPoint},
    movement::{self, FindRange},
};
use gpui::{Action, Window, actions};
use itertools::Itertools;
use language::{BufferSnapshot, CharKind, Point, Selection, TextObject, TreeSitterOptions};
use multi_buffer::MultiBufferRow;
use schemars::JsonSchema;
use serde::Deserialize;
use ui::Context;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Object {
    Word { ignore_punctuation: bool },
    Subword { ignore_punctuation: bool },
    Sentence,
    Paragraph,
    Quotes,
    BackQuotes,
    AnyQuotes,
    MiniQuotes,
    DoubleQuotes,
    VerticalBars,
    AnyBrackets,
    MiniBrackets,
    Parentheses,
    SquareBrackets,
    CurlyBrackets,
    AngleBrackets,
    Argument,
    IndentObj { include_below: bool },
    Tag,
    Method,
    Class,
    Comment,
    EntireFile,
}

/// Selects a word text object.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
struct Word {
    #[serde(default)]
    ignore_punctuation: bool,
}

/// Selects a subword text object.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
struct Subword {
    #[serde(default)]
    ignore_punctuation: bool,
}
/// Selects text at the same indentation level.
#[derive(Clone, Deserialize, JsonSchema, PartialEq, Action)]
#[action(namespace = vim)]
#[serde(deny_unknown_fields)]
struct IndentObj {
    #[serde(default)]
    include_below: bool,
}

actions!(
    vim,
    [
        /// Selects a sentence text object.
        Sentence,
        /// Selects a paragraph text object.
        Paragraph,
        /// Selects text within single quotes.
        Quotes,
        /// Selects text within backticks.
        BackQuotes,
        /// Selects text within the nearest quotes (single or double).
        MiniQuotes,
        /// Selects text within any type of quotes.
        AnyQuotes,
        /// Selects text within double quotes.
        DoubleQuotes,
        /// Selects text within vertical bars (pipes).
        VerticalBars,
        /// Selects text within the nearest brackets.
        MiniBrackets,
        /// Selects text within any type of brackets.
        AnyBrackets,
        /// Selects a function argument.
        Argument,
        /// Selects an HTML/XML tag.
        Tag,
        /// Selects a method or function.
        Method,
        /// Selects a class definition.
        Class,
        /// Selects a comment block.
        Comment,
        /// Selects the entire file.
        EntireFile
    ]
);

pub fn register(editor: &mut Editor, cx: &mut Context<Vim>) {
    Vim::action(
        editor,
        cx,
        |vim, &Word { ignore_punctuation }: &Word, window, cx| {
            vim.object(Object::Word { ignore_punctuation }, window, cx)
        },
    );
    Vim::action(
        editor,
        cx,
        |vim, &Subword { ignore_punctuation }: &Subword, window, cx| {
            vim.object(Object::Subword { ignore_punctuation }, window, cx)
        },
    );
    Vim::action(editor, cx, |vim, _: &Tag, window, cx| {
        vim.object(Object::Tag, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &Sentence, window, cx| {
        vim.object(Object::Sentence, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &Paragraph, window, cx| {
        vim.object(Object::Paragraph, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &Quotes, window, cx| {
        vim.object(Object::Quotes, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &BackQuotes, window, cx| {
        vim.object(Object::BackQuotes, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &MiniQuotes, window, cx| {
        vim.object(Object::MiniQuotes, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &MiniBrackets, window, cx| {
        vim.object(Object::MiniBrackets, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &AnyQuotes, window, cx| {
        vim.object(Object::AnyQuotes, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &AnyBrackets, window, cx| {
        vim.object(Object::AnyBrackets, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &BackQuotes, window, cx| {
        vim.object(Object::BackQuotes, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &DoubleQuotes, window, cx| {
        vim.object(Object::DoubleQuotes, window, cx)
    });
    Vim::action(editor, cx, |vim, action: &Parentheses, window, cx| {
        vim.object_impl(Object::Parentheses, action.opening, window, cx)
    });
    Vim::action(editor, cx, |vim, action: &SquareBrackets, window, cx| {
        vim.object_impl(Object::SquareBrackets, action.opening, window, cx)
    });
    Vim::action(editor, cx, |vim, action: &CurlyBrackets, window, cx| {
        vim.object_impl(Object::CurlyBrackets, action.opening, window, cx)
    });
    Vim::action(editor, cx, |vim, action: &AngleBrackets, window, cx| {
        vim.object_impl(Object::AngleBrackets, action.opening, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &VerticalBars, window, cx| {
        vim.object(Object::VerticalBars, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &Argument, window, cx| {
        vim.object(Object::Argument, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &Method, window, cx| {
        vim.object(Object::Method, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &Class, window, cx| {
        vim.object(Object::Class, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &EntireFile, window, cx| {
        vim.object(Object::EntireFile, window, cx)
    });
    Vim::action(editor, cx, |vim, _: &Comment, window, cx| {
        if !matches!(vim.active_operator(), Some(Operator::Object { .. })) {
            vim.push_operator(Operator::Object { around: true }, window, cx);
        }
        vim.object(Object::Comment, window, cx)
    });
    Vim::action(
        editor,
        cx,
        |vim, &IndentObj { include_below }: &IndentObj, window, cx| {
            vim.object(Object::IndentObj { include_below }, window, cx)
        },
    );
}

impl Vim {
    fn object(&mut self, object: Object, window: &mut Window, cx: &mut Context<Self>) {
        self.object_impl(object, false, window, cx);
    }

    fn object_impl(
        &mut self,
        object: Object,
        opening: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = Self::take_count(cx);

        match self.mode {
            Mode::Normal | Mode::HelixNormal => {
                self.normal_object(object, count, opening, window, cx)
            }
            Mode::Visual | Mode::VisualLine | Mode::VisualBlock | Mode::HelixSelect => {
                self.visual_object(object, count, window, cx)
            }
            Mode::Insert | Mode::Replace => {
                // Shouldn't execute a text object in insert mode. Ignoring
            }
        }
    }
}

#[path = "object/delimiters.rs"]
mod delimiters;
mod object_impl;
#[path = "object/paragraph_sentence.rs"]
mod paragraph_sentence;
mod range_helpers;
#[path = "object/tag.rs"]
mod tag;
#[path = "object/word.rs"]
mod word;

use delimiters::{AngleBrackets, CurlyBrackets, Parentheses, SquareBrackets};
use paragraph_sentence::{expand_to_include_whitespace, paragraph, sentence};
pub use tag::surrounding_html_tag;
use word::{around_subword, around_word, in_subword, in_word};

#[cfg(test)]
#[path = "object/test/mod.rs"]
mod test;
