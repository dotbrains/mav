use super::super::*;
use gpui::{AbsoluteLength, Hsla, font};

#[test]
fn test_batched_text_run_can_append() {
    let style1 = TextRun {
        len: 1,
        font: font("Helvetica"),
        color: Hsla::red(),
        ..Default::default()
    };

    let style2 = TextRun {
        len: 1,
        font: font("Helvetica"),
        color: Hsla::red(),
        ..Default::default()
    };

    let style3 = TextRun {
        len: 1,
        font: font("Helvetica"),
        color: Hsla::blue(), // Different color
        ..Default::default()
    };

    let font_size = AbsoluteLength::Pixels(px(12.0));
    let batch = BatchedTextRun::new_from_char(LayoutPoint::new(0, 0), 'a', style1, font_size);

    // Should be able to append same style
    assert!(batch.can_append(&style2));

    // Should not be able to append different style
    assert!(!batch.can_append(&style3));
}

#[test]
fn test_batched_text_run_append() {
    let style = TextRun {
        len: 1,
        font: font("Helvetica"),
        color: Hsla::red(),
        ..Default::default()
    };

    let font_size = AbsoluteLength::Pixels(px(12.0));
    let mut batch = BatchedTextRun::new_from_char(LayoutPoint::new(0, 0), 'a', style, font_size);

    assert_eq!(batch.text, "a");
    assert_eq!(batch.cell_count, 1);
    assert_eq!(batch.style.len, 1);

    batch.append_char('b');

    assert_eq!(batch.text, "ab");
    assert_eq!(batch.cell_count, 2);
    assert_eq!(batch.style.len, 2);

    batch.append_char('c');

    assert_eq!(batch.text, "abc");
    assert_eq!(batch.cell_count, 3);
    assert_eq!(batch.style.len, 3);
}

#[test]
fn test_batched_text_run_append_char() {
    let style = TextRun {
        len: 1,
        font: font("Helvetica"),
        color: Hsla::red(),
        ..Default::default()
    };

    let font_size = AbsoluteLength::Pixels(px(12.0));
    let mut batch = BatchedTextRun::new_from_char(LayoutPoint::new(0, 0), 'x', style, font_size);

    assert_eq!(batch.text, "x");
    assert_eq!(batch.cell_count, 1);
    assert_eq!(batch.style.len, 1);

    batch.append_char('y');

    assert_eq!(batch.text, "xy");
    assert_eq!(batch.cell_count, 2);
    assert_eq!(batch.style.len, 2);

    // Test with multi-byte character
    batch.append_char('😀');

    assert_eq!(batch.text, "xy😀");
    assert_eq!(batch.cell_count, 3);
    assert_eq!(batch.style.len, 6); // 1 + 1 + 4 bytes for emoji
}

#[test]
fn test_batched_text_run_append_zero_width_char() {
    let style = TextRun {
        len: 1,
        font: font("Helvetica"),
        color: Hsla::red(),
        ..Default::default()
    };

    let font_size = AbsoluteLength::Pixels(px(12.0));
    let mut batch = BatchedTextRun::new_from_char(LayoutPoint::new(0, 0), 'x', style, font_size);

    let combining = '\u{0301}';
    batch.append_zero_width_chars(&[combining]);

    assert_eq!(batch.text, format!("x{}", combining));
    assert_eq!(batch.cell_count, 1);
    assert_eq!(batch.style.len, 1 + combining.len_utf8());
}
