use super::*;

#[gpui::test]
async fn test_click_on_parameter_inlay_hint_places_cursor_correctly(cx: &mut TestAppContext) {
    use crate::inlays::inlay_hints::tests::{cached_hint_labels, visible_hint_labels};

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            inlay_hint_provider: Some(lsp::OneOf::Left(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, &|settings: &mut SettingsContent| {
                settings.project.all_languages.defaults.inlay_hints =
                    Some(InlayHintSettingsContent {
                        enabled: Some(true),
                        show_parameter_hints: Some(true),
                        show_type_hints: Some(true),
                        edit_debounce_ms: Some(0),
                        scroll_debounce_ms: Some(0),
                        ..Default::default()
                    })
            });
        });
    });

    cx.set_state("fn foo(value: i32) {} fn main() { foo(ˇ42); }");

    // Buffer: `fn foo(value: i32) {} fn main() { foo(42); }`
    // The parameter hint "value:" appears before "42"
    let hint_start_offset = cx.ranges("fn foo(value: i32) {} fn main() { foo(ˇ42); }")[0].start;
    let hint_position = cx.to_lsp(MultiBufferOffset(hint_start_offset));
    let hint_label = "value:";
    let expected_uri = cx.buffer_lsp_url.clone();
    cx.lsp
        .set_request_handler::<lsp::request::InlayHintRequest, _, _>(move |params, _| {
            let expected_uri = expected_uri.clone();
            async move {
                assert_eq!(params.text_document.uri, expected_uri);
                Ok(Some(vec![lsp::InlayHint {
                    position: hint_position,
                    label: lsp::InlayHintLabel::String(hint_label.to_string()),
                    kind: Some(lsp::InlayHintKind::PARAMETER),
                    text_edits: None,
                    tooltip: None,
                    padding_left: None,
                    padding_right: Some(true),
                    data: None,
                }]))
            }
        })
        .next()
        .await;
    cx.background_executor.run_until_parked();

    cx.update_editor(|editor, _window, cx| {
        let expected_labels = vec!["value: ".to_string()];
        assert_eq!(expected_labels, cached_hint_labels(editor, cx));
        assert_eq!(expected_labels, visible_hint_labels(editor, cx));
    });

    // The cursor is at `4` in `42`. The parameter hint "value: " appears just
    // before it in display space. We'll click a few characters to the left of
    // the cursor position to land inside the inlay hint text.
    let cursor_display_point = cx.update_editor(|editor, _window, cx| {
        editor
            .selections
            .newest_display(&editor.display_snapshot(cx))
            .head()
    });
    let cursor_pixel = cx.pixel_position_for(cursor_display_point);
    let em_width =
        cx.update_editor(|editor, _, _| editor.last_position_map.as_ref().unwrap().em_layout_width);
    // Click 3 characters to the left of the cursor, which lands inside the
    // "value: " inlay hint text.
    let click_position = gpui::Point {
        x: cursor_pixel.x - em_width * 3.0,
        y: cursor_pixel.y,
    };
    cx.simulate_click(click_position, Modifiers::none());
    cx.background_executor.run_until_parked();

    // The cursor should be placed after the `(`, at the `4` in `42`,
    // NOT before the `(`.
    cx.assert_editor_state("fn foo(value: i32) {} fn main() { foo(ˇ42); }");
}
