use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use anyhow::Result;
use client::proto;
use fancy_regex::{Captures, Regex, RegexBuilder};
use gpui::Entity;
use itertools::Itertools as _;
use language::{Buffer, BufferSnapshot, CharKind};
use smol::future::yield_now;
use std::{
    borrow::Cow,
    io::{BufRead, BufReader, Read},
    ops::Range,
    sync::{Arc, LazyLock},
};
use text::Anchor;
use util::{
    paths::{PathMatcher, PathStyle},
    rel_path::RelPath,
};

#[path = "search/execute.rs"]
mod execute;
#[path = "search/properties.rs"]
mod properties;

#[derive(Debug)]
pub enum SearchResult {
    Buffer {
        buffer: Entity<Buffer>,
        ranges: Vec<Range<Anchor>>,
    },
    LimitReached,
    WaitingForScan,
    Searching,
}

#[derive(Clone, Copy, PartialEq)]
pub enum SearchInputKind {
    Query,
    Include,
    Exclude,
}

#[derive(Clone, Debug)]
pub struct SearchInputs {
    query: Arc<str>,
    files_to_include: PathMatcher,
    files_to_exclude: PathMatcher,
    match_full_paths: bool,
    buffers: Option<Vec<Entity<Buffer>>>,
}

impl SearchInputs {
    pub fn as_str(&self) -> &str {
        self.query.as_ref()
    }
    pub fn files_to_include(&self) -> &PathMatcher {
        &self.files_to_include
    }
    pub fn files_to_exclude(&self) -> &PathMatcher {
        &self.files_to_exclude
    }
    pub fn buffers(&self) -> &Option<Vec<Entity<Buffer>>> {
        &self.buffers
    }
}
#[derive(Clone, Debug)]
pub enum SearchQuery {
    Text {
        search: AhoCorasick,
        replacement: Option<String>,
        whole_word: bool,
        case_sensitive: bool,
        include_ignored: bool,
        inner: SearchInputs,
    },
    Regex {
        regex: Regex,
        replacement: Option<String>,
        multiline: bool,
        whole_word: bool,
        case_sensitive: bool,
        include_ignored: bool,
        one_match_per_line: bool,
        inner: SearchInputs,
        escaped: bool,
    },
}

static WORD_MATCH_TEST: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\B")
        .build()
        .expect("Failed to create WORD_MATCH_TEST")
});

impl SearchQuery {
    /// Create a text query
    ///
    /// If `match_full_paths` is true, include/exclude patterns will always be matched against fully qualified project paths beginning with a project root.
    /// If `match_full_paths` is false, patterns will be matched against worktree-relative paths.
    pub fn text(
        query: impl ToString,
        whole_word: bool,
        case_sensitive: bool,
        include_ignored: bool,
        files_to_include: PathMatcher,
        files_to_exclude: PathMatcher,
        match_full_paths: bool,
        buffers: Option<Vec<Entity<Buffer>>>,
    ) -> Result<Self> {
        let mut query = query.to_string();
        text::LineEnding::normalize(&mut query);
        if !case_sensitive && !query.is_ascii() {
            // AhoCorasickBuilder doesn't support case-insensitive search with unicode characters
            // Fallback to regex search as recommended by
            // https://docs.rs/aho-corasick/1.1/aho_corasick/struct.AhoCorasickBuilder.html#method.ascii_case_insensitive
            return Self::escaped_regex(
                query,
                whole_word,
                case_sensitive,
                include_ignored,
                files_to_include,
                files_to_exclude,
                false,
                buffers,
            );
        }
        let search = AhoCorasickBuilder::new()
            .ascii_case_insensitive(!case_sensitive)
            .build([&query])?;
        let inner = SearchInputs {
            query: query.into(),
            files_to_exclude,
            files_to_include,
            match_full_paths,
            buffers,
        };
        Ok(Self::Text {
            search,
            replacement: None,
            whole_word,
            case_sensitive,
            include_ignored,
            inner,
        })
    }

