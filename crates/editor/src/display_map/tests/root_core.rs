use super::super::*;

use crate::{
    movement,
    test::{marked_display_snapshot, test_font},
};
use Bias::*;
use block_map::BlockPlacement;
use gpui::{App, AppContext as _, BorrowAppContext, Element, Hsla, Rgba, div, font, observe, px};
use language::{
    Buffer, Diagnostic, DiagnosticEntry, DiagnosticSet, Language, LanguageConfig, LanguageMatcher,
};
use lsp::LanguageServerId;

use futures::stream::StreamExt;
use rand::{Rng, prelude::*};
use settings::{SettingsContent, SettingsStore};
use std::{env, sync::Arc};
use text::PointUtf16;
use theme::{LoadThemes, SyntaxTheme};
use unindent::Unindent as _;
use util::test::{marked_text_ranges, sample_text};

mod basic;
mod chunks;
mod random;

#[gpui::test]
async fn test_point_translation_with_replace_blocks(cx: &mut gpui::TestAppContext) {
    cx.background_executor
        .set_block_on_ticks(usize::MAX..=usize::MAX);

    cx.update(|cx| init_test(cx, &|_| {}));

    let buffer = cx.update(|cx| MultiBuffer::build_simple("abcde\nfghij\nklmno\npqrst", cx));
    let buffer_snapshot = buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx));
    let map = cx.new(|cx| {
        DisplayMap::new(
            buffer.clone(),
            font("Courier"),
            px(16.0),
            None,
            1,
            1,
            FoldPlaceholder::test(),
            DiagnosticSeverity::Warning,
            cx,
        )
    });

    let snapshot = map.update(cx, |map, cx| {
        map.insert_blocks(
            [BlockProperties {
                placement: BlockPlacement::Replace(
                    buffer_snapshot.anchor_before(Point::new(1, 2))
                        ..=buffer_snapshot.anchor_after(Point::new(2, 3)),
                ),
                height: Some(4),
                style: BlockStyle::Fixed,
                render: Arc::new(|_| div().into_any()),
                priority: 0,
            }],
            cx,
        );
        map.snapshot(cx)
    });

    assert_eq!(snapshot.text(), "abcde\n\n\n\n\npqrst");

    let point_to_display_points = [
        (Point::new(1, 0), DisplayPoint::new(DisplayRow(1), 0)),
        (Point::new(2, 0), DisplayPoint::new(DisplayRow(1), 0)),
        (Point::new(3, 0), DisplayPoint::new(DisplayRow(5), 0)),
    ];
    for (buffer_point, display_point) in point_to_display_points {
        assert_eq!(
            snapshot.point_to_display_point(buffer_point, Bias::Left),
            display_point,
            "point_to_display_point({:?}, Bias::Left)",
            buffer_point
        );
        assert_eq!(
            snapshot.point_to_display_point(buffer_point, Bias::Right),
            display_point,
            "point_to_display_point({:?}, Bias::Right)",
            buffer_point
        );
    }

    let display_points_to_points = [
        (
            DisplayPoint::new(DisplayRow(1), 0),
            Point::new(1, 0),
            Point::new(2, 5),
        ),
        (
            DisplayPoint::new(DisplayRow(2), 0),
            Point::new(1, 0),
            Point::new(2, 5),
        ),
        (
            DisplayPoint::new(DisplayRow(3), 0),
            Point::new(1, 0),
            Point::new(2, 5),
        ),
        (
            DisplayPoint::new(DisplayRow(4), 0),
            Point::new(1, 0),
            Point::new(2, 5),
        ),
        (
            DisplayPoint::new(DisplayRow(5), 0),
            Point::new(3, 0),
            Point::new(3, 0),
        ),
    ];
    for (display_point, left_buffer_point, right_buffer_point) in display_points_to_points {
        assert_eq!(
            snapshot.display_point_to_point(display_point, Bias::Left),
            left_buffer_point,
            "display_point_to_point({:?}, Bias::Left)",
            display_point
        );
        assert_eq!(
            snapshot.display_point_to_point(display_point, Bias::Right),
            right_buffer_point,
            "display_point_to_point({:?}, Bias::Right)",
            display_point
        );
    }
}

