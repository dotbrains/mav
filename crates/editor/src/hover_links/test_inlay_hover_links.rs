use super::*;

#[gpui::test]
async fn test_inlay_hover_links(cx: &mut gpui::TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            enabled: Some(true),
            show_value_hints: Some(false),
            edit_debounce_ms: Some(0),
            scroll_debounce_ms: Some(0),
            show_type_hints: Some(true),
            show_parameter_hints: Some(true),
            show_other_hints: Some(true),
            show_background: Some(false),
            toggle_on_modifiers_press: None,
        })
    });

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            inlay_hint_provider: Some(lsp::OneOf::Left(true)),
            ..Default::default()
        },
        cx,
    )
    .await;
    cx.set_state(indoc! {"
            struct TestStruct;

            fn main() {
                let variableˇ = TestStruct;
            }
        "});
    let hint_start_offset = cx.ranges(indoc! {"
            struct TestStruct;

            fn main() {
                let variableˇ = TestStruct;
            }
        "})[0]
        .start;
    let hint_position = cx.to_lsp(MultiBufferOffset(hint_start_offset));
    let target_range = cx.lsp_range(indoc! {"
            struct «TestStruct»;

            fn main() {
                let variable = TestStruct;
            }
        "});

    let expected_uri = cx.buffer_lsp_url.clone();
    let hint_label = ": TestStruct";
    cx.lsp
        .set_request_handler::<lsp::request::InlayHintRequest, _, _>(move |params, _| {
            let expected_uri = expected_uri.clone();
            async move {
                assert_eq!(params.text_document.uri, expected_uri);
                Ok(Some(vec![lsp::InlayHint {
                    position: hint_position,
                    label: lsp::InlayHintLabel::LabelParts(vec![lsp::InlayHintLabelPart {
                        value: hint_label.to_string(),
                        location: Some(lsp::Location {
                            uri: params.text_document.uri,
                            range: target_range,
                        }),
                        ..Default::default()
                    }]),
                    kind: Some(lsp::InlayHintKind::TYPE),
                    text_edits: None,
                    tooltip: None,
                    padding_left: Some(false),
                    padding_right: Some(false),
                    data: None,
                }]))
            }
        })
        .next()
        .await;
    cx.background_executor.run_until_parked();
    cx.update_editor(|editor, _window, cx| {
        let expected_layers = vec![hint_label.to_string()];
        assert_eq!(expected_layers, cached_hint_labels(editor, cx));
        assert_eq!(expected_layers, visible_hint_labels(editor, cx));
    });

    let inlay_range = cx
        .ranges(indoc! {"
            struct TestStruct;

            fn main() {
                let variable« »= TestStruct;
            }
        "})
        .first()
        .cloned()
        .unwrap();
    let midpoint = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let previous_valid = MultiBufferOffset(inlay_range.start).to_display_point(&snapshot);
        let next_valid = MultiBufferOffset(inlay_range.end).to_display_point(&snapshot);
        assert_eq!(previous_valid.row(), next_valid.row());
        assert!(previous_valid.column() < next_valid.column());
        DisplayPoint::new(
            previous_valid.row(),
            previous_valid.column() + (hint_label.len() / 2) as u32,
        )
    });
    // Press cmd to trigger highlight
    let hover_point = cx.pixel_position_for(midpoint);
    cx.simulate_mouse_move(hover_point, None, Modifiers::secondary_key());
    cx.background_executor.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let actual_highlights = snapshot
            .inlay_highlights(HighlightKey::HoveredLinkState)
            .into_iter()
            .flat_map(|highlights| highlights.values().map(|(_, highlight)| highlight))
            .collect::<Vec<_>>();

        let buffer_snapshot = editor.buffer().update(cx, |buffer, cx| buffer.snapshot(cx));
        let expected_highlight = InlayHighlight {
            inlay: InlayId::Hint(0),
            inlay_position: buffer_snapshot.anchor_after(MultiBufferOffset(inlay_range.start)),
            range: 0..hint_label.len(),
        };
        assert_set_eq!(actual_highlights, vec![&expected_highlight]);
    });

    cx.simulate_mouse_move(hover_point, None, Modifiers::none());
    // Assert no link highlights
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let actual_ranges = snapshot
            .text_highlight_ranges(HighlightKey::HoveredLinkState)
            .map(|ranges| ranges.as_ref().clone().1)
            .unwrap_or_default();

        assert!(
            actual_ranges.is_empty(),
            "When no cmd is pressed, should have no hint label selected, but got: {actual_ranges:?}"
        );
    });

    cx.simulate_modifiers_change(Modifiers::secondary_key());
    cx.background_executor.run_until_parked();
    cx.simulate_click(hover_point, Modifiers::secondary_key());
    cx.background_executor.run_until_parked();
    cx.assert_editor_state(indoc! {"
            struct «TestStructˇ»;

            fn main() {
                let variable = TestStruct;
            }
        "});
}