    /// Create a regex query
    ///
    /// If `match_full_paths` is true, include/exclude patterns will be matched against fully qualified project paths
    /// beginning with a project root name. If false, they will be matched against project-relative paths (which don't start
    /// with their respective project root).
    pub fn regex(
        query: impl ToString,
        whole_word: bool,
        case_sensitive: bool,
        include_ignored: bool,
        one_match_per_line: bool,
        files_to_include: PathMatcher,
        files_to_exclude: PathMatcher,
        match_full_paths: bool,
        buffers: Option<Vec<Entity<Buffer>>>,
    ) -> Result<Self> {
        let query = query.to_string();
        let inner = SearchInputs {
            query: Arc::from(query.as_str()),
            files_to_include,
            files_to_exclude,
            match_full_paths,
            buffers,
        };
        Self::build_regex(
            query,
            whole_word,
            case_sensitive,
            include_ignored,
            one_match_per_line,
            inner,
            false,
        )
    }

    /// Create a regex query from a literal string, escaping any regex
    /// metacharacters so that the resulting query matches the literal text.
    ///
    /// Unlike `regex`, the query stored on the resulting `SearchQuery` is the
    /// original unescaped text, so `as_str` returns what the user typed.
    pub fn escaped_regex(
        query: impl ToString,
        whole_word: bool,
        case_sensitive: bool,
        include_ignored: bool,
        files_to_include: PathMatcher,
        files_to_exclude: PathMatcher,
        match_full_paths: bool,
        buffers: Option<Vec<Entity<Buffer>>>,
    ) -> Result<Self> {
        let mut query = query.to_string();
        text::LineEnding::normalize(&mut query);
        let inner = SearchInputs {
            query: Arc::from(query.as_str()),
            files_to_include,
            files_to_exclude,
            match_full_paths,
            buffers,
        };
        Self::build_regex(
            regex::escape(&query),
            whole_word,
            case_sensitive,
            include_ignored,
            false,
            inner,
            true,
        )
    }

