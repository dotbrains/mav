use super::*;
use crate::Buffer;
use clock::ReplicaId;
use collections::BTreeMap;
use futures::FutureExt as _;
use futures_lite::future::yield_now;
use gpui::{App, AppContext as _, BorrowAppContext, Entity};
use gpui::{HighlightStyle, TestAppContext};
use indoc::indoc;
use proto::deserialize_operation;
use rand::prelude::*;
use regex::RegexBuilder;
use settings::SettingsStore;
use settings::{AllLanguageSettingsContent, LanguageSettingsContent};
use std::collections::BTreeSet;
use std::{
    env,
    ops::Range,
    sync::LazyLock,
    time::{Duration, Instant},
};
use syntax_map::TreeSitterOptions;
use text::network::Network;
use text::{BufferId, LineEnding};
use text::{Point, ToPoint};
use theme::ActiveTheme;
use unindent::Unindent as _;
use util::rel_path::rel_path;
use util::test::marked_text_offsets;
use util::{RandomCharIter, assert_set_eq, post_inc, test::marked_text_ranges};

pub static TRAILING_WHITESPACE_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"[ \t]+$")
        .multi_line(true)
        .build()
        .expect("Failed to create TRAILING_WHITESPACE_REGEX")
});

#[cfg(test)]
#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

mod language_matching;
mod line_endings;

mod autoindent_basic;
mod autoindent_block;
mod autoindent_language_preview;
mod collaboration_preview;
mod edit_diff_reparse;
mod empty_lines;
mod injected_languages;
mod language_scopes;
mod random_collaboration;
mod ranges_words;
fn ruby_lang() -> Language {
    Language::new(
        LanguageConfig {
            name: "Ruby".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rb".to_string()],
                ..Default::default()
            },
            line_comments: vec!["# ".into()],
            ..Default::default()
        },
        Some(tree_sitter_ruby::LANGUAGE.into()),
    )
    .with_indents_query(
        r#"
            (class "end" @end) @indent
            (method "end" @end) @indent
            (rescue) @outdent
            (then) @indent
        "#,
    )
    .unwrap()
}

