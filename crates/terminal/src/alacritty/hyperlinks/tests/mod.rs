
use crate::terminal_settings::TerminalSettings;

use super::*;
use alacritty_terminal::{
    event::VoidListener,
    grid::Dimensions,
    index::{Boundary, Column, Line, Point as AlacPoint},
    term::{Config, cell::Flags, test::TermSize},
    vte::ansi::Handler,
};
use regex::Regex;
use settings::{self, Settings, SettingsContent};
use std::{cell::RefCell, ops::RangeInclusive, path::PathBuf, rc::Rc};
use url::Url;
use util::paths::PathWithPosition;

fn re_test(re: &str, hay: &str, expected: Vec<&str>) {
    let results: Vec<_> = Regex::new(re)
        .unwrap()
        .find_iter(hay)
        .map(|m| m.as_str())
        .collect();
    assert_eq!(results, expected);
}

#[test]
fn test_url_regex() {
    re_test(
        URL_REGEX,
        "test http://example.com test 'https://website1.com' test mailto:bob@example.com train",
        vec![
            "http://example.com",
            "https://website1.com",
            "mailto:bob@example.com",
        ],
    );
    re_test(
        URL_REGEX,
        "open mav://channel/the-channel and mav://settings/theme now",
        vec!["mav://channel/the-channel", "mav://settings/theme"],
    );
}

#[test]
fn test_url_parentheses_sanitization() {
    // Test our sanitize_url_parentheses function directly
    let test_cases = vec![
        // Cases that should be sanitized (unbalanced parentheses)
        ("https://www.google.com/)", "https://www.google.com/"),
        ("https://example.com/path)", "https://example.com/path"),
        ("https://test.com/))", "https://test.com/"),
        ("https://test.com/(((", "https://test.com/"),
        ("https://test.com/(test)(", "https://test.com/(test)"),
        // Cases that should NOT be sanitized (balanced parentheses)
        (
            "https://en.wikipedia.org/wiki/Example_(disambiguation)",
            "https://en.wikipedia.org/wiki/Example_(disambiguation)",
        ),
        ("https://test.com/(hello)", "https://test.com/(hello)"),
        (
            "https://example.com/path(1)(2)",
            "https://example.com/path(1)(2)",
        ),
        // Edge cases
        ("https://test.com/", "https://test.com/"),
        ("https://example.com", "https://example.com"),
    ];

    for (input, expected) in test_cases {
        // Create a minimal terminal for testing
        let term = Term::new(Config::default(), &TermSize::new(80, 24), VoidListener);

        // Create a dummy match that spans the entire input
        let start_point = AlacPoint::new(Line(0), Column(0));
        let end_point = AlacPoint::new(Line(0), Column(input.len()));
        let dummy_match = Match::new(start_point, end_point);

        let (result, _) = sanitize_url_punctuation(input.to_string(), dummy_match, &term);
        assert_eq!(result, expected, "Failed for input: {}", input);
    }
}

#[test]
fn test_url_punctuation_sanitization() {
    // Test URLs with trailing punctuation (sentence/text punctuation)
    // The sanitize_url_punctuation function removes ., ,, :, ;, from the end
    let test_cases = vec![
        ("https://example.com.", "https://example.com"),
        (
            "https://github.com/mav-industries/mav.",
            "https://github.com/mav-industries/mav",
        ),
        (
            "https://example.com/path/file.html.",
            "https://example.com/path/file.html",
        ),
        (
            "https://example.com/file.pdf.",
            "https://example.com/file.pdf",
        ),
        ("https://example.com:8080.", "https://example.com:8080"),
        ("https://example.com..", "https://example.com"),
        (
            "https://en.wikipedia.org/wiki/C.E.O.",
            "https://en.wikipedia.org/wiki/C.E.O",
        ),
        ("https://example.com,", "https://example.com"),
        ("https://example.com/path,", "https://example.com/path"),
        ("https://example.com,,", "https://example.com"),
        ("https://example.com:", "https://example.com"),
        ("https://example.com/path:", "https://example.com/path"),
        ("https://example.com::", "https://example.com"),
        ("https://example.com;", "https://example.com"),
        ("https://example.com/path;", "https://example.com/path"),
        ("https://example.com;;", "https://example.com"),
        ("https://example.com.,", "https://example.com"),
        ("https://example.com.:;", "https://example.com"),
        ("https://example.com!.", "https://example.com!"),
        ("https://example.com/).", "https://example.com/"),
        ("https://example.com/);", "https://example.com/"),
        ("https://example.com/;)", "https://example.com/"),
        (
            "https://example.com/v1.0/api",
            "https://example.com/v1.0/api",
        ),
        ("https://192.168.1.1", "https://192.168.1.1"),
        ("https://sub.domain.com", "https://sub.domain.com"),
        (
            "https://example.com?query=value",
            "https://example.com?query=value",
        ),
        ("https://example.com?a=1&b=2", "https://example.com?a=1&b=2"),
        (
            "https://example.com/path:8080",
            "https://example.com/path:8080",
        ),
    ];

    for (input, expected) in test_cases {
        // Create a minimal terminal for testing
        let term = Term::new(Config::default(), &TermSize::new(80, 24), VoidListener);

        // Create a dummy match that spans the entire input
        let start_point = AlacPoint::new(Line(0), Column(0));
        let end_point = AlacPoint::new(Line(0), Column(input.len()));
        let dummy_match = Match::new(start_point, end_point);

        let (result, _) = sanitize_url_punctuation(input.to_string(), dummy_match, &term);
        assert_eq!(result, expected, "Failed for input: {}", input);
    }
}

macro_rules! test_hyperlink {
        ($($lines:expr),+; $hyperlink_kind:ident) => { {
            use crate::alacritty::hyperlinks::tests::line_cells_count;
            use std::cmp;

            let test_lines = vec![$($lines),+];
            let (total_cells, longest_line_cells) =
                test_lines.iter().copied()
                    .map(line_cells_count)
                    .fold((0, 0), |state, cells| (state.0 + cells, cmp::max(state.1, cells)));
            let contains_tab_char = test_lines.iter().copied()
                .map(str::chars).flatten().find(|&c| c == '\t');
            let columns = if contains_tab_char.is_some() {
                // This avoids tabs at end of lines causing whitespace-eating line wraps...
                vec![longest_line_cells + 1]
            } else {
                // Alacritty has issues with 2 columns, use 3 as the minimum for now.
                vec![3, longest_line_cells / 2, longest_line_cells + 1]
            };
            test_hyperlink!(
                columns;
                total_cells;
                test_lines.iter().copied();
                $hyperlink_kind
            )
        } };

        ($columns:expr; $total_cells:expr; $lines:expr; $hyperlink_kind:ident) => { {
            use crate::alacritty::hyperlinks::tests::{ test_hyperlink, HyperlinkKind };

            let source_location = format!("{}:{}", std::file!(), std::line!());
            for columns in $columns {
                test_hyperlink(columns, $total_cells, $lines, HyperlinkKind::$hyperlink_kind,
                    &source_location);
            }
        } };
    }

mod file_iri;
mod helpers;
mod iri;
mod path;
mod perf;

pub(super) use helpers::{HyperlinkKind, line_cells_count, test_hyperlink};
