use regex::Regex;
use rope::Rope;
use std::{borrow::Cow, cmp, sync::Arc, sync::LazyLock};

static LINE_SEPARATORS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\r\n|\r").expect("Failed to create LINE_SEPARATORS_REGEX"));

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineEnding {
    Unix,
    Windows,
}

impl Default for LineEnding {
    fn default() -> Self {
        #[cfg(unix)]
        return Self::Unix;

        #[cfg(not(unix))]
        return Self::Windows;
    }
}

impl LineEnding {
    pub fn as_str(&self) -> &'static str {
        match self {
            LineEnding::Unix => "\n",
            LineEnding::Windows => "\r\n",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            LineEnding::Unix => "LF",
            LineEnding::Windows => "CRLF",
        }
    }

    pub fn detect(text: &str) -> Self {
        let mut max_ix = cmp::min(text.len(), 1000);
        while !text.is_char_boundary(max_ix) {
            max_ix -= 1;
        }

        if let Some(ix) = text[..max_ix].find(['\n']) {
            if ix > 0 && text.as_bytes()[ix - 1] == b'\r' {
                Self::Windows
            } else {
                Self::Unix
            }
        } else {
            Self::default()
        }
    }

    pub fn normalize(text: &mut String) {
        if let Cow::Owned(replaced) = LINE_SEPARATORS_REGEX.replace_all(text, "\n") {
            *text = replaced;
        }
    }

    pub fn normalize_arc(text: Arc<str>) -> Arc<str> {
        if let Cow::Owned(replaced) = LINE_SEPARATORS_REGEX.replace_all(&text, "\n") {
            replaced.into()
        } else {
            text
        }
    }

    pub fn normalize_cow(text: Cow<str>) -> Cow<str> {
        if let Cow::Owned(replaced) = LINE_SEPARATORS_REGEX.replace_all(&text, "\n") {
            replaced.into()
        } else {
            text
        }
    }
}

pub fn chunks_with_line_ending(rope: &Rope, line_ending: LineEnding) -> impl Iterator<Item = &str> {
    rope.chunks().flat_map(move |chunk| {
        let mut newline = false;
        let end_with_newline = chunk.ends_with('\n').then_some(line_ending.as_str());
        chunk
            .lines()
            .flat_map(move |line| {
                let ending = if newline {
                    Some(line_ending.as_str())
                } else {
                    None
                };
                newline = true;
                ending.into_iter().chain([line])
            })
            .chain(end_with_newline)
    })
}
