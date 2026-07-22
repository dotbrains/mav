use super::*;

#[test]
fn test_inlay_properties_label_padding() {
    assert_eq!(
        Inlay::hint(
            InlayId::Hint(0),
            Anchor::Min,
            &InlayHint {
                label: InlayHintLabel::String("a".to_string()),
                position: text::Anchor::min_for_buffer(BufferId::new(1).unwrap()),
                padding_left: false,
                padding_right: false,
                tooltip: None,
                kind: None,
                resolve_state: ResolveState::Resolved,
            },
        )
        .text()
        .to_string(),
        "a",
        "Should not pad label if not requested"
    );

    assert_eq!(
        Inlay::hint(
            InlayId::Hint(0),
            Anchor::Min,
            &InlayHint {
                label: InlayHintLabel::String("a".to_string()),
                position: text::Anchor::min_for_buffer(BufferId::new(1).unwrap()),
                padding_left: true,
                padding_right: true,
                tooltip: None,
                kind: None,
                resolve_state: ResolveState::Resolved,
            },
        )
        .text()
        .to_string(),
        " a ",
        "Should pad label for every side requested"
    );

    assert_eq!(
        Inlay::hint(
            InlayId::Hint(0),
            Anchor::Min,
            &InlayHint {
                label: InlayHintLabel::String(" a ".to_string()),
                position: text::Anchor::min_for_buffer(BufferId::new(1).unwrap()),
                padding_left: false,
                padding_right: false,
                tooltip: None,
                kind: None,
                resolve_state: ResolveState::Resolved,
            },
        )
        .text()
        .to_string(),
        " a ",
        "Should not change already padded label"
    );

    assert_eq!(
        Inlay::hint(
            InlayId::Hint(0),
            Anchor::Min,
            &InlayHint {
                label: InlayHintLabel::String(" a ".to_string()),
                position: text::Anchor::min_for_buffer(BufferId::new(1).unwrap()),
                padding_left: true,
                padding_right: true,
                tooltip: None,
                kind: None,
                resolve_state: ResolveState::Resolved,
            },
        )
        .text()
        .to_string(),
        " a ",
        "Should not change already padded label"
    );
}

#[gpui::test]
fn test_inlay_hint_padding_with_multibyte_chars() {
    assert_eq!(
        Inlay::hint(
            InlayId::Hint(0),
            Anchor::Min,
            &InlayHint {
                label: InlayHintLabel::String("🎨".to_string()),
                position: text::Anchor::min_for_buffer(BufferId::new(1).unwrap()),
                padding_left: true,
                padding_right: true,
                tooltip: None,
                kind: None,
                resolve_state: ResolveState::Resolved,
            },
        )
        .text()
        .to_string(),
        " 🎨 ",
        "Should pad single emoji correctly"
    );
}