fn html_lang() -> Language {
    Language::new(
        LanguageConfig {
            name: LanguageName::new_static("HTML"),
            block_comment: Some(BlockCommentConfig {
                start: "<!--".into(),
                prefix: "".into(),
                end: "-->".into(),
                tab_size: 0,
            }),
            ..Default::default()
        },
        Some(tree_sitter_html::LANGUAGE.into()),
    )
    .with_indents_query(
        "
        (element
          (start_tag) @start
          (end_tag)? @end) @indent
        ",
    )
    .unwrap()
    .with_injection_query(
        r#"
        (script_element
            (raw_text) @injection.content
            (#set! injection.language "javascript"))
        "#,
    )
    .unwrap()
}

fn erb_lang() -> Language {
    Language::new(
        LanguageConfig {
            name: "HTML+ERB".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["erb".to_string()],
                ..Default::default()
            },
            block_comment: Some(BlockCommentConfig {
                start: "<%#".into(),
                prefix: "".into(),
                end: "%>".into(),
                tab_size: 0,
            }),
            ..Default::default()
        },
        Some(tree_sitter_embedded_template::LANGUAGE.into()),
    )
    .with_injection_query(
        r#"
            (
                (code) @content
                (#set! "language" "ruby")
                (#set! "combined")
            )

            (
                (content) @content
                (#set! "language" "html")
                (#set! "combined")
            )
        "#,
    )
    .unwrap()
}

fn json_lang() -> Language {
    Language::new(
        LanguageConfig {
            name: "Json".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["js".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_json::LANGUAGE.into()),
    )
}

fn javascript_lang() -> Language {
    Language::new(
        LanguageConfig {
            name: "JavaScript".into(),
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
    )
    .with_brackets_query(
        r#"
        ("{" @open "}" @close)
        ("(" @open ")" @close)
        "#,
    )
    .unwrap()
    .with_indents_query(
        r#"
        (object "}" @end) @indent
        "#,
    )
    .unwrap()
}

fn c_lang() -> Arc<Language> {
    Arc::new(
        Language::new(
            LanguageConfig {
                name: "C".into(),
                ..Default::default()
            },
            Some(tree_sitter_c::LANGUAGE.into()),
        )
        .with_outline_query(include_str!("../../grammars/src/c/outline.scm"))
        .unwrap(),
    )
}

pub fn markdown_inline_lang() -> Language {
    Language::new(
        LanguageConfig {
            name: "Markdown-Inline".into(),
            hidden: true,
            ..LanguageConfig::default()
        },
        Some(tree_sitter_md::INLINE_LANGUAGE.into()),
    )
    .with_highlights_query("(emphasis) @emphasis")
    .unwrap()
}

fn get_tree_sexp(buffer: &Entity<Buffer>, cx: &mut gpui::TestAppContext) -> String {
    buffer.update(cx, |buffer, _| {
        let snapshot = buffer.snapshot();
        let layers = snapshot.syntax.layers(buffer.as_text_snapshot());
        layers[0].node().to_sexp()
    })
}

// Assert that the enclosing bracket ranges around the selection match the pairs indicated by the marked text in `range_markers`
#[track_caller]
fn assert_bracket_pairs(
    selection_text: &'static str,
    bracket_pair_texts: Vec<&'static str>,
    language: Arc<Language>,
    cx: &mut App,
) {
    let (expected_text, selection_ranges) = marked_text_ranges(selection_text, false);
    let buffer = cx.new(|cx| Buffer::local(expected_text.clone(), cx).with_language(language, cx));
    let buffer = buffer.update(cx, |buffer, _cx| buffer.snapshot());

    let selection_range = selection_ranges[0].clone();

    let bracket_pairs = bracket_pair_texts
        .into_iter()
        .map(|pair_text| {
            let (bracket_text, ranges) = marked_text_ranges(pair_text, false);
            assert_eq!(bracket_text, expected_text);
            (ranges[0].clone(), ranges[1].clone())
        })
        .collect::<Vec<_>>();

    assert_set_eq!(
        buffer
            .bracket_ranges(selection_range)
            .map(|pair| (pair.open_range, pair.close_range))
            .collect::<Vec<_>>(),
        bracket_pairs
    );
}

fn init_settings(cx: &mut App, f: fn(&mut AllLanguageSettingsContent)) {
    let settings_store = SettingsStore::test(cx);
    cx.set_global(settings_store);
    cx.update_global::<SettingsStore, _>(|settings, cx| {
        settings.update_user_settings(cx, |content| f(&mut content.project.all_languages));
    });
}

#[gpui::test(iterations = 100)]
fn test_random_chunk_bitmaps(cx: &mut App, mut rng: StdRng) {
    use util::RandomCharIter;

    // Generate random text
    let len = rng.random_range(0..10000);
    let text = RandomCharIter::new(&mut rng).take(len).collect::<String>();

    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let snapshot = buffer.read(cx).snapshot();

    // Get all chunks and verify their bitmaps
    let chunks = snapshot.chunks(
        0..snapshot.len(),
        LanguageAwareStyling {
            tree_sitter: false,
            diagnostics: false,
        },
    );

    for chunk in chunks {
        let chunk_text = chunk.text;
        let chars_bitmap = chunk.chars;
        let tabs_bitmap = chunk.tabs;

        // Check empty chunks have empty bitmaps
        if chunk_text.is_empty() {
            assert_eq!(
                chars_bitmap, 0,
                "Empty chunk should have empty chars bitmap"
            );
            assert_eq!(tabs_bitmap, 0, "Empty chunk should have empty tabs bitmap");
            continue;
        }

        // Verify that chunk text doesn't exceed 128 bytes
        assert!(
            chunk_text.len() <= 128,
            "Chunk text length {} exceeds 128 bytes",
            chunk_text.len()
        );

        // Verify chars bitmap
        let char_indices = chunk_text
            .char_indices()
            .map(|(i, _)| i)
            .collect::<Vec<_>>();

        for byte_idx in 0..chunk_text.len() {
            let should_have_bit = char_indices.contains(&byte_idx);
            let has_bit = chars_bitmap & (1 << byte_idx) != 0;

            if has_bit != should_have_bit {
                eprintln!("Chunk text bytes: {:?}", chunk_text.as_bytes());
                eprintln!("Char indices: {:?}", char_indices);
                eprintln!("Chars bitmap: {:#b}", chars_bitmap);
            }

            assert_eq!(
                has_bit, should_have_bit,
                "Chars bitmap mismatch at byte index {} in chunk {:?}. Expected bit: {}, Got bit: {}",
                byte_idx, chunk_text, should_have_bit, has_bit
            );
        }

        // Verify tabs bitmap
        for (byte_idx, byte) in chunk_text.bytes().enumerate() {
            let is_tab = byte == b'\t';
            let has_bit = tabs_bitmap & (1 << byte_idx) != 0;

            if has_bit != is_tab {
                eprintln!("Chunk text bytes: {:?}", chunk_text.as_bytes());
                eprintln!("Tabs bitmap: {:#b}", tabs_bitmap);
                assert_eq!(
                    has_bit, is_tab,
                    "Tabs bitmap mismatch at byte index {} in chunk {:?}. Byte: {:?}, Expected bit: {}, Got bit: {}",
                    byte_idx, chunk_text, byte as char, is_tab, has_bit
                );
            }
        }
    }
}

#[gpui::test]
fn test_formatted_chunks(cx: &mut gpui::App) {
    init_settings(cx, |_| {});
    let buffer = cx.new(|cx| Buffer::local("use std::cmp::Eq;", cx).with_language(rust_lang(), cx));
    let snapshot = buffer.read(cx).snapshot();

    let chunks = snapshot.chunks(
        0..snapshot.len(),
        LanguageAwareStyling {
            tree_sitter: true,
            diagnostics: false,
        },
    );

    for chunk in chunks {
        let chunk_text = chunk.text;
        let chars_bitmap = chunk.chars;

        // Verify chars bitmap
        let char_indices = chunk_text
            .char_indices()
            .map(|(i, _)| i)
            .collect::<Vec<_>>();

        assert_eq!(char_indices.len() as u32, chars_bitmap.count_ones());

        for byte_idx in 0..chunk_text.len() {
            let should_have_bit = char_indices.contains(&byte_idx);
            let has_bit = chars_bitmap & (1 << byte_idx) != 0;

            if has_bit != should_have_bit {
                eprintln!("Chunk text bytes: {:?}", chunk_text.as_bytes());
                eprintln!("Char indices: {:?}", char_indices);
                eprintln!("Chars bitmap: {:#b}", chars_bitmap);
            }

            assert_eq!(
                has_bit, should_have_bit,
                "Chars bitmap mismatch at byte index {} in chunk {:?}. Expected bit: {}, Got bit: {}",
                byte_idx, chunk_text, should_have_bit, has_bit
            );
        }
    }
}