#[gpui::test]
async fn test_chunks_with_soft_wrapping(cx: &mut gpui::TestAppContext) {
    cx.background_executor
        .set_block_on_ticks(usize::MAX..=usize::MAX);

    let text = r#"
            fn outer() {}

            mod module {
                fn inner() {}
            }"#
    .unindent();

    let theme = SyntaxTheme::new_test(vec![("mod.body", Hsla::red()), ("fn.name", Hsla::blue())]);
    let language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "Test".into(),
                matcher: LanguageMatcher {
                    path_suffixes: vec![".test".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_highlights_query(
            r#"
                (mod_item name: (identifier) body: _ @mod.body)
                (function_item name: (identifier) @fn.name)
                "#,
        )
        .unwrap(),
    );
    language.set_theme(&theme);

    cx.update(|cx| init_test(cx, &|_| {}));

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    cx.condition(&buffer, |buf, _| !buf.is_parsing()).await;
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));

    let font_size = px(16.0);

    let map = cx.new(|cx| {
        DisplayMap::new(
            buffer,
            font("Courier"),
            font_size,
            Some(px(40.0)),
            1,
            1,
            FoldPlaceholder::test(),
            DiagnosticSeverity::Warning,
            cx,
        )
    });
    assert_eq!(
        cx.update(|cx| syntax_chunks(DisplayRow(0)..DisplayRow(5), &map, &theme, cx)),
        [
            ("fn \n".to_string(), None),
            ("oute".to_string(), Some(Hsla::blue())),
            ("\n".to_string(), None),
            ("r".to_string(), Some(Hsla::blue())),
            ("() \n{}\n\n".to_string(), None),
        ]
    );
    assert_eq!(
        cx.update(|cx| syntax_chunks(DisplayRow(3)..DisplayRow(5), &map, &theme, cx)),
        [("{}\n\n".to_string(), None)]
    );

    map.update(cx, |map, cx| {
        map.fold(
            vec![Crease::simple(
                MultiBufferPoint::new(0, 6)..MultiBufferPoint::new(3, 2),
                FoldPlaceholder::test(),
            )],
            cx,
        )
    });
    assert_eq!(
        cx.update(|cx| syntax_chunks(DisplayRow(1)..DisplayRow(4), &map, &theme, cx)),
        [
            ("out".to_string(), Some(Hsla::blue())),
            ("⋯\n".to_string(), None),
            ("  ".to_string(), Some(Hsla::red())),
            ("\n".to_string(), None),
            ("fn ".to_string(), Some(Hsla::red())),
            ("i".to_string(), Some(Hsla::blue())),
            ("\n".to_string(), None)
        ]
    );
}

#[gpui::test]
async fn test_chunks_with_text_highlights(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| init_test(cx, &|_| {}));

    let theme = SyntaxTheme::new_test(vec![("operator", Hsla::red()), ("string", Hsla::green())]);
    let language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "Test".into(),
                matcher: LanguageMatcher {
                    path_suffixes: vec![".test".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_highlights_query(
            r#"
                ":" @operator
                (string_literal) @string
                "#,
        )
        .unwrap(),
    );
    language.set_theme(&theme);

    let (text, highlighted_ranges) = marked_text_ranges(r#"constˇ «a»«:» B = "c «d»""#, false);

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    cx.condition(&buffer, |buf, _| !buf.is_parsing()).await;

    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let buffer_snapshot = buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx));

    let font_size = px(16.0);
    let map = cx.new(|cx| {
        DisplayMap::new(
            buffer,
            font("Courier"),
            font_size,
            None,
            1,
            1,
            FoldPlaceholder::test(),
            DiagnosticSeverity::Warning,
            cx,
        )
    });

    let style = HighlightStyle {
        color: Some(Hsla::blue()),
        ..Default::default()
    };

    map.update(cx, |map, cx| {
        map.highlight_text(
            HighlightKey::Editor,
            highlighted_ranges
                .into_iter()
                .map(|range| MultiBufferOffset(range.start)..MultiBufferOffset(range.end))
                .map(|range| {
                    buffer_snapshot.anchor_before(range.start)
                        ..buffer_snapshot.anchor_before(range.end)
                })
                .collect(),
            style,
            false,
            cx,
        );
    });

    assert_eq!(
        cx.update(|cx| chunks(DisplayRow(0)..DisplayRow(10), &map, &theme, cx)),
        [
            ("const ".to_string(), None, None),
            ("a".to_string(), None, Some(Hsla::blue())),
            (":".to_string(), Some(Hsla::red()), Some(Hsla::blue())),
            (" B = ".to_string(), None, None),
            ("\"c ".to_string(), Some(Hsla::green()), None),
            ("d".to_string(), Some(Hsla::green()), Some(Hsla::blue())),
            ("\"".to_string(), Some(Hsla::green()), None),
        ]
    );
}