    fn build_regex(
        mut pattern: String,
        whole_word: bool,
        mut case_sensitive: bool,
        include_ignored: bool,
        one_match_per_line: bool,
        inner: SearchInputs,
        escaped: bool,
    ) -> Result<Self> {
        if let Some((case_sensitive_from_pattern, new_pattern)) =
            Self::case_sensitive_from_pattern(&pattern)
        {
            case_sensitive = case_sensitive_from_pattern;
            pattern = new_pattern
        }

        if whole_word {
            let mut word_pattern = String::new();
            if let Some(first) = pattern.get(0..1)
                && WORD_MATCH_TEST.is_match(first).is_ok_and(|x| !x)
            {
                word_pattern.push_str("\\b");
            }
            word_pattern.push_str(&pattern);
            if let Some(last) = pattern.get(pattern.len() - 1..)
                && WORD_MATCH_TEST.is_match(last).is_ok_and(|x| !x)
            {
                word_pattern.push_str("\\b");
            }
            pattern = word_pattern
        }

        let multiline = pattern.contains('\n') || pattern.contains("\\n");
        if multiline {
            pattern.insert_str(0, "(?m)");
        }

        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!case_sensitive)
            .build()?;
        Ok(Self::Regex {
            regex,
            replacement: None,
            multiline,
            whole_word,
            case_sensitive,
            include_ignored,
            inner,
            one_match_per_line,
            escaped,
        })
    }

    /// Extracts case sensitivity settings from pattern items in the provided
    /// query and returns the same query, with the pattern items removed.
    ///
    /// The following pattern modifiers are supported:
    ///
    /// - `\c` (case_sensitive: false)
    /// - `\C` (case_sensitive: true)
    ///
    /// If no pattern item were found, `None` will be returned.
    fn case_sensitive_from_pattern(query: &str) -> Option<(bool, String)> {
        if !(query.contains("\\c") || query.contains("\\C")) {
            return None;
        }

        let mut was_escaped = false;
        let mut new_query = String::new();
        let mut is_case_sensitive = None;

        for c in query.chars() {
            if was_escaped {
                if c == 'c' {
                    is_case_sensitive = Some(false);
                } else if c == 'C' {
                    is_case_sensitive = Some(true);
                } else {
                    new_query.push('\\');
                    new_query.push(c);
                }
                was_escaped = false
            } else if c == '\\' {
                was_escaped = true
            } else {
                new_query.push(c);
            }
        }

        is_case_sensitive.map(|c| (c, new_query))
    }

    pub fn from_proto(message: proto::SearchQuery, path_style: PathStyle) -> Result<Self> {
        let files_to_include = if message.files_to_include.is_empty() {
            message
                .files_to_include_legacy
                .split(',')
                .map(str::trim)
                .filter(|&glob_str| !glob_str.is_empty())
                .map(|s| s.to_string())
                .collect()
        } else {
            message.files_to_include
        };

        let files_to_exclude = if message.files_to_exclude.is_empty() {
            message
                .files_to_exclude_legacy
                .split(',')
                .map(str::trim)
                .filter(|&glob_str| !glob_str.is_empty())
                .map(|s| s.to_string())
                .collect()
        } else {
            message.files_to_exclude
        };

        if message.regex {
            Self::regex(
                message.query,
                message.whole_word,
                message.case_sensitive,
                message.include_ignored,
                false,
                PathMatcher::new(files_to_include, path_style)?,
                PathMatcher::new(files_to_exclude, path_style)?,
                message.match_full_paths,
                None, // search opened only don't need search remote
            )
        } else {
            Self::text(
                message.query,
                message.whole_word,
                message.case_sensitive,
                message.include_ignored,
                PathMatcher::new(files_to_include, path_style)?,
                PathMatcher::new(files_to_exclude, path_style)?,
                message.match_full_paths,
                None, // search opened only don't need search remote
            )
        }
    }

    pub fn with_replacement(mut self, new_replacement: String) -> Self {
        match self {
            Self::Text {
                ref mut replacement,
                ..
            }
            | Self::Regex {
                ref mut replacement,
                ..
            } => {
                *replacement = Some(new_replacement);
                self
            }
        }
    }

    pub fn to_proto(&self) -> proto::SearchQuery {
        let mut files_to_include = self.files_to_include().sources();
        let mut files_to_exclude = self.files_to_exclude().sources();
        proto::SearchQuery {
            query: self.as_str().to_string(),
            regex: self.is_regex(),
            whole_word: self.whole_word(),
            case_sensitive: self.case_sensitive(),
            include_ignored: self.include_ignored(),
            files_to_include: files_to_include.clone().map(ToOwned::to_owned).collect(),
            files_to_exclude: files_to_exclude.clone().map(ToOwned::to_owned).collect(),
            match_full_paths: self.match_full_paths(),
            // Populate legacy fields for backwards compatibility
            files_to_include_legacy: files_to_include.join(","),
            files_to_exclude_legacy: files_to_exclude.join(","),
        }
    }

    /// Returns the replacement text for this `SearchQuery`.
    pub fn replacement(&self) -> Option<&str> {
        match self {
            SearchQuery::Text { replacement, .. } | SearchQuery::Regex { replacement, .. } => {
                replacement.as_deref()
            }
        }
    }
    /// Replaces search hits if replacement is set. `text` is assumed to be a string that matches this `SearchQuery` exactly, without any leftovers on either side.
    pub fn replacement_for<'a>(&self, text: &'a str) -> Option<Cow<'a, str>> {
        match self {
            SearchQuery::Text { replacement, .. }
            | SearchQuery::Regex {
                replacement,
                escaped: true,
                ..
            } => replacement.clone().map(Cow::from),

            SearchQuery::Regex {
                regex,
                replacement: Some(replacement),
                escaped: false,
                ..
            } => {
                static TEXT_REPLACEMENT_SPECIAL_CHARACTERS_REGEX: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r"\\\\|\\n|\\t").unwrap());
                let replacement = TEXT_REPLACEMENT_SPECIAL_CHARACTERS_REGEX.replace_all(
                    replacement,
                    |c: &Captures| match c.get(0).unwrap().as_str() {
                        r"\\" => "\\",
                        r"\n" => "\n",
                        r"\t" => "\t",
                        x => unreachable!("Unexpected escape sequence: {}", x),
                    },
                );
                Some(regex.replace(text, replacement))
            }

            SearchQuery::Regex {
                replacement: None, ..
            } => None,
        }
    }
}
