use super::*;
use std::collections::HashSet;

#[test]
fn test_window_button_layout_parse_standard() {
    let layout = WindowButtonLayout::parse("close,minimize:maximize").unwrap();
    assert_eq!(
        layout.left,
        [
            Some(WindowButton::Close),
            Some(WindowButton::Minimize),
            None
        ]
    );
    assert_eq!(layout.right, [Some(WindowButton::Maximize), None, None]);
}

#[test]
fn test_window_button_layout_parse_right_only() {
    let layout = WindowButtonLayout::parse("minimize,maximize,close").unwrap();
    assert_eq!(layout.left, [None, None, None]);
    assert_eq!(
        layout.right,
        [
            Some(WindowButton::Minimize),
            Some(WindowButton::Maximize),
            Some(WindowButton::Close)
        ]
    );
}

#[test]
fn test_window_button_layout_parse_left_only() {
    let layout = WindowButtonLayout::parse("close,minimize,maximize:").unwrap();
    assert_eq!(
        layout.left,
        [
            Some(WindowButton::Close),
            Some(WindowButton::Minimize),
            Some(WindowButton::Maximize)
        ]
    );
    assert_eq!(layout.right, [None, None, None]);
}

#[test]
fn test_window_button_layout_parse_with_whitespace() {
    let layout = WindowButtonLayout::parse(" close , minimize : maximize ").unwrap();
    assert_eq!(
        layout.left,
        [
            Some(WindowButton::Close),
            Some(WindowButton::Minimize),
            None
        ]
    );
    assert_eq!(layout.right, [Some(WindowButton::Maximize), None, None]);
}

#[test]
fn test_window_button_layout_parse_empty() {
    let layout = WindowButtonLayout::parse("").unwrap();
    assert_eq!(layout.left, [None, None, None]);
    assert_eq!(layout.right, [None, None, None]);
}

#[test]
fn test_window_button_layout_parse_intentionally_empty() {
    let layout = WindowButtonLayout::parse(":").unwrap();
    assert_eq!(layout.left, [None, None, None]);
    assert_eq!(layout.right, [None, None, None]);
}

#[test]
fn test_window_button_layout_parse_invalid_buttons() {
    let layout = WindowButtonLayout::parse("close,invalid,minimize:maximize,foo").unwrap();
    assert_eq!(
        layout.left,
        [
            Some(WindowButton::Close),
            Some(WindowButton::Minimize),
            None
        ]
    );
    assert_eq!(layout.right, [Some(WindowButton::Maximize), None, None]);
}

#[test]
fn test_window_button_layout_parse_deduplicates_same_side_buttons() {
    let layout = WindowButtonLayout::parse("close,close,minimize").unwrap();
    assert_eq!(
        layout.right,
        [
            Some(WindowButton::Close),
            Some(WindowButton::Minimize),
            None
        ]
    );
    assert_eq!(layout.format(), ":close,minimize");
}

#[test]
fn test_window_button_layout_parse_deduplicates_buttons_across_sides() {
    let layout = WindowButtonLayout::parse("close:maximize,close,minimize").unwrap();
    assert_eq!(layout.left, [Some(WindowButton::Close), None, None]);
    assert_eq!(
        layout.right,
        [
            Some(WindowButton::Maximize),
            Some(WindowButton::Minimize),
            None
        ]
    );

    let button_ids: Vec<_> = layout
        .left
        .iter()
        .chain(layout.right.iter())
        .flatten()
        .map(WindowButton::id)
        .collect();
    let unique_button_ids = button_ids.iter().copied().collect::<HashSet<_>>();
    assert_eq!(unique_button_ids.len(), button_ids.len());
    assert_eq!(layout.format(), "close:maximize,minimize");
}

#[test]
fn test_window_button_layout_parse_gnome_style() {
    let layout = WindowButtonLayout::parse("close").unwrap();
    assert_eq!(layout.left, [None, None, None]);
    assert_eq!(layout.right, [Some(WindowButton::Close), None, None]);
}

#[test]
fn test_window_button_layout_parse_elementary_style() {
    let layout = WindowButtonLayout::parse("close:maximize").unwrap();
    assert_eq!(layout.left, [Some(WindowButton::Close), None, None]);
    assert_eq!(layout.right, [Some(WindowButton::Maximize), None, None]);
}

#[test]
fn test_window_button_layout_round_trip() {
    let cases = [
        "close:minimize,maximize",
        "minimize,maximize,close:",
        ":close",
        "close:",
        "close:maximize",
        ":",
    ];

    for case in cases {
        let layout = WindowButtonLayout::parse(case).unwrap();
        assert_eq!(layout.format(), case, "Round-trip failed for: {}", case);
    }
}

#[test]
fn test_window_button_layout_linux_default() {
    let layout = WindowButtonLayout::linux_default();
    assert_eq!(layout.left, [None, None, None]);
    assert_eq!(
        layout.right,
        [
            Some(WindowButton::Minimize),
            Some(WindowButton::Maximize),
            Some(WindowButton::Close)
        ]
    );

    let round_tripped = WindowButtonLayout::parse(&layout.format()).unwrap();
    assert_eq!(round_tripped, layout);
}

#[test]
fn test_window_button_layout_parse_all_invalid() {
    assert!(WindowButtonLayout::parse("asdfghjkl").is_err());
}