#[gpui::test]
fn test_clip_point(cx: &mut gpui::App) {
    init_test(cx, &|_| {});

    fn assert(text: &str, shift_right: bool, bias: Bias, cx: &mut gpui::App) {
        let (unmarked_snapshot, mut markers) = marked_display_snapshot(text, cx);

        match bias {
            Bias::Left => {
                if shift_right {
                    *markers[1].column_mut() += 1;
                }

                assert_eq!(unmarked_snapshot.clip_point(markers[1], bias), markers[0])
            }
            Bias::Right => {
                if shift_right {
                    *markers[0].column_mut() += 1;
                }

                assert_eq!(unmarked_snapshot.clip_point(markers[0], bias), markers[1])
            }
        };
    }

    use Bias::{Left, Right};
    assert("ˇˇα", false, Left, cx);
    assert("ˇˇα", true, Left, cx);
    assert("ˇˇα", false, Right, cx);
    assert("ˇαˇ", true, Right, cx);
    assert("ˇˇ✋", false, Left, cx);
    assert("ˇˇ✋", true, Left, cx);
    assert("ˇˇ✋", false, Right, cx);
    assert("ˇ✋ˇ", true, Right, cx);
    assert("ˇˇ🍐", false, Left, cx);
    assert("ˇˇ🍐", true, Left, cx);
    assert("ˇˇ🍐", false, Right, cx);
    assert("ˇ🍐ˇ", true, Right, cx);
    assert("ˇˇ\t", false, Left, cx);
    assert("ˇˇ\t", true, Left, cx);
    assert("ˇˇ\t", false, Right, cx);
    assert("ˇ\tˇ", true, Right, cx);
    assert(" ˇˇ\t", false, Left, cx);
    assert(" ˇˇ\t", true, Left, cx);
    assert(" ˇˇ\t", false, Right, cx);
    assert(" ˇ\tˇ", true, Right, cx);
    assert("   ˇˇ\t", false, Left, cx);
    assert("   ˇˇ\t", false, Right, cx);
}

#[gpui::test]
fn test_clip_at_line_ends(cx: &mut gpui::App) {
    init_test(cx, &|_| {});

    fn assert(text: &str, cx: &mut gpui::App) {
        let (mut unmarked_snapshot, markers) = marked_display_snapshot(text, cx);
        unmarked_snapshot.clip_at_line_ends = true;
        assert_eq!(
            unmarked_snapshot.clip_point(markers[1], Bias::Left),
            markers[0]
        );
    }

    assert("ˇˇ", cx);
    assert("ˇaˇ", cx);
    assert("aˇbˇ", cx);
    assert("aˇαˇ", cx);
}

#[gpui::test]
fn test_creases(cx: &mut gpui::App) {
    init_test(cx, &|_| {});

    let text = "aaa\nbbb\nccc\nddd\neee\nfff\nggg\nhhh\niii\njjj\nkkk\nlll";
    let buffer = MultiBuffer::build_simple(text, cx);
    let font_size = px(14.0);
    cx.new(|cx| {
        let mut map = DisplayMap::new(
            buffer.clone(),
            font("Helvetica"),
            font_size,
            None,
            1,
            1,
            FoldPlaceholder::test(),
            DiagnosticSeverity::Warning,
            cx,
        );
        let snapshot = map.buffer.read(cx).snapshot(cx);
        let range =
            snapshot.anchor_before(Point::new(2, 0))..snapshot.anchor_after(Point::new(3, 3));

        map.crease_map.insert(
            [Crease::inline(
                range,
                FoldPlaceholder::test(),
                |_row, _status, _toggle, _window, _cx| div(),
                |_row, _status, _window, _cx| div(),
            )],
            &map.buffer.read(cx).snapshot(cx),
        );

        map
    });
}
