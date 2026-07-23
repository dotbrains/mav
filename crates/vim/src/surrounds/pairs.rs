use crate::object::Object;
use language::BracketPair;

use std::sync::Arc;

/// A char-based surround pair definition.
/// Single source of truth for all supported surround pairs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurroundPair {
    pub open: char,
    pub close: char,
}

impl SurroundPair {
    pub const fn new(open: char, close: char) -> Self {
        Self { open, close }
    }

    pub fn to_bracket_pair(self) -> BracketPair {
        BracketPair {
            start: self.open.to_string(),
            end: self.close.to_string(),
            close: true,
            surround: true,
            newline: false,
        }
    }

    pub fn to_object(self) -> Option<Object> {
        match self.open {
            '\'' => Some(Object::Quotes),
            '`' => Some(Object::BackQuotes),
            '"' => Some(Object::DoubleQuotes),
            '|' => Some(Object::VerticalBars),
            '(' => Some(Object::Parentheses),
            '[' => Some(Object::SquareBrackets),
            '{' => Some(Object::CurlyBrackets),
            '<' => Some(Object::AngleBrackets),
            _ => None,
        }
    }
}

/// All supported surround pairs - single source of truth.
pub const SURROUND_PAIRS: &[SurroundPair] = &[
    SurroundPair::new('(', ')'),
    SurroundPair::new('[', ']'),
    SurroundPair::new('{', '}'),
    SurroundPair::new('<', '>'),
    SurroundPair::new('"', '"'),
    SurroundPair::new('\'', '\''),
    SurroundPair::new('`', '`'),
    SurroundPair::new('|', '|'),
];

/// Bracket-only pairs for AnyBrackets matching.
pub const BRACKET_PAIRS: &[SurroundPair] = &[
    SurroundPair::new('(', ')'),
    SurroundPair::new('[', ']'),
    SurroundPair::new('{', '}'),
    SurroundPair::new('<', '>'),
];

/// Quote-only pairs for AnyQuotes matching.
pub const QUOTE_PAIRS: &[SurroundPair] = &[
    SurroundPair::new('"', '"'),
    SurroundPair::new('\'', '\''),
    SurroundPair::new('`', '`'),
];

fn object_to_surround_pair(object: Object) -> Option<SurroundPair> {
    let open = match object {
        Object::Quotes => '\'',
        Object::BackQuotes => '`',
        Object::DoubleQuotes => '"',
        Object::VerticalBars => '|',
        Object::Parentheses => '(',
        Object::SquareBrackets => '[',
        Object::CurlyBrackets { .. } => '{',
        Object::AngleBrackets => '<',
        _ => return None,
    };
    surround_pair_for_char_vim(open)
}

pub fn surround_alias(ch: &str) -> &str {
    match ch {
        "b" => ")",
        "B" => "}",
        "a" => ">",
        "r" => "]",
        _ => ch,
    }
}

fn literal_surround_pair(ch: char) -> Option<SurroundPair> {
    SURROUND_PAIRS
        .iter()
        .find(|p| p.open == ch || p.close == ch)
        .copied()
}

/// Resolve a character (including Vim aliases) to its surround pair.
/// Returns None for 'm' (match nearest) or unknown chars.
pub fn surround_pair_for_char_vim(ch: char) -> Option<SurroundPair> {
    let resolved = match ch {
        'b' => ')',
        'B' => '}',
        'r' => ']',
        'a' => '>',
        'm' => return None,
        _ => ch,
    };
    literal_surround_pair(resolved)
}

/// Get a BracketPair for the given string, with fallback for unknown chars.
/// For vim surround operations that accept any character as a surround.
pub fn bracket_pair_for_str_vim(text: &str) -> BracketPair {
    text.chars()
        .next()
        .and_then(surround_pair_for_char_vim)
        .map(|p| p.to_bracket_pair())
        .unwrap_or_else(|| BracketPair {
            start: text.to_string(),
            end: text.to_string(),
            close: true,
            surround: true,
            newline: false,
        })
}

/// Resolve a character to its surround pair using Helix semantics (no Vim aliases).
/// Returns None only for 'm' (match nearest). Unknown chars map to symmetric pairs.
pub fn surround_pair_for_char_helix(ch: char) -> Option<SurroundPair> {
    if ch == 'm' {
        return None;
    }
    literal_surround_pair(ch).or_else(|| Some(SurroundPair::new(ch, ch)))
}

/// Get a BracketPair for the given string in Helix mode (literal, symmetric fallback).
pub fn bracket_pair_for_str_helix(text: &str) -> BracketPair {
    text.chars()
        .next()
        .and_then(surround_pair_for_char_helix)
        .map(|p| p.to_bracket_pair())
        .unwrap_or_else(|| BracketPair {
            start: text.to_string(),
            end: text.to_string(),
            close: true,
            surround: true,
            newline: false,
        })
}
