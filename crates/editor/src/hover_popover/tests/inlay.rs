use super::*;

#[gpui::test]
async fn test_hover_inlay_label_parts(cx: &mut gpui::TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            show_value_hints: Some(true),
            enabled: Some(true),
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
            inlay_hint_provider: Some(lsp::OneOf::Right(
                lsp::InlayHintServerCapabilities::Options(lsp::InlayHintOptions {
                    resolve_provider: Some(true),
                    ..Default::default()
                }),
            )),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"
            struct TestStruct;

            // ==================

            struct TestNewType<T>(T);

            fn main() {
                let variableˇ = TestNewType(TestStruct);
            }
        "});

    let hint_start_offset = cx.ranges(indoc! {"
            struct TestStruct;

            // ==================

            struct TestNewType<T>(T);

            fn main() {
                let variableˇ = TestNewType(TestStruct);
            }
        "})[0]
        .start;
    let hint_position = cx.to_lsp(MultiBufferOffset(hint_start_offset));
    let new_type_target_range = cx.lsp_range(indoc! {"
            struct TestStruct;

            // ==================

            struct «TestNewType»<T>(T);

            fn main() {
                let variable = TestNewType(TestStruct);
            }
        "});
    let struct_target_range = cx.lsp_range(indoc! {"
            struct «TestStruct»;

            // ==================

            struct TestNewType<T>(T);

            fn main() {
                let variable = TestNewType(TestStruct);
            }
        "});

    let uri = cx.buffer_lsp_url.clone();
    let new_type_label = "TestNewType";
    let struct_label = "TestStruct";
    let entire_hint_label = ": TestNewType<TestStruct>";
    let closure_uri = uri.clone();
    cx.lsp
        .set_request_handler::<lsp::request::InlayHintRequest, _, _>(move |params, _| {
            let task_uri = closure_uri.clone();
            async move {
                assert_eq!(params.text_document.uri, task_uri);
                Ok(Some(vec![lsp::InlayHint {
                    position: hint_position,
                    label: lsp::InlayHintLabel::LabelParts(vec![lsp::InlayHintLabelPart {
                        value: entire_hint_label.to_string(),
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
    cx.update_editor(|editor, _, cx| {
        let expected_layers = vec![entire_hint_label.to_string()];
        assert_eq!(expected_layers, cached_hint_labels(editor, cx));
        assert_eq!(expected_layers, visible_hint_labels(editor, cx));
    });

    let inlay_range = cx
        .ranges(indoc! {"
                struct TestStruct;

                // ==================

                struct TestNewType<T>(T);

                fn main() {
                    let variable« »= TestNewType(TestStruct);
                }
        "})
        .first()
        .cloned()
        .unwrap();
    let new_type_hint_part_hover_position = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let previous_valid = MultiBufferOffset(inlay_range.start).to_display_point(&snapshot);
        let next_valid = MultiBufferOffset(inlay_range.end).to_display_point(&snapshot);
        assert_eq!(previous_valid.row(), next_valid.row());
        assert!(previous_valid.column() < next_valid.column());
        let exact_unclipped = DisplayPoint::new(
            previous_valid.row(),
            previous_valid.column()
                + (entire_hint_label.find(new_type_label).unwrap() + new_type_label.len() / 2)
                    as u32,
        );
        PointForPosition {
            previous_valid,
            next_valid,
            nearest_valid: previous_valid,
            exact_unclipped,
            column_overshoot_after_line_end: 0,
        }
    });
    cx.update_editor(|editor, window, cx| {
        editor.update_inlay_link_and_hover_points(
            &editor.snapshot(window, cx),
            new_type_hint_part_hover_position,
            None,
            true,
            false,
            window,
            cx,
        );
    });

    let resolve_closure_uri = uri.clone();
    cx.lsp
        .set_request_handler::<lsp::request::InlayHintResolveRequest, _, _>(
            move |mut hint_to_resolve, _| {
                let mut resolved_hint_positions = BTreeSet::new();
                let task_uri = resolve_closure_uri.clone();
                async move {
                    let inserted = resolved_hint_positions.insert(hint_to_resolve.position);
                    assert!(inserted, "Hint {hint_to_resolve:?} was resolved twice");

                    // `: TestNewType<TestStruct>`
                    hint_to_resolve.label = lsp::InlayHintLabel::LabelParts(vec![
                        lsp::InlayHintLabelPart {
                            value: ": ".to_string(),
                            ..Default::default()
                        },
                        lsp::InlayHintLabelPart {
                            value: new_type_label.to_string(),
                            location: Some(lsp::Location {
                                uri: task_uri.clone(),
                                range: new_type_target_range,
                            }),
                            tooltip: Some(lsp::InlayHintLabelPartTooltip::String(format!(
                                "A tooltip for `{new_type_label}`"
                            ))),
                            ..Default::default()
                        },
                        lsp::InlayHintLabelPart {
                            value: "<".to_string(),
                            ..Default::default()
                        },
                        lsp::InlayHintLabelPart {
                            value: struct_label.to_string(),
                            location: Some(lsp::Location {
                                uri: task_uri,
                                range: struct_target_range,
                            }),
                            tooltip: Some(lsp::InlayHintLabelPartTooltip::MarkupContent(
                                lsp::MarkupContent {
                                    kind: lsp::MarkupKind::Markdown,
                                    value: format!("A tooltip for `{struct_label}`"),
                                },
                            )),
                            ..Default::default()
                        },
                        lsp::InlayHintLabelPart {
                            value: ">".to_string(),
                            ..Default::default()
                        },
                    ]);

                    Ok(hint_to_resolve)
                }
            },
        )
        .next()
        .await;
    cx.background_executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.update_inlay_link_and_hover_points(
            &editor.snapshot(window, cx),
            new_type_hint_part_hover_position,
            None,
            true,
            false,
            window,
            cx,
        );
    });
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    cx.background_executor.run_until_parked();
    cx.update_editor(|editor, _, cx| {
        let hover_state = &editor.hover_state;
        assert!(hover_state.diagnostic_popover.is_none() && hover_state.info_popovers.len() == 1);
        let popover = hover_state.info_popovers.first().unwrap();
        let buffer_snapshot = editor.buffer().update(cx, |buffer, cx| buffer.snapshot(cx));
        assert_eq!(
            popover.symbol_range,
            RangeInEditor::Inlay(InlayHighlight {
                inlay: InlayId::Hint(0),
                inlay_position: buffer_snapshot.anchor_after(MultiBufferOffset(inlay_range.start)),
                range: ": ".len()..": ".len() + new_type_label.len(),
            }),
            "Popover range should match the new type label part"
        );
        assert_eq!(
            popover.get_rendered_text(cx),
            format!("A tooltip for {new_type_label}"),
        );
    });

    let struct_hint_part_hover_position = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let previous_valid = MultiBufferOffset(inlay_range.start).to_display_point(&snapshot);
        let next_valid = MultiBufferOffset(inlay_range.end).to_display_point(&snapshot);
        assert_eq!(previous_valid.row(), next_valid.row());
        assert!(previous_valid.column() < next_valid.column());
        let exact_unclipped = DisplayPoint::new(
            previous_valid.row(),
            previous_valid.column()
                + (entire_hint_label.find(struct_label).unwrap() + struct_label.len() / 2) as u32,
        );
        PointForPosition {
            previous_valid,
            next_valid,
            nearest_valid: previous_valid,
            exact_unclipped,
            column_overshoot_after_line_end: 0,
        }
    });
    cx.update_editor(|editor, window, cx| {
        editor.update_inlay_link_and_hover_points(
            &editor.snapshot(window, cx),
            struct_hint_part_hover_position,
            None,
            true,
            false,
            window,
            cx,
        );
    });
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    cx.background_executor.run_until_parked();
    cx.update_editor(|editor, _, cx| {
        let hover_state = &editor.hover_state;
        assert!(hover_state.diagnostic_popover.is_none() && hover_state.info_popovers.len() == 1);
        let popover = hover_state.info_popovers.first().unwrap();
        let buffer_snapshot = editor.buffer().update(cx, |buffer, cx| buffer.snapshot(cx));
        assert_eq!(
            popover.symbol_range,
            RangeInEditor::Inlay(InlayHighlight {
                inlay: InlayId::Hint(0),
                inlay_position: buffer_snapshot.anchor_after(MultiBufferOffset(inlay_range.start)),
                range: ": ".len() + new_type_label.len() + "<".len()
                    ..": ".len() + new_type_label.len() + "<".len() + struct_label.len(),
            }),
            "Popover range should match the struct label part"
        );
        assert_eq!(
            popover.get_rendered_text(cx),
            format!("A tooltip for {struct_label}"),
            "Rendered markdown element should remove backticks from text"
        );
    });
}

#[test]
fn test_find_hovered_hint_part_with_multibyte_characters() {
    use crate::display_map::InlayOffset;
    use multi_buffer::MultiBufferOffset;
    use project::InlayHintLabelPart;

    // Test with multi-byte UTF-8 character "→" (3 bytes, 1 character)
    let label = "→ app/Livewire/UserProfile.php";
    let label_parts = vec![InlayHintLabelPart {
        value: label.to_string(),
        tooltip: None,
        location: None,
    }];

    let hint_start = InlayOffset(MultiBufferOffset(100));

    // Verify the label has more bytes than characters (due to "→")
    assert_eq!(label.len(), 32); // bytes
    assert_eq!(label.chars().count(), 30); // characters

    // Test hovering at the last byte (should find the part)
    let last_byte_offset = InlayOffset(MultiBufferOffset(100 + label.len() - 1));
    let result = find_hovered_hint_part(label_parts.clone(), hint_start, last_byte_offset);
    assert!(
        result.is_some(),
        "Should find part when hovering at last byte"
    );
    let (part, range) = result.unwrap();
    assert_eq!(part.value, label);
    assert_eq!(range.start, hint_start);
    assert_eq!(range.end, InlayOffset(MultiBufferOffset(100 + label.len())));

    // Test hovering at the first byte of "→" (byte 0)
    let first_byte_offset = InlayOffset(MultiBufferOffset(100));
    let result = find_hovered_hint_part(label_parts.clone(), hint_start, first_byte_offset);
    assert!(
        result.is_some(),
        "Should find part when hovering at first byte"
    );

    // Test hovering in the middle of "→" (byte 1, still part of the arrow character)
    let mid_arrow_offset = InlayOffset(MultiBufferOffset(101));
    let result = find_hovered_hint_part(label_parts, hint_start, mid_arrow_offset);
    assert!(
        result.is_some(),
        "Should find part when hovering in middle of multi-byte char"
    );

    // Test with multiple parts containing multi-byte characters
    // Part ranges are [start, end) - start inclusive, end exclusive
    // "→ " occupies bytes [0, 4), "path" occupies bytes [4, 8)
    let parts = vec![
        InlayHintLabelPart {
            value: "→ ".to_string(), // 4 bytes (3 + 1)
            tooltip: None,
            location: None,
        },
        InlayHintLabelPart {
            value: "path".to_string(), // 4 bytes
            tooltip: None,
            location: None,
        },
    ];

    // Hover at byte 3 (last byte of "→ ", the space character)
    let arrow_last_byte = InlayOffset(MultiBufferOffset(100 + 3));
    let result = find_hovered_hint_part(parts.clone(), hint_start, arrow_last_byte);
    assert!(result.is_some(), "Should find first part at its last byte");
    let (part, range) = result.unwrap();
    assert_eq!(part.value, "→ ");
    assert_eq!(
        range,
        InlayOffset(MultiBufferOffset(100))..InlayOffset(MultiBufferOffset(104))
    );

    // Hover at byte 4 (first byte of "path", at the boundary)
    let path_start_offset = InlayOffset(MultiBufferOffset(100 + 4));
    let result = find_hovered_hint_part(parts.clone(), hint_start, path_start_offset);
    assert!(result.is_some(), "Should find second part at boundary");
    let (part, _) = result.unwrap();
    assert_eq!(part.value, "path");

    // Hover at byte 7 (last byte of "path")
    let path_end_offset = InlayOffset(MultiBufferOffset(100 + 7));
    let result = find_hovered_hint_part(parts, hint_start, path_end_offset);
    assert!(result.is_some(), "Should find second part at last byte");
    let (part, range) = result.unwrap();
    assert_eq!(part.value, "path");
    assert_eq!(
        range,
        InlayOffset(MultiBufferOffset(104))..InlayOffset(MultiBufferOffset(108))
    );
}
